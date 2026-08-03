import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/remote/auth_service.dart';
import 'package:remote_logging/src/remote/request/request_log_batch.dart';
import 'package:remote_logging/src/util/internal_logger.dart';

/// A service that collects log records and periodically sends them to a remote
/// server. It also handles device metadata transmission and retries failed log
/// transmissions.
///
/// Queues log records and sends them in batches every 10 seconds. If a
/// transmission fails, it stores the failed requests and retries them in the
/// next cycle. Optionally stores unsent logs locally using
/// [ILocalLogRepository].
final class LogSenderService {
  /// Creates a [LogSenderService] with the given [config] and
  /// [deviceDataGenerator].
  LogSenderService({
    required this.config,
    required this.deviceDataGenerator,
    this.localRepository,
    OnError? onError,
  }) : _onError = onError;

  /// Configuration for the remote logging service.
  final RemoteLoggingConfig config;

  /// Generator for device data to be included in log records.
  final IDeviceDataGenerator deviceDataGenerator;

  /// Optional local storage for unsent logs to prevent loss on crash/offline.
  final ILocalLogRepository? localRepository;

  /// Optional error listener.
  final OnError? _onError;

  late final INetworkInvoker _invoker;
  late final DeviceMetadata _deviceMetadata;
  late final AuthService _authService;

  static const Duration _syncInterval = Duration(seconds: 10);

  final List<ServerLog> _queue = [];
  bool _coping = false;
  final Set<RequestLogBatch> _failedRequests = {};

  /// Enqueues a [LogRecord] to be sent to the server.
  void enqueue(LogRecord logRecord) {
    final serverLog = ServerLog.fromLogRecord(logRecord, deviceDataGenerator);

    unawaited(
      localRepository?.saveLogs([serverLog]).catchError((
        Object e,
        StackTrace s,
      ) {
        internalLog.warning('Failed to save log locally', e, s);
      }),
    );

    unawaited(
      Future.doWhile(() async {
        if (_coping) {
          await Future<void>.delayed(const Duration(milliseconds: 10));
          return true;
        } else {
          _queue.add(serverLog);
          return false;
        }
      }),
    );
  }

  /// Starts the log sending service with the provided [invoker] and
  /// [deviceMetadata].
  Future<void> start({
    required INetworkInvoker invoker,
    required DeviceMetadata deviceMetadata,
    required AuthService authService,
  }) async {
    internalLog.config('Starting LogSenderService...');
    if (!config.enableRemoteLogging) {
      internalLog.warning(
        'Remote logging is disabled. '
        'Logs will not be sent to remote endpoints.',
      );
      return;
    }

    _invoker = invoker;
    _deviceMetadata = deviceMetadata;
    _authService = authService;

    internalLog.config(
      () => 'Device metadata: ${_deviceMetadata.toJsonPayload()}',
    );

    final localRepository = this.localRepository;
    if (localRepository != null) {
      try {
        final unsent = await localRepository.getUnsentLogs();
        if (unsent.isNotEmpty) {
          internalLog.info(
            'Loaded ${unsent.length} unsent logs from local repository',
          );
          _queue.insertAll(0, unsent);
        }
      } on Object catch (e, s) {
        internalLog.warning('Failed to load unsent logs', e, s);
      }
    }

    await _sendAll();
    return Stream<void>.periodic(_syncInterval).listen((_) async {
      await _resendFailedRequests();
      await _sendAll();
    }).asFuture();
  }

  /// Forces an immediate flush of the log queue.
  Future<void> flush() async {
    await _sendAll();
  }

  Future<void> _sendAll() async {
    if (_queue.isEmpty) return;

    if (_coping) return;
    late final List<ServerLog> toSend;
    await Future.doWhile(() async {
      if (_coping) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
        return true;
      } else {
        _coping = true;
        toSend = List<ServerLog>.of(_queue);
        _queue.clear();
        _coping = false;
        return false;
      }
    });

    final request = RequestLogBatch(
      logs: toSend,
      path: config.path,
      token: await _authService.getValidAccessToken(),
    );
    final response = await _invoker.send(request);
    switch (response) {
      case SuccessResponseResult():
        internalLog.finer(
          () => 'Received response for log batch request: ${request.hashCode}',
        );
        _removeLocalLogs(toSend);
      case NetworkErrorResult():
        _failedRequests.add(request);
        internalLog.warning(
          'Network error while sending log batch request: ${request.hashCode}',
          response.error,
          response.error.stackTrace,
        );
        _onError?.call(response.error, response.error.stackTrace);
      // If it's a 401, token might be expired. Handle it if needed by force
      // refreshing token on next try.
      // For now, it will be added to failed requests and retried.
      case SpecifiedResponseResult(:final data, :final type):
        final err = Exception(
          'Unexpected response type for log batch request: $type, data: $data',
        );
        internalLog.severe(
          'Unexpected response type for log batch request: ${request.hashCode}',
          err,
          StackTrace.current,
        );

        throw err;
    }
  }

  Future<void> _resendFailedRequests() async {
    if (_failedRequests.isEmpty) return;
    internalLog.info(
      'Resending ${_failedRequests.length} failed log batch requests',
    );

    // Make a copy to iterate safely
    final toRetry = Set<RequestLogBatch>.from(_failedRequests);

    for (final request in toRetry) {
      final response = await _invoker.send(request);
      switch (response) {
        case SuccessResponseResult<IgnoredSchema>():
          _failedRequests.remove(request);
          internalLog.fine(
            'Successfully resent log batch request: ${request.hashCode}',
          );
          _removeLocalLogs(request.logs);
        case NetworkErrorResult<IgnoredSchema>():
          internalLog.fine(
            () => 'Failed to resend log batch request: ${request.hashCode}',
            response.error,
            response.error.stackTrace,
          );
        case SpecifiedResponseResult<IgnoredSchema>():
          throw Exception(
            'Unexpected response type for log batch request: '
            '${response.runtimeType}',
          );
      }
    }

    if (_failedRequests.isEmpty) {
      internalLog.info(
        'All failed log batch requests have been resent successfully',
      );
    } else {
      internalLog.warning(
        '${_failedRequests.length} log batch requests '
        'still failed after resend attempts',
      );
    }
  }

  void _removeLocalLogs(List<ServerLog> logs) {
    final ids = logs.map((l) => l.id).toList();
    unawaited(
      localRepository?.removeLogs(ids).catchError((Object e, StackTrace s) {
        internalLog.warning(
          'Failed to remove sent logs from local repo',
          e,
          s,
        );
      }),
    );
  }
}
