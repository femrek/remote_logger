import 'package:remote_logging/src/model/auth_exchange_request.dart';
import 'package:remote_logging/src/model/device_metadata.dart';
import 'package:test/test.dart';

void main() {
  group('AuthExchangeRequest', () {
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

    test('toJson contains all 5 required fields', () {
      const req = AuthExchangeRequest(
        platform: 'android',
        attestationToken: 'token',
        deviceId: 'device-123',
        deviceMetadata: metadata,
      );
      final json = req.toJsonPayload();

      expect(
        json.keys,
        containsAll([
          'platform',
          'attestation_token',
          'device_id',
          'device_metadata',
        ]),
      );
    });

    test('platform is "android" or "ios"', () {
      expect(
        () => AuthExchangeRequest(
          platform: 'windows',
          attestationToken: 'token',
          deviceId: 'device-123',
          deviceMetadata: metadata,
        ),
        throwsA(isA<AssertionError>()),
      );
    });

    test('device_metadata is a nested JSON object with all 11 fields', () {
      const req = AuthExchangeRequest(
        platform: 'android',
        attestationToken: 'token',
        deviceId: 'device-123',
        deviceMetadata: metadata,
      );
      final json = req.toJsonPayload();
      final deviceMetadataJson =
          json['device_metadata'] as Map<String, dynamic>;
      expect(deviceMetadataJson.keys.length, 11);
    });

    test('device_public_key is included when provided', () {
      const req = AuthExchangeRequest(
        platform: 'ios',
        attestationToken: 'token',
        deviceId: 'device-123',
        deviceMetadata: metadata,
        devicePublicKey: 'public-key',
      );
      final json = req.toJsonPayload();
      expect(json['device_public_key'], 'public-key');
    });

    test('device_public_key is omitted when null', () {
      const req = AuthExchangeRequest(
        platform: 'android',
        attestationToken: 'token',
        deviceId: 'device-123',
        deviceMetadata: metadata,
      );
      final json = req.toJsonPayload();
      expect(json.containsKey('device_public_key'), isFalse);
    });
  });
}
