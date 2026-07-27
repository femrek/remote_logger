import 'package:remote_logging/remote_logging.dart';

class AnonymousDeviceDataGenerator implements IDeviceDataGenerator {
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
