import 'package:remote_logging/remote_logging.dart';
import 'package:uuid/uuid.dart';

RemoteLoggingConfig get testConfig => const RemoteLoggingConfig(
  gatewayUrl: 'http://localhost:8080',
  channelId: 'test-channel',
  remoteLoggingLibraryLogPrintLevel: Level.FINEST,
  logSendLevel: Level.FINEST,
);

class TestDeviceDataGenerator implements DeviceDataGenerator {
  late final String _deviceIdCache;
  late final String _sessionIdCache;
  late final DeviceMetadata _metadataCache;

  @override
  Future<void> init() async {
    _deviceIdCache = 'test-device-id';
    _sessionIdCache = _generateRandomId();
    _metadataCache = await _collect(_deviceIdCache, _sessionIdCache);
  }

  @override
  String get deviceId => _deviceIdCache;

  @override
  String get sessionId => _sessionIdCache;

  @override
  DeviceMetadata get deviceMetadata => _metadataCache;

  Future<DeviceMetadata> _collect(String deviceId, String sessionId) async {
    return DeviceMetadata(
      timestamp: DateTime.now().toUtc().toIso8601String(),
      deviceId: deviceId,
      sessionId: sessionId,
      os: 'test_os',
      osVersion: 'test_os_version',
      model: 'test_model',
      brand: 'test_brand',
      device: 'test_device',
      identifier: 'test_identifier',
      manufacturer: 'test_manufacturer',
      isPhysicalDevice: 'true',
    );
  }

  String _generateRandomId() => const Uuid().v4();
}
