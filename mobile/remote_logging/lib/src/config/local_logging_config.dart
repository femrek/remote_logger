import 'package:logging/logging.dart';
import 'package:remote_logging/src/config/base_logging_config.dart';

/// Configuration class for local-only logging operations.
///
/// Extends [BaseLoggingConfig] with default logging settings suitable
/// for local logging without remote transport.
class LocalLoggingConfig extends BaseLoggingConfig {
  /// Creates a new [LocalLoggingConfig] instance with optional parameters.
  const LocalLoggingConfig({
    super.logPrintLevel = Level.FINEST,
    super.remoteLoggingLibraryLogPrintLevel = Level.CONFIG,
    super.logPrinter,
  });

  /// Creates a copy of this configuration with the given fields replaced by
  /// the new values.
  LocalLoggingConfig copyWith({
    Level? logPrintLevel,
    Level? localLogPrintLevel,
  }) {
    return LocalLoggingConfig(
      logPrintLevel: logPrintLevel ?? this.logPrintLevel,
      remoteLoggingLibraryLogPrintLevel:
          localLogPrintLevel ?? remoteLoggingLibraryLogPrintLevel,
    );
  }

  @override
  String toString() {
    return 'LocalLoggingConfig('
        'logPrintLevel: $logPrintLevel, '
        'localLogPrintLevel: $remoteLoggingLibraryLogPrintLevel'
        ')';
  }
}
