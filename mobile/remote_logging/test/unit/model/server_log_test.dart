import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

void main() {
  group('ServerLog', () {
    test('toJsonPayload contains all 5 required keys', () {
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
      );
      final json = log.toJsonPayload();

      expect(
        json.keys,
        containsAll([
          'timestamp',
          'loggerName',
          'message',
          'level',
          'zone',
        ]),
      );
      expect(json['loggerName'], 'TestLogger');
    });

    test('toJsonPayload does NOT contain sessionId or deviceUUID', () {
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
      );
      final json = log.toJsonPayload();

      expect(json.containsKey('sessionId'), isFalse);
      expect(json.containsKey('deviceUUID'), isFalse);
    });

    test('timestamp is serialised as UTC ISO 8601 string', () {
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime(2023, 1, 1, 12),
        // Local time
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
      );
      final json = log.toJsonPayload();

      expect(json['timestamp'], endsWith('Z'));
      expect(
        () => DateTime.parse(json['timestamp'] as String),
        returnsNormally,
      );
    });

    test('error is included as String when present', () {
      final exception = Exception('test exception');
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
        error: exception,
      );
      final json = log.toJsonPayload();

      expect(json['error'], exception.toString());
      expect(json['error'], isA<String>());
    });

    test('stacktrace is included as String when present', () {
      final stacktrace = StackTrace.fromString('test stacktrace');
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
        stacktrace: stacktrace.toString(),
      );
      final json = log.toJsonPayload();

      expect(json['stacktrace'], stacktrace.toString());
      expect(json['stacktrace'], isA<String>());
    });

    test('error and stacktrace are omitted when null', () {
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
      );
      final json = log.toJsonPayload();

      expect(json.containsKey('error'), isFalse);
      expect(json.containsKey('stacktrace'), isFalse);
    });

    test(
      'fromLogRecord maps all fields from LogRecord',
      () {
        final record = LogRecord(
          Level.INFO,
          'Test message',
          'TestLogger',
          Exception('test exception'),
          StackTrace.fromString('test stacktrace'),
        );
        final log = ServerLog.fromLogRecord(
          record,
          const LogTruncationConfig(),
        );

        expect(log.loggerName, 'TestLogger');
        expect(log.level, 'INFO');
        expect(log.zone, 'no-zone');
        expect(log.error, isNotNull);
        expect(log.stacktrace, isNotNull);
      },
    );

    test('toJsonPayload produces no extra keys', () {
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
      );
      final json = log.toJsonPayload();

      final allowedKeys = {
        'timestamp',
        'loggerName',
        'message',
        'level',
        'zone',
        'error',
        'stacktrace',
      };

      for (final key in json.keys) {
        expect(
          allowedKeys.contains(key),
          isTrue,
          reason: 'Key $key should not be present',
        );
      }
    });
  });

  group('ServerLog truncation', () {
    test(
      'message is truncated when it exceeds maxMessageChars '
      '(append ...[truncated])',
      () {
        final record = LogRecord(Level.INFO, '1234567890', 'loggerName');
        const truncation = LogTruncationConfig(maxMessageChars: 5);
        final log = ServerLog.fromLogRecord(
          record,
          truncation,
        );

        expect(log.message, equals('12345...[truncated]'));
      },
    );

    test('message is unchanged when within limit', () {
      final record = LogRecord(Level.INFO, 'hello', 'loggerName');
      const truncation = LogTruncationConfig(maxMessageChars: 10);
      final log = ServerLog.fromLogRecord(
        record,
        truncation,
      );

      expect(log.message, equals('hello'));
    });

    test('message is not truncated when maxMessageChars is 0 (unlimited)', () {
      final record = LogRecord(Level.INFO, '1234567890', 'loggerName');
      const truncation = LogTruncationConfig(maxMessageChars: 0);
      final log = ServerLog.fromLogRecord(
        record,
        truncation,
      );

      expect(log.message, equals('1234567890'));
    });

    test('error string is truncated when it exceeds maxErrorChars', () {
      final record = LogRecord(
        Level.INFO,
        'msg',
        'log',
        Exception('1234567890'),
      );
      const truncation = LogTruncationConfig(maxErrorChars: 10);
      final log = ServerLog.fromLogRecord(
        record,
        truncation,
      );

      expect(log.error, equals('Exception:...[truncated]'));
    });

    test(
      'stacktrace is capped to maxStacktraceLines lines '
      '(excess lines dropped, ...[truncated] appended as extra line)',
      () {
        final record = LogRecord(
          Level.INFO,
          'msg',
          'log',
          null,
          StackTrace.fromString('line1\nline2\nline3\nline4'),
        );
        const truncation = LogTruncationConfig(maxStacktraceLines: 2);
        final log = ServerLog.fromLogRecord(
          record,
          truncation,
        );

        expect(log.stacktrace, equals('line1\nline2\n...[truncated]'));
      },
    );

    test(
      'each stacktrace line is truncated to maxStacktraceLineChars '
      '(with ...[truncated] suffix)',
      () {
        final record = LogRecord(
          Level.INFO,
          'msg',
          'log',
          null,
          StackTrace.fromString('line1_very_long\nline2_very_long'),
        );
        const truncation = LogTruncationConfig(maxStacktraceLineChars: 5);
        final log = ServerLog.fromLogRecord(
          record,
          truncation,
        );

        expect(
          log.stacktrace,
          equals('line1...[truncated]\nline2...[truncated]'),
        );
      },
    );

    test('both stacktrace limits applied together', () {
      final record = LogRecord(
        Level.INFO,
        'msg',
        'log',
        null,
        StackTrace.fromString(
          'line1_very_long\nline2_very_long\nline3_very_long',
        ),
      );
      const truncation = LogTruncationConfig(
        maxStacktraceLines: 2,
        maxStacktraceLineChars: 5,
      );
      final log = ServerLog.fromLogRecord(
        record,
        truncation,
      );

      expect(
        log.stacktrace,
        equals('line1...[truncated]\nline2...[truncated]\n...[truncated]'),
      );
    });

    test('charCount reflects post-truncation lengths', () {
      final record = LogRecord(
        Level.INFO,
        '1234567890',
        'log',
        Exception('1234567890'),
        StackTrace.fromString('line1_very_long\nline2_very_long'),
      );
      const truncation = LogTruncationConfig(
        maxMessageChars: 5,
        maxErrorChars: 10,
        maxStacktraceLineChars: 5,
      );
      final log = ServerLog.fromLogRecord(
        record,
        truncation,
      );

      final expectedCharCount =
          log.message.length +
          log.error!.toString().length +
          log.stacktrace!.length;
      expect(log.charCount, equals(expectedCharCount));
    });

    test(
      'charCount is 0 when error and stacktrace are null '
      '(only message length)',
      () {
        final recordNoMsg = LogRecord(Level.INFO, '', 'log');
        final logNoMsg = ServerLog.fromLogRecord(
          recordNoMsg,
          const LogTruncationConfig(),
        );

        expect(logNoMsg.charCount, equals(0));

        final record = LogRecord(Level.INFO, 'the message', 'log');
        final log = ServerLog.fromLogRecord(
          record,
          const LogTruncationConfig(),
        );

        expect(log.charCount, equals('the message'.length));
      },
    );
  });
}
