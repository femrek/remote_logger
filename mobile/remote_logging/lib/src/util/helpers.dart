import 'package:logging/logging.dart';

/// Checks if the [log]'s level is greater than or equal to [requiredLevel].
bool isLoggable(LogRecord log, Level requiredLevel) {
  return log.level >= requiredLevel;
}

/// Compares two log levels and returns the level with the larger scope
/// (i.e., the lower value, meaning it includes more logs).
Level getBiggerScopeLevel(Level level1, Level level2) {
  return level1.value < level2.value ? level1 : level2;
}
