import 'dart:async';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

class MockDeviceDataGenerator implements IDeviceDataGenerator {
  @override
  String get deviceId => 'test-device-uuid';

  @override
  String get sessionId => 'test-session-id';

  @override
  DeviceMetadata get deviceMetadata => const DeviceMetadata(
    timestamp: '2023-01-01T12:00:00Z',
    deviceId: 'test-device-uuid',
    sessionId: 'test-session-id',
    os: 'test-os',
    osVersion: 'test-os-version',
    model: 'test-model',
    brand: 'test-brand',
    device: 'test-device',
    identifier: 'test-identifier',
    manufacturer: 'test-manufacturer',
    isPhysicalDevice: 'true',
  );

  @override
  Future<void> init() async {}
}

void main() {
  group('ServerLog', () {
    test('toJsonPayload contains all 6 required keys', () {
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
        deviceUUID: 'device-123',
        sessionId: 'session-456',
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
          'deviceUUID',
        ]),
      );
      expect(json['loggerName'], 'TestLogger');
    });

    test('toJsonPayload does NOT contain sessionId', () {
      final log = ServerLog(
        id: 'test-id',
        timestamp: DateTime.utc(2023),
        loggerName: 'TestLogger',
        message: 'Test message',
        level: 'INFO',
        zone: 'test-zone',
        deviceUUID: 'device-123',
        sessionId: 'session-456',
      );
      final json = log.toJsonPayload();

      expect(json.containsKey('sessionId'), isFalse);
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
        deviceUUID: 'device-123',
        sessionId: 'session-456',
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
        deviceUUID: 'device-123',
        sessionId: 'session-456',
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
        deviceUUID: 'device-123',
        sessionId: 'session-456',
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
        deviceUUID: 'device-123',
        sessionId: 'session-456',
      );
      final json = log.toJsonPayload();

      expect(json.containsKey('error'), isFalse);
      expect(json.containsKey('stacktrace'), isFalse);
    });

    test(
      'fromLogRecord maps all fields from LogRecord + IDeviceDataGenerator',
      () {
        final record = LogRecord(
          Level.INFO,
          'Test message',
          'TestLogger',
          Exception('test exception'),
          StackTrace.fromString('test stacktrace'),
        );
        final generator = MockDeviceDataGenerator();
        final log = ServerLog.fromLogRecord(record, generator);

        expect(log.loggerName, 'TestLogger');
        expect(log.level, 'INFO');
        expect(log.zone, 'no-zone');
        expect(log.deviceUUID, 'test-device-uuid');
        expect(log.sessionId, 'test-session-id');
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
        deviceUUID: 'device-123',
        sessionId: 'session-456',
      );
      final json = log.toJsonPayload();

      final allowedKeys = {
        'timestamp',
        'loggerName',
        'message',
        'level',
        'zone',
        'deviceUUID',
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
}
