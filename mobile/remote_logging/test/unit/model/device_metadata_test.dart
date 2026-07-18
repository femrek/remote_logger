import 'package:remote_logging/src/model/device_metadata.dart';
import 'package:test/test.dart';

void main() {
  group('DeviceMetadata', () {
    test('toJsonPayload contains all 11 required keys', () {
      const metadata = DeviceMetadata(
        timestamp: '2023-01-01T12:00:00Z',
        deviceId: 'device-123',
        sessionId: 'session-456',
        os: 'Android',
        osVersion: '13',
        model: 'Pixel 6',
        brand: 'Google',
        device: 'oriole',
        identifier: 'identifier-789',
        manufacturer: 'Google',
        isPhysicalDevice: 'true',
      );
      final json = metadata.toJsonPayload();

      expect(
        json.keys,
        containsAll([
          'timestamp',
          'deviceUUID',
          'sessionId',
          'os',
          'osVersion',
          'model',
          'brand',
          'device',
          'identifier',
          'manufacturer',
          'isPhysicalDevice',
        ]),
      );
    });

    test('deviceId field maps to JSON key "deviceUUID"', () {
      const metadata = DeviceMetadata(
        timestamp: '2023-01-01T12:00:00Z',
        deviceId: 'device-123',
        sessionId: 'session-456',
        os: 'Android',
        osVersion: '13',
        model: 'Pixel 6',
        brand: 'Google',
        device: 'oriole',
        identifier: 'identifier-789',
        manufacturer: 'Google',
        isPhysicalDevice: 'true',
      );
      final json = metadata.toJsonPayload();

      expect(json['deviceUUID'], 'device-123');
      expect(json.containsKey('deviceId'), isFalse);
    });

    test('isPhysicalDevice is serialised as String "true" or "false"', () {
      const metadata = DeviceMetadata(
        timestamp: '2023-01-01T12:00:00Z',
        deviceId: 'device-123',
        sessionId: 'session-456',
        os: 'Android',
        osVersion: '13',
        model: 'Pixel 6',
        brand: 'Google',
        device: 'oriole',
        identifier: 'identifier-789',
        manufacturer: 'Google',
        isPhysicalDevice: 'false',
      );
      final json = metadata.toJsonPayload();

      expect(json['isPhysicalDevice'], 'false');
      expect(json['isPhysicalDevice'], isA<String>());
    });

    test('toJsonPayload produces no extra keys', () {
      const metadata = DeviceMetadata(
        timestamp: '2023-01-01T12:00:00Z',
        deviceId: 'device-123',
        sessionId: 'session-456',
        os: 'Android',
        osVersion: '13',
        model: 'Pixel 6',
        brand: 'Google',
        device: 'oriole',
        identifier: 'identifier-789',
        manufacturer: 'Google',
        isPhysicalDevice: 'true',
      );
      final json = metadata.toJsonPayload();

      expect(json.keys.length, 11);
    });
  });
}
