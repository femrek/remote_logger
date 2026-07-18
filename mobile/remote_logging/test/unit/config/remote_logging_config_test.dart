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
      expect(config.localLogPrintLevel, Level.FINEST);
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
  });
}
