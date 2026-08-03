import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:logging/logging.dart';
import 'package:remote_logging/src/remote/auth_service.dart';

/// The public contract for a service that collects log records and sends them
/// to a remote server.
///
/// Consuming code (e.g. `RemoteLogging`) depends on this interface, allowing
/// the concrete `LogSenderService` to be replaced with a test double.
abstract interface class ILogSenderService {
  /// Enqueues a [LogRecord] to be sent to the server on the next cycle.
  void enqueue(LogRecord logRecord);

  /// Starts the periodic log-sending loop.
  ///
  /// Must be called before [flush] or [stop]. Calling [start] when remote
  /// logging is disabled in the config is a no-op.
  Future<void> start({
    required INetworkInvoker invoker,
    required AuthService authService,
  });

  /// Forces an immediate flush of the current log queue.
  ///
  /// No-ops if [start] has not been called.
  Future<void> flush();

  /// Stops the periodic log-sending loop.
  ///
  /// If [flush] is `true` (the default), the queue is drained before the
  /// loop is cancelled.
  Future<void> stop({bool flush = true});
}
