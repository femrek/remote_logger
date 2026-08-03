import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/util/log_record_extension.dart';

/// Base configuration class for logging setups.
///
/// Contains common options for console log levels, internal library log levels,
/// and custom log printers.
class BaseLoggingConfig {
  /// Creates a new [BaseLoggingConfig] instance.
  const BaseLoggingConfig({
    required this.logPrintLevel,
    required this.remoteLoggingLibraryLogPrintLevel,
    this.logPrinter = _printRecord,
  });

  /// The minimum log level for console output.
  ///
  /// Logs with a level below this threshold will not be printed to console.
  /// Defaults to [Level.FINEST].
  final Level logPrintLevel;

  /// The minimum log level for this library internal logs.
  ///
  /// Logs with a level below this threshold will not be stored locally.
  /// Defaults to [Level.CONFIG].
  final Level remoteLoggingLibraryLogPrintLevel;

  /// The printer function used to format and print log records to console.
  ///
  /// Sending logs never uses this function.
  final LogPrinter logPrinter;
}

/// Prints a [LogRecord] to the console with formatted output.
void _printRecord(LogRecord record) {
  print(
    '${record.time.toIso8601String()} '
    '[${record.zoneName}] '
    '[${_s7(record.level.name)}] '
    '${record.loggerName}: '
    '${record.message}',
  );

  if (record.error != null) {
    print('ERROR: ${record.error}');
  }
  if (record.stackTrace != null) {
    print('${record.stackTrace}');
  }
}

String _s7(String str) {
  if (str.length >= 7) {
    return str;
  } else {
    return str + ' ' * (7 - str.length);
  }
}
