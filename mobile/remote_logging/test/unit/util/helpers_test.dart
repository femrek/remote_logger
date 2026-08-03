import 'package:logging/logging.dart';
import 'package:remote_logging/src/util/helpers.dart';
import 'package:test/test.dart';

void main() {
  group('isLoggable', () {
    test('returns true when log level equals required level (boundary)', () {
      final log = LogRecord(Level.INFO, 'test message', 'logger');
      expect(isLoggable(log, Level.INFO), isTrue);
    });

    test('returns true when log level is greater than required level', () {
      final log = LogRecord(Level.SEVERE, 'test message', 'logger');
      expect(isLoggable(log, Level.INFO), isTrue);
    });

    test('returns false when log level is less than required level', () {
      final log = LogRecord(Level.FINE, 'test message', 'logger');
      expect(isLoggable(log, Level.INFO), isFalse);
    });
  });

  group('getBiggerScopeLevel', () {
    test('returns the lower-value level (wider scope) when levels differ', () {
      expect(getBiggerScopeLevel(Level.INFO, Level.FINE), equals(Level.FINE));
      expect(
        getBiggerScopeLevel(Level.SEVERE, Level.WARNING),
        equals(Level.WARNING),
      );
    });

    test('returns either level when both are equal', () {
      expect(getBiggerScopeLevel(Level.INFO, Level.INFO), equals(Level.INFO));
    });
  });
}
