import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

void main() {
  group('LocalLoggingConfig', () {
    test('default logPrintLevel is Level.FINEST', () {
      const config = LocalLoggingConfig();
      expect(config.logPrintLevel, equals(Level.FINEST));
    });

    test('default remoteLoggingLibraryLogPrintLevel is Level.CONFIG', () {
      const config = LocalLoggingConfig();
      expect(config.remoteLoggingLibraryLogPrintLevel, equals(Level.CONFIG));
    });

    test('copyWith replaces specified fields and preserves others', () {
      const config = LocalLoggingConfig(
        logPrintLevel: Level.INFO,
        remoteLoggingLibraryLogPrintLevel: Level.WARNING,
      );

      final updated = config.copyWith(
        logPrintLevel: Level.SEVERE,
      );

      expect(updated.logPrintLevel, equals(Level.SEVERE));
      expect(updated.remoteLoggingLibraryLogPrintLevel, equals(Level.WARNING));
    });

    test('toString contains both field values', () {
      const config = LocalLoggingConfig(
        logPrintLevel: Level.INFO,
        remoteLoggingLibraryLogPrintLevel: Level.WARNING,
      );

      final str = config.toString();
      expect(str, contains('logPrintLevel: INFO'));
      expect(str, contains('localLogPrintLevel: WARNING'));
    });
  });
}
