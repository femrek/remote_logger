import 'package:logging/logging.dart';

/// Configuration class for remote logging functionality.
///
/// This class encapsulates all configuration parameters required for setting up
/// and controlling the behavior of remote logging in the application. It
/// includes endpoints for device information, individual logs, and batch log
/// submissions, along with level controls for filtering and displaying logs.
///
/// Example usage:
/// ```dart
/// final config = RemoteLoggingConfig(
///   loggerEndpointDevice: 'https://api.example.com/device',
///   loggerEndpointLog: 'https://api.example.com/log',
///   loggerEndpointLogBatch: 'https://api.example.com/batch',
///   enableRemoteLogging: true,
///   logSendLevel: Level.WARNING,
/// );
/// ```
class RemoteLoggingConfig {
  /// Creates a configuration instance for remote logging.
  ///
  /// Parameters:
  /// - [loggerEndpointDevice]: The HTTP endpoint URL for sending device
  /// information
  /// - [loggerEndpointLog]: The HTTP endpoint URL for sending individual log
  /// entries
  /// - [loggerEndpointLogBatch]: The HTTP endpoint URL for sending batch log
  /// submissions
  /// - [enableRemoteLogging]: Whether remote logging is enabled (default: true)
  /// - [logSendLevel]: The minimum log level for sending logs remotely
  /// (default: Level.INFO)
  /// - [logPrintLevel]: The minimum log level for console output (default:
  /// Level.FINEST)
  /// - [localLogPrintLevel]: The minimum log level for local logging (default:
  /// Level.FINEST)
  const RemoteLoggingConfig({
    required this.loggerEndpointDevice,
    required this.loggerEndpointLog,
    required this.loggerEndpointLogBatch,
    this.enableRemoteLogging = true,
    this.logSendLevel = Level.INFO,
    this.logPrintLevel = Level.FINEST,
    this.localLogPrintLevel = Level.FINEST,
  });

  /// The HTTP endpoint URL for sending device information to the logging
  /// service.
  final String loggerEndpointDevice;

  /// The HTTP endpoint URL for sending individual log entries to the logging
  /// service.
  final String loggerEndpointLog;

  /// The HTTP endpoint URL for sending batch log submissions to the logging
  /// service.
  final String loggerEndpointLogBatch;

  /// Whether remote logging is enabled.
  ///
  /// When set to false, logs will not be sent to remote endpoints,
  /// but local logging and console output may still occur.
  final bool enableRemoteLogging;

  /// The minimum log level for sending logs to the remote logging service.
  ///
  /// Logs with a level below this threshold will not be sent remotely.
  /// Defaults to [Level.INFO].
  final Level logSendLevel;

  /// The minimum log level for console output.
  ///
  /// Logs with a level below this threshold will not be printed to console.
  /// Defaults to [Level.FINEST].
  final Level logPrintLevel;

  /// The minimum log level for local logging.
  ///
  /// Logs with a level below this threshold will not be stored locally.
  /// Defaults to [Level.FINEST].
  final Level localLogPrintLevel;

  /// Creates a copy of this configuration with the given fields replaced by
  /// the new values.
  RemoteLoggingConfig copyWith({
    String? loggerEndpointDevice,
    String? loggerEndpointLog,
    String? loggerEndpointLogBatch,
    bool? enableRemoteLogging,
    Level? logSendLevel,
    Level? logPrintLevel,
    Level? localLogPrintLevel,
  }) {
    return RemoteLoggingConfig(
      loggerEndpointDevice: loggerEndpointDevice ?? this.loggerEndpointDevice,
      loggerEndpointLog: loggerEndpointLog ?? this.loggerEndpointLog,
      loggerEndpointLogBatch:
          loggerEndpointLogBatch ?? this.loggerEndpointLogBatch,
      enableRemoteLogging: enableRemoteLogging ?? this.enableRemoteLogging,
      logSendLevel: logSendLevel ?? this.logSendLevel,
      logPrintLevel: logPrintLevel ?? this.logPrintLevel,
      localLogPrintLevel: localLogPrintLevel ?? this.localLogPrintLevel,
    );
  }

  @override
  String toString() {
    return 'RemoteLoggingConfig('
        'loggerEndpointDevice: $loggerEndpointDevice, '
        'loggerEndpointLog: $loggerEndpointLog, '
        'loggerEndpointLogBatch: $loggerEndpointLogBatch, '
        'enableRemoteLogging: $enableRemoteLogging, '
        'logSendLevel: $logSendLevel, '
        'logPrintLevel: $logPrintLevel, '
        'localLogPrintLevel: $localLogPrintLevel'
        ')';
  }
}
