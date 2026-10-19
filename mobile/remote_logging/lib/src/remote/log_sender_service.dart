import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/remote/auth_service.dart';
import 'package:remote_logging/src/remote/request/request_log_batch.dart';
import 'package:remote_logging/src/util/internal_logger.dart';

/// A service that collects log records and periodically sends them to a remote
/// server. It also handles retries for failed log transmissions.
///
/// Queues log records and sends them in batches every 10 seconds. If a
/// transmission fails, it stores the failed request and retries it in the next
/// cycle. The total number of pending logs across all failed batches is capped
/// by [RemoteLoggingConfig.maxPendingLogCount] to prevent unbounded growth
/// during offline periods (FIFO eviction). Optionally stores unsent logs
/// locally using [LocalLogRepository].
final class LogSenderService {
  /// Creates a [LogSenderService] with the given [config].
  LogSenderService({
    required RemoteLoggingConfig config,
    DeviceDataGenerator? deviceDataGenerator,
    LocalLogRepository? localRepository,
    OnError? onError,
  }) : _config = config,
       _deviceDataGenerator = deviceDataGenerator,
       _localRepository = localRepository,
       _onError = onError;

  final RemoteLoggingConfig _config;
  final DeviceDataGenerator? _deviceDataGenerator;
  final LocalLogRepository? _localRepository;
  final OnError? _onError;

  late final INetworkInvoker _invoker;
  late final AuthService _authService;

  static const Duration _syncInterval = Duration(seconds: 10);

  final List<ServerLog> _queue = [];

  // Completer-based mutex: prevents concurrent queue mutations.
  // Safe on Dart's single-threaded event loop — no two awaits interleave.
  Completer<void>? _copyLock;

  // Ordered list of failed batches — maintained as a queue for FIFO
  // eviction.
  final List<RequestLogBatch> _failedBatches = [];

  StreamSubscription<void>? _subscription;
  bool _isStarted = false;

  /// Enqueues a [LogRecord] to be sent to the server on the next cycle.
  void enqueue(LogRecord logRecord) {
    final serverLog = ServerLog.fromLogRecord(
      logRecord,
      _config.truncation,
      sessionId: _deviceDataGenerator?.sessionId,
    );

    unawaited(
      _localRepository?.saveLogs([serverLog]).catchError((
        Object e,
        StackTrace s,
      ) {
        internalLog.warning('Failed to save log locally', e, s);
      }),
    );

    unawaited(_withCopyLock(() => _queue.add(serverLog)));
  }

  /// Starts the periodic log-sending loop.
  ///
  /// Must be called before [flush] or [stop]. Calling [start] when remote
  /// logging is disabled in the config is a no-op.
  ///
  /// It is designed to run until [stop] is called.
  Future<void> start({
    required INetworkInvoker invoker,
    required AuthService authService,
  }) async {
    internalLog.config('Starting LogSenderService...');
    if (!_config.enableRemoteLogging) {
      internalLog.warning(
        'Remote logging is disabled. '
        'Logs will not be sent to remote endpoints.',
      );
      return;
    }

    _invoker = invoker;
    _authService = authService;
    _isStarted = true;

    final localRepository = _localRepository;
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

    _subscription = Stream<void>.periodic(_syncInterval).listen((_) async {
      try {
        await _resendFailedRequests();
        await _sendAll();
      } on Object catch (e, s) {
        internalLog.severe('Unexpected error in log send cycle', e, s);
        _onError?.call(e, s);
      }
    });
  }

  /// Forces an immediate flush of the current log queue.
  ///
  /// No-ops if [start] has not been called.
  ///
  /// Flushes any pending logs immediately. Useful for testing or when shutting
  /// down.
  Future<void> flush() async {
    if (!_isStarted) return;
    await _resendFailedRequests();
    await _sendAll();
  }

  /// Stops the periodic log-sending loop.
  ///
  /// Stops the log sender service and resets the initialization state so
  /// [RemoteLoggingInit.initRemote] can be called again if needed.
  ///
  /// If [flush] is `true` (the default), any pending logs in the queue are
  /// sent before the service stops.
  Future<void> stop({bool flush = true}) async {
    if (flush) await this.flush();
    await _subscription?.cancel();
    _subscription = null;
  }

  // ---------------------------------------------------------------------------
  // Internal helpers
  // ---------------------------------------------------------------------------

  /// Atomically drains the queue and returns the snapshot.
  Future<List<ServerLog>> _drainQueue() async {
    while (_copyLock != null) {
      await _copyLock!.future;
    }
    _copyLock = Completer();
    final items = List<ServerLog>.of(_queue);
    _queue.clear();
    _copyLock!.complete();
    _copyLock = null;
    return items;
  }

  /// Runs [action] while holding the copy lock.
  Future<void> _withCopyLock(void Function() action) async {
    while (_copyLock != null) {
      await _copyLock!.future;
    }
    _copyLock = Completer();
    action();
    _copyLock!.complete();
    _copyLock = null;
  }

  Future<void> _sendAll() async {
    if (_queue.isEmpty) return;

    final String? token;
    try {
      token = await _authService.getValidAccessToken();
    } on Object catch (e, s) {
      internalLog.warning(
        'Failed to acquire access token before sending logs',
        e,
        s,
      );
      _onError?.call(e, s);
      return;
    }

    final toSend = await _drainQueue();
    if (toSend.isEmpty) return;

    final request = RequestLogBatch(
      logs: toSend,
      path: _config.path,
      token: token,
    );
    final response = await _invoker.send(request);
    switch (response) {
      case SuccessResponseResult():
        internalLog.finer(
          () => 'Received response for log batch request: ${request.hashCode}',
        );
        _removeLocalLogs(toSend);
      case NetworkErrorResult():
        _addFailedBatch(request);
        internalLog.warning(
          'Network error while sending log batch request: ${request.hashCode}',
          response.error,
          response.error.stackTrace,
        );
        _onError?.call(response.error, response.error.stackTrace);
      case SpecifiedResponseResult(:final data, :final type):
        _addFailedBatch(request);
        final err = Exception(
          'Unexpected response type for log batch request: $type, data: $data',
        );
        internalLog.severe(
          'Unexpected response type for log batch request: ${request.hashCode}',
          err,
          StackTrace.current,
        );
        _onError?.call(err, StackTrace.current);
    }
  }

  Future<void> _resendFailedRequests() async {
    if (_failedBatches.isEmpty) return;
    internalLog.info(
      'Resending ${_failedBatches.length} failed log batch requests',
    );

    final String? token;
    try {
      token = await _authService.getValidAccessToken();
    } on Object catch (e, s) {
      internalLog.fine(
        () => 'Cannot resend failed batches: unable to acquire token',
        e,
        s,
      );
      return;
    }

    // Copy to iterate safely while mutating _failedBatches.
    final toRetry = List<RequestLogBatch>.of(_failedBatches);

    for (final original in toRetry) {
      // Rebuild with a fresh token — original token may be expired.
      final request = RequestLogBatch(
        logs: original.logs,
        path: original.path,
        token: token,
      );

      final response = await _invoker.send(request);
      switch (response) {
        case SuccessResponseResult<IgnoredSchema>():
          _failedBatches.remove(original);
          internalLog.fine(
            'Successfully resent log batch request: ${request.hashCode}',
          );
          _removeLocalLogs(original.logs);
        case NetworkErrorResult<IgnoredSchema>():
          internalLog.fine(
            () => 'Failed to resend log batch request: ${request.hashCode}',
            response.error,
            response.error.stackTrace,
          );
        case SpecifiedResponseResult<IgnoredSchema>():
          final err = Exception(
            'Unexpected response type during retry: ${response.runtimeType}',
          );
          internalLog.severe(
            'Unexpected response type during retry: ${request.hashCode}',
            err,
            StackTrace.current,
          );
          _onError?.call(err, StackTrace.current);
      }
    }

    if (_failedBatches.isEmpty) {
      internalLog.info(
        'All failed log batch requests have been resent successfully',
      );
    } else {
      internalLog.warning(
        '${_failedBatches.length} log batch requests '
        'still failed after resend attempts',
      );
    }
  }

  /// Adds a failed batch, enforcing [RemoteLoggingConfig.maxPendingLogCount].
  ///
  /// Evicts the oldest batch (FIFO) until adding [batch] would not exceed the
  /// configured limit. If the limit is 0, no eviction is applied.
  void _addFailedBatch(RequestLogBatch batch) {
    final max = _config.maxPendingLogCount;
    if (max <= 0) {
      _failedBatches.add(batch);
      return;
    }

    var pendingCount = _failedBatches.fold<int>(
      0,
      (sum, b) => sum + b.logs.length,
    );

    // Evict oldest batches until there is room.
    while (_failedBatches.isNotEmpty &&
        pendingCount + batch.logs.length > max) {
      final evicted = _failedBatches.removeAt(0);
      pendingCount -= evicted.logs.length;
      internalLog.warning(
        'Evicted oldest failed batch (${evicted.logs.length} logs) '
        'because maxPendingLogCount ($max) would be exceeded.',
      );
    }

    _failedBatches.add(batch);
  }

  void _removeLocalLogs(List<ServerLog> logs) {
    final ids = logs.map((l) => l.id).toList();
    unawaited(
      _localRepository?.removeLogs(ids).catchError((Object e, StackTrace s) {
        internalLog.warning(
          'Failed to remove sent logs from local repo',
          e,
          s,
        );
      }),
    );
  }
}
