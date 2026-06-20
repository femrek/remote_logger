import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/remote/request/request_device_metadata.dart';
import 'package:remote_logging/src/remote/request/request_log_batch.dart';

/// A service that collects log records and periodically sends them to a remote
/// server. It also handles device metadata transmission and retries failed log
/// transmissions.
///
/// Queues log records and sends them in batches every 10 seconds. If a
/// transmission fails, it stores the failed requests and retries them in the
/// next cycle.
final class LogSenderService {
  /// Creates a [LogSenderService] with the given [config] and
  /// [deviceDataGenerator].
  LogSenderService({
    required this.config,
    required this.deviceDataGenerator,
  });

  /// Configuration for the remote logging service.
  final RemoteLoggingConfig config;

  /// Generator for device data to be included in log records.
  final IDeviceDataGenerator deviceDataGenerator;

  late final INetworkInvoker _invoker;
  late final DeviceMetadata _deviceMetadata;

  static const Duration _syncInterval = Duration(seconds: 10);

  final List<LogRecord> _queue = [];
  bool _coping = false;
  final Set<RequestLogBatch> _failedRequests = {};

  /// Enqueues a [LogRecord] to be sent to the server.
  void enqueue(LogRecord logRecord) {
    unawaited(
      Future.doWhile(() async {
        if (_coping) {
          await Future<void>.delayed(const Duration(milliseconds: 10));
          return true;
        } else {
          _queue.add(logRecord);
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
  }) async {
    localLog.config('Starting LogSenderService...');
    if (!config.enableRemoteLogging) {
      localLog.warning(
        'Remote logging is disabled. '
        'Logs will not be sent to remote endpoints.',
      );
      return;
    }

    _invoker = invoker;
    _deviceMetadata = deviceMetadata;

    localLog.config(
      () => 'Device metadata: ${_deviceMetadata.toJsonPayload()}',
    );

    await _sendDeviceMetadata();
    await _sendAll();
    return Stream<void>.periodic(
      _syncInterval,
    ).listen((_) async {
      await _resendFailedRequests();
      await _sendAll();
    }).asFuture();
  }

  Future<void> _sendDeviceMetadata() async {
    final request = RequestDeviceMetadata(
      metadata: _deviceMetadata,
      path: config.loggerEndpointDevice,
    );
    await _invoker.send(request);
  }

  Future<void> _sendAll() async {
    if (_queue.isEmpty) return;

    if (_coping) return;
    late final List<LogRecord> toSend;
    await Future.doWhile(() async {
      if (_coping) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
        return true;
      } else {
        _coping = true;
        toSend = List<LogRecord>.of(_queue);
        _queue.clear();
        _coping = false;
        return false;
      }
    });

    final request = RequestLogBatch(
      logs: toSend
          .map<ServerLog>(
            (log) => ServerLog.fromLogRecord(
              log,
              deviceDataGenerator,
            ),
          )
          .toList(),
      path: config.loggerEndpointLogBatch,
    );
    final response = await _invoker.send(request);
    switch (response) {
      case SuccessResponseResult<IgnoredSchema>():
        localLog.finer(
          () => 'Received response for log batch request: ${request.hashCode}',
        );
      case NetworkErrorResult<IgnoredSchema>():
        _failedRequests.add(request);
        localLog.warning(
          'Network error while sending log batch request: ${request.hashCode}',
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

  Future<void> _resendFailedRequests() async {
    if (_failedRequests.isEmpty) return;
    localLog.info(
      'Resending ${_failedRequests.length} failed log batch requests',
    );

    // Make a copy to iterate safely
    final toRetry = Set<RequestLogBatch>.from(_failedRequests);

    for (final request in toRetry) {
      final response = await _invoker.send(request);
      switch (response) {
        case SuccessResponseResult<IgnoredSchema>():
          _failedRequests.remove(request);
          localLog.fine(
            'Successfully resent log batch request: ${request.hashCode}',
          );
        case NetworkErrorResult<IgnoredSchema>():
          localLog.fine(
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
      localLog.info(
        'All failed log batch requests have been resent successfully',
      );
    } else {
      localLog.warning(
        '${_failedRequests.length} log batch requests '
        'still failed after resend attempts',
      );
    }
  }
}
