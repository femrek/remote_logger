import 'package:logging/logging.dart';

/// Configuration class for remote logging functionality.
///
/// This class encapsulates all configuration parameters required for setting up
/// and controlling the behavior of remote logging in the application.
///
/// Example usage:
/// ```dart
/// final config = RemoteLoggingConfig(
///   gatewayUrl: 'https://api.example.com',
///   channelId: 'my-channel',
///   enableRemoteLogging: true,
///   logSendLevel: Level.WARNING,
/// );
/// ```
class RemoteLoggingConfig {
  /// Creates a configuration instance for remote logging.
  ///
  /// Parameters:
  /// - [gatewayUrl]: The base HTTP endpoint URL for the remote logger gateway
  /// - [channelId]: The channel ID for logs
  /// - [devicePublicKey]: The device public key (optional)
  /// - [enableRemoteLogging]: Whether remote logging is enabled (default: true)
  /// - [logSendLevel]: The minimum log level for sending logs remotely
  /// (default: Level.INFO)
  /// - [logPrintLevel]: The minimum log level for console output (default:
  /// Level.FINEST)
  /// - [localLogPrintLevel]: The minimum log level for local logging (default:
  /// Level.FINEST)
  const RemoteLoggingConfig({
    required this.gatewayUrl,
    required this.channelId,
    this.devicePublicKey,
    this.enableRemoteLogging = true,
    this.logSendLevel = Level.INFO,
    this.logPrintLevel = Level.FINEST,
    this.localLogPrintLevel = Level.FINEST,
  });

  /// The base HTTP endpoint URL for the remote logger gateway
  final String gatewayUrl;

  /// The channel ID for logs
  final String channelId;

  /// The device public key (optional)
  final String? devicePublicKey;

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
    String? gatewayUrl,
    String? channelId,
    String? devicePublicKey,
    bool? enableRemoteLogging,
    Level? logSendLevel,
    Level? logPrintLevel,
    Level? localLogPrintLevel,
  }) {
    return RemoteLoggingConfig(
      gatewayUrl: gatewayUrl ?? this.gatewayUrl,
      channelId: channelId ?? this.channelId,
      devicePublicKey: devicePublicKey ?? this.devicePublicKey,
      enableRemoteLogging: enableRemoteLogging ?? this.enableRemoteLogging,
      logSendLevel: logSendLevel ?? this.logSendLevel,
      logPrintLevel: logPrintLevel ?? this.logPrintLevel,
      localLogPrintLevel: localLogPrintLevel ?? this.localLogPrintLevel,
    );
  }

  @override
  String toString() {
    return 'RemoteLoggingConfig('
        'gatewayUrl: $gatewayUrl, '
        'channelId: $channelId, '
        'devicePublicKey: $devicePublicKey, '
        'enableRemoteLogging: $enableRemoteLogging, '
        'logSendLevel: $logSendLevel, '
        'logPrintLevel: $logPrintLevel, '
        'localLogPrintLevel: $localLogPrintLevel'
        ')';
  }
}
