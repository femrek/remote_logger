import 'package:flutter/services.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('FakeAttestationProvider', () {
    test('reports supported status and generates fake materials', () async {
      final fake = FakeAttestationProvider(supported: true);

      expect(await fake.isSupported(), isTrue);

      final material = await fake.enroll('challenge-123');
      expect(material.publicKey, isNotEmpty);
      expect(material.attestation, isNotEmpty);
      expect(fake.enrollCallCount, 1);

      final proof = await fake.prove('challenge-123', 'session-1', 'device-1');
      expect(proof, isNotEmpty);
      expect(fake.proveCallCount, 1);

      expect(await fake.loadEnrollmentToken(), isNull);
      await fake.saveEnrollmentToken('token-abc');
      expect(await fake.loadEnrollmentToken(), 'token-abc');

      await fake.reset();
      expect(await fake.loadEnrollmentToken(), isNull);
      expect(fake.resetCallCount, 1);
    });

    test('can be configured as unsupported', () async {
      final fake = FakeAttestationProvider(supported: false);
      expect(await fake.isSupported(), isFalse);
    });
  });

  group('PlatformAttestationProvider', () {
    const channel = MethodChannel(
      'dev.femrek.flutter_remote_logging/attestation',
    );
    final log = <MethodCall>[];
    String? storedToken;

    setUp(() {
      log.clear();
      storedToken = null;

      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (MethodCall methodCall) async {
            log.add(methodCall);
            switch (methodCall.method) {
              case 'isSupported':
                return true;
              case 'enroll':
                return {
                  'publicKey': 'mock-platform-pubkey',
                  'attestation': 'mock-platform-attestation',
                };
              case 'prove':
                return 'mock-platform-signature';
              case 'loadEnrollmentToken':
                return storedToken;
              case 'saveEnrollmentToken':
                storedToken = (methodCall.arguments as Map)['token'] as String;
                return null;
              case 'reset':
                storedToken = null;
                return null;
              default:
                return null;
            }
          });
    });

    tearDown(() {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, null);
    });

    test('invokes native platform methods correctly', () async {
      const provider = PlatformAttestationProvider();

      expect(await provider.isSupported(), isTrue);

      final material = await provider.enroll('server-challenge-xyz');
      expect(material.publicKey, 'mock-platform-pubkey');
      expect(material.attestation, 'mock-platform-attestation');

      final proof = await provider.prove(
        'server-challenge-xyz',
        'sess-1',
        'dev-1',
      );
      expect(proof, 'mock-platform-signature');

      expect(await provider.loadEnrollmentToken(), isNull);
      await provider.saveEnrollmentToken('enrolled-token-123');
      expect(await provider.loadEnrollmentToken(), 'enrolled-token-123');

      await provider.reset();
      expect(await provider.loadEnrollmentToken(), isNull);

      expect(log.map((c) => c.method), [
        'isSupported',
        'enroll',
        'prove',
        'loadEnrollmentToken',
        'saveEnrollmentToken',
        'loadEnrollmentToken',
        'reset',
        'loadEnrollmentToken',
      ]);
    });
  });
}
