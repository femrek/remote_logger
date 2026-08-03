import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

void main() {
  group('RemoteLoggingConfig', () {
    test('default values are applied correctly', () {
      const config = RemoteLoggingConfig(
        gatewayUrl: 'https://api.example.com',
        channelId: 'channel-id',
      );

      expect(config.enableRemoteLogging, isTrue);
      expect(config.logSendLevel, Level.INFO);
      expect(config.logPrintLevel, Level.FINEST);
      expect(config.remoteLoggingLibraryLogPrintLevel, Level.CONFIG);
    });

    test('copyWith replaces specified fields and preserves others', () {
      const config = RemoteLoggingConfig(
        gatewayUrl: 'https://api.example.com',
        channelId: 'channel-id',
        enableRemoteLogging: false,
      );

      final updatedConfig = config.copyWith(
        gatewayUrl: 'https://api.example.com/v2',
        enableRemoteLogging: true,
      );

      expect(
        updatedConfig.gatewayUrl,
        'https://api.example.com/v2',
      );
      expect(updatedConfig.channelId, 'channel-id');
      expect(updatedConfig.enableRemoteLogging, isTrue);
    });

    test('toString contains all field values', () {
      const config = RemoteLoggingConfig(
        gatewayUrl: 'https://api.example.com',
        channelId: 'channel-id',
      );

      final str = config.toString();
      expect(str, contains('https://api.example.com'));
      expect(str, contains('channel-id'));
    });

    group('computed properties and new fields', () {
      test('gatewayUrlTrimmed strips trailing slash', () {
        const config = RemoteLoggingConfig(
          gatewayUrl: 'https://api.example.com/',
          channelId: 'channel-id',
        );

        expect(config.gatewayUrlTrimmed, equals('https://api.example.com'));
      });

      test('gatewayUrlTrimmed is stable when no trailing slash', () {
        const config = RemoteLoggingConfig(
          gatewayUrl: 'https://api.example.com',
          channelId: 'channel-id',
        );

        expect(config.gatewayUrlTrimmed, equals('https://api.example.com'));
      });

      test('gatewayUrlTrimmed returns null when gatewayUrl is null', () {
        const config = RemoteLoggingConfig(
          channelId: 'channel-id',
        );

        expect(config.gatewayUrlTrimmed, isNull);
      });

      test('path returns the correct /v1/logs/{channelId}/batch URL', () {
        const config = RemoteLoggingConfig(
          gatewayUrl: 'https://api.example.com/',
          channelId: 'my-channel',
        );

        expect(
          config.path,
          equals('https://api.example.com/v1/logs/my-channel/batch'),
        );
      });

      test('maxPendingLogCount defaults to 500', () {
        const config = RemoteLoggingConfig();

        expect(config.maxPendingLogCount, equals(500));
      });

      test(
        'truncation defaults to LogTruncationConfig() with expected defaults',
        () {
          const config = RemoteLoggingConfig();

          expect(config.truncation.maxMessageChars, equals(8192));
          expect(config.truncation.maxErrorChars, equals(2048));
          expect(config.truncation.maxStacktraceLines, equals(100));
          expect(config.truncation.maxStacktraceLineChars, equals(512));
        },
      );
    });
  });
}
