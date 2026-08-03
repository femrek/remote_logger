import 'package:logging/logging.dart';
import 'package:meta/meta.dart';
import 'package:remote_logging/src/config/base_logging_config.dart';

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
    : _logger = Logger.detached('dev.femrek.remote_logging');

  static _InternalLogger? _instance;
  final Logger _logger;
}
