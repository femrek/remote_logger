import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

void main() {
  group('AnonymousDeviceDataGenerator', () {
    final generator = AnonymousDeviceDataGenerator();

    test(
      'deviceId returns fixed placeholder '
      '000000000000-0000-0000-0000-00000000',
      () {
        expect(
          generator.deviceId,
          equals('000000000000-0000-0000-0000-00000000'),
        );
      },
    );

    test(
      'sessionId returns fixed placeholder '
      '000000000000-0000-0000-0000-00000000',
      () {
        expect(
          generator.sessionId,
          equals('000000000000-0000-0000-0000-00000000'),
        );
      },
    );

    test('deviceMetadata.os is unknown', () {
      expect(generator.deviceMetadata.os, equals('unknown'));
    });

    test(
      'deviceMetadata.timestamp is a valid ISO 8601 string that '
      'DateTime.parse parses without error',
      () {
        final timestampStr = generator.deviceMetadata.timestamp;
        expect(() => DateTime.parse(timestampStr), returnsNormally);
      },
    );

    test('init completes without error', () {
      expect(generator.init(), completes);
    });
  });
}
