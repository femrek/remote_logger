import 'package:flutter/foundation.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';

@internal
final Logger internalLog = _InternalLogger()._logger;

@internal
void initInternalLogger(BaseLoggingConfig config) {
  internalLog.level = config.remoteLoggingLibraryLogPrintLevel;
  internalLog.onRecord.listen(config.logPrinter);
}

class _InternalLogger {
  factory _InternalLogger() => _instance ??= _InternalLogger._internal();

  _InternalLogger._internal()
    : _logger = Logger.detached('dev.femrek.flutter_remote_logging');

  static _InternalLogger? _instance;
  final Logger _logger;
}
