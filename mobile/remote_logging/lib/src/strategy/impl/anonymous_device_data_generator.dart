import 'package:remote_logging/remote_logging.dart';

/// A device data generator that generates anonymous/placeholder device metadata.
///
/// This is used when real device metadata is not available (e.g.
/// non-mobile platforms) or when anonymous logging is preferred.
class AnonymousDeviceDataGenerator implements DeviceDataGenerator {
  @override
  Future<void> init() {
    return Future.value();
  }

  @override
  DeviceMetadata get deviceMetadata => DeviceMetadata(
    timestamp: DateTime.now().toIso8601String(),
    deviceId: deviceId,
    sessionId: sessionId,
    os: 'unknown',
    osVersion: 'unknown',
    model: 'unknown',
    brand: 'unknown',
    device: 'unknown',
    identifier: 'unknown',
    manufacturer: 'unknown',
    isPhysicalDevice: 'unknown',
  );

  @override
  String get deviceId => '000000000000-0000-0000-0000-00000000';

  @override
  String get sessionId => '000000000000-0000-0000-0000-00000000';
}
