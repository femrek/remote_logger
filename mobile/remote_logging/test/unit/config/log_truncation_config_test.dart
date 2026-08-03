import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

void main() {
  group('LogTruncationConfig', () {
    test('default maxMessageChars is 8192', () {
      const config = LogTruncationConfig();
      expect(config.maxMessageChars, equals(8192));
    });

    test('default maxErrorChars is 2048', () {
      const config = LogTruncationConfig();
      expect(config.maxErrorChars, equals(2048));
    });

    test('default maxStacktraceLines is 100', () {
      const config = LogTruncationConfig();
      expect(config.maxStacktraceLines, equals(100));
    });

    test('default maxStacktraceLineChars is 512', () {
      const config = LogTruncationConfig();
      expect(config.maxStacktraceLineChars, equals(512));
    });

    test('copyWith replaces specified fields and preserves others', () {
      const config = LogTruncationConfig(
        maxMessageChars: 1000,
        maxErrorChars: 500,
        maxStacktraceLines: 50,
        maxStacktraceLineChars: 256,
      );

      final updated = config.copyWith(
        maxMessageChars: 2000,
        maxStacktraceLines: 30,
      );

      expect(updated.maxMessageChars, equals(2000));
      expect(updated.maxErrorChars, equals(500));
      expect(updated.maxStacktraceLines, equals(30));
      expect(updated.maxStacktraceLineChars, equals(256));
    });

    test('toString contains all field values', () {
      const config = LogTruncationConfig(
        maxMessageChars: 1234,
        maxErrorChars: 5678,
        maxStacktraceLines: 90,
        maxStacktraceLineChars: 123,
      );

      final result = config.toString();
      expect(result, contains('maxMessageChars: 1234'));
      expect(result, contains('maxErrorChars: 5678'));
      expect(result, contains('maxStacktraceLines: 90'));
      expect(result, contains('maxStacktraceLineChars: 123'));
    });

    test('setting a value to 0 is valid (represents unlimited)', () {
      const config = LogTruncationConfig(
        maxMessageChars: 0,
        maxErrorChars: 0,
        maxStacktraceLines: 0,
        maxStacktraceLineChars: 0,
      );

      expect(config.maxMessageChars, equals(0));
      expect(config.maxErrorChars, equals(0));
      expect(config.maxStacktraceLines, equals(0));
      expect(config.maxStacktraceLineChars, equals(0));
    });
  });
}
