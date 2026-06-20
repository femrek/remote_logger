# Remote Logging

## Init

```dart
static void init(AppLogSenderService service) {
  _logSenderService = service;

  const level = kDebugMode ? Level.FINE : Level.INFO;

  Logger.root.level = level;
  Logger.root.onRecord.listen(_onLog);
  _LocalLog.init(level, _onLocalLog);

  FlutterError.onError = (FlutterErrorDetails details) {
    logger('FlutterErrorHandler').severe(
      details.exceptionAsString(),
      details.exception,
      details.stack,
    );
  };
}
```

```dart
import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:remote_logging/remote_logging.dart';

/// An helper class to collect device metadata such as OS, model, and
/// manufacturer.
abstract final class DeviceMetadataCollector {
  /// Retrieves device metadata.
  static Future<DeviceMetadata> collect(
    String deviceId,
    String sessionId,
  ) async {
    final deviceInfo = DeviceInfoPlugin();
    final timestamp = DateTime.now().toUtc().toIso8601String();

    if (Platform.isAndroid) {
      final info = await deviceInfo.androidInfo;
      return DeviceMetadata(
        timestamp: timestamp,
        deviceId: deviceId,
        sessionId: sessionId,
        os: 'Android',
        osVersion: info.version.release,
        model: info.model,
        brand: info.brand,
        device: info.device,
        identifier: info.id,
        manufacturer: info.manufacturer,
        isPhysicalDevice: info.isPhysicalDevice.toString(),
      );
    } else if (Platform.isIOS) {
      final info = await deviceInfo.iosInfo;
      return DeviceMetadata(
        timestamp: timestamp,
        deviceId: deviceId,
        sessionId: sessionId,
        os: 'iOS',
        osVersion: info.systemVersion,
        model: info.utsname.machine,
        brand: 'Apple',
        device: info.name,
        identifier: info.identifierForVendor ?? 'unknown',
        manufacturer: 'Apple',
        isPhysicalDevice: info.isPhysicalDevice.toString(),
      );
    } else {
      return DeviceMetadata(
        timestamp: timestamp,
        deviceId: deviceId,
        sessionId: sessionId,
        os: 'Unknown',
        osVersion: 'unknown',
        model: 'unknown',
        brand: 'unknown',
        device: 'unknown',
        identifier: 'unknown',
        manufacturer: 'unknown',
        isPhysicalDevice: 'unknown',
      );
    }
  }
}
```
