import 'package:logging/logging.dart';
import 'package:remote_logging/src/config/base_logging_config.dart';
import 'package:remote_logging/src/config/log_truncation_config.dart';

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
class RemoteLoggingConfig extends BaseLoggingConfig {
  /// Creates a configuration instance for remote logging.
  ///
  /// Parameters:
  /// - [gatewayUrl]: The base HTTP endpoint URL for the remote logger gateway.
  /// - [channelId]: The identifier used to route logs to the correct project or
  /// application in the remote server.
  /// - [appleDevicePublicKey]: An optional public key for the device, used
  /// only if the backend requires asymmetric encryption or cryptographic
  /// identity.
  /// - [enableRemoteLogging]: Whether remote logging is enabled (default: true)
  /// - [logSendLevel]: The minimum log level for sending logs remotely
  /// (default: Level.INFO)
  /// - [logPrintLevel]: The minimum log level for console output (default:
  /// Level.FINEST)
  /// - [remoteLoggingLibraryLogPrintLevel]: The minimum log level for local
  /// logging (default: Level.CONFIG)
  const RemoteLoggingConfig({
    this.gatewayUrl,
    this.channelId,
    this.appleDevicePublicKey,
    this.enableRemoteLogging = true,
    this.logSendLevel = Level.INFO,
    this.truncation = const LogTruncationConfig(),
    this.maxPendingLogCount = 500,
    super.logPrintLevel = Level.FINEST,
    super.remoteLoggingLibraryLogPrintLevel = Level.CONFIG,
    super.logPrinter,
  });

  /// The base HTTP endpoint URL for the remote logger gateway.
  final String? gatewayUrl;

  /// The identifier used to route logs to the correct project or application
  /// in the remote server.
  final String? channelId;

  /// An optional public key for the device.
  ///
  /// This is only required if your backend is configured to use asymmetric
  /// cryptography (e.g., end-to-end encryption or cryptographic device
  /// authentication). For most standard logging setups, this can be left null.
  final String? appleDevicePublicKey;

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

  /// Per-field truncation limits applied when converting a `LogRecord` to a
  /// `ServerLog`.
  ///
  /// Truncation happens at source so the queue, local storage, and retry
  /// buffer never hold oversized data. Defaults to [LogTruncationConfig]
  /// safe defaults (8 KB message, 2 KB error, 100 stacktrace lines).
  final LogTruncationConfig truncation;

  /// Maximum total number of `ServerLog` entries that may be held across all
  /// pending failed-retry batches.
  ///
  /// When adding a new failed batch would exceed this count, the oldest batch
  /// is evicted first (FIFO). Set to `0` to disable the cap.
  ///
  /// Default: `500`.
  final int maxPendingLogCount;

  /// Returns the gateway URL without a trailing slash.
  String? get gatewayUrlTrimmed {
    final gatewayUrl = this.gatewayUrl;
    if (gatewayUrl == null) return null;
    return gatewayUrl.endsWith('/')
        ? gatewayUrl.substring(0, gatewayUrl.length - 1)
        : gatewayUrl;
  }

  /// Returns the full path for sending log batches to the remote logging
  /// service.
  String get path => '$gatewayUrlTrimmed/v1/logs/$channelId/batch';

  /// Creates a copy of this configuration with the given fields replaced by
  /// the new values.
  RemoteLoggingConfig copyWith({
    String? gatewayUrl,
    String? channelId,
    String? appleDevicePublicKey,
    bool? enableRemoteLogging,
    Level? logSendLevel,
    LogTruncationConfig? truncation,
    int? maxPendingLogCount,
    Level? logPrintLevel,
    Level? localLogPrintLevel,
  }) {
    return RemoteLoggingConfig(
      gatewayUrl: gatewayUrl ?? this.gatewayUrl,
      channelId: channelId ?? this.channelId,
      appleDevicePublicKey: appleDevicePublicKey ?? this.appleDevicePublicKey,
      enableRemoteLogging: enableRemoteLogging ?? this.enableRemoteLogging,
      logSendLevel: logSendLevel ?? this.logSendLevel,
      truncation: truncation ?? this.truncation,
      maxPendingLogCount: maxPendingLogCount ?? this.maxPendingLogCount,
      logPrintLevel: logPrintLevel ?? this.logPrintLevel,
      remoteLoggingLibraryLogPrintLevel:
          localLogPrintLevel ?? remoteLoggingLibraryLogPrintLevel,
    );
  }

  @override
  String toString() {
    return 'RemoteLoggingConfig('
        'gatewayUrl: $gatewayUrl, '
        'channelId: $channelId, '
        'appleDevicePublicKey: $appleDevicePublicKey, '
        'enableRemoteLogging: $enableRemoteLogging, '
        'logSendLevel: $logSendLevel, '
        'truncation: $truncation, '
        'maxPendingLogCount: $maxPendingLogCount, '
        'logPrintLevel: $logPrintLevel, '
        'localLogPrintLevel: $remoteLoggingLibraryLogPrintLevel'
        ')';
  }
}
