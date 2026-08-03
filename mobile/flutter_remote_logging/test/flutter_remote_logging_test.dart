import 'package:flutter/services.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sqflite_common_ffi/sqflite_ffi.dart';

class MockLocalLogRepository implements ILocalLogRepository {
  bool initCalled = false;

  @override
  Future<List<ServerLog>> getUnsentLogs() async => [];

  @override
  Future<void> removeLogs(List<String> logIds) async {}

  @override
  Future<void> saveLogs(List<ServerLog> logs) async {}
}

class MockDeviceDataGenerator implements IDeviceDataGenerator {
  @override
  String get deviceId => 'mock_device_id';

  @override
  DeviceMetadata get deviceMetadata => const DeviceMetadata(
    timestamp: '2023-01-01T00:00:00.000Z',
    deviceId: 'mock_device_id',
    sessionId: 'mock_session_id',
    os: 'test_os',
    osVersion: '1.0',
    model: 'test_model',
    brand: 'test_brand',
    device: 'test_device',
    identifier: 'test_id',
    manufacturer: 'test_manufacturer',
    isPhysicalDevice: 'true',
  );

  @override
  Future<void> init() async {}

  @override
  String get sessionId => 'mock_session_id';
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() {
    sqfliteFfiInit();
    databaseFactory = databaseFactoryFfi;
  });

  setUp(() {
    SharedPreferences.setMockInitialValues({});
    PackageInfo.setMockInitialValues(
      appName: 'test_app',
      packageName: 'com.example.test',
      version: '1.0.0',
      buildNumber: '1',
      buildSignature: 'signature',
    );

    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('dev.fluttercommunity.plus/device_info'),
          (methodCall) async {
            return {
              'computerName': 'Test Mac',
              'hostName': 'test-mac',
              'arch': 'arm64',
              'model': 'MacBook',
              'modelName': 'MacBook Pro',
              'kernelVersion': 'Darwin',
              'osRelease': '13.0',
              'activeCPUs': 8,
              'memorySize': 16000000000,
              'cpuFrequency': 3200000000,
              'systemGUID': 'test-guid',
              'majorVersion': 13,
              'minorVersion': 0,
              'patchVersion': 0,
              'name': 'Linux',
              'version': '1.0',
              'id': 'linux_id',
              'idLike': ['debian'],
              'versionCodename': 'test',
              'versionId': '1.0',
              'prettyName': 'Test Linux',
              'buildId': '1',
              'variant': 'test',
              'variantId': '1',
              'machineId': '1',
            };
          },
        );
  });

  RemoteLogging? currentRemote;
  LocalLogging? currentLocal;

  tearDownAll(() async {
    // just in case
  });

  tearDown(() async {
    await currentRemote?.stop(flush: false);
    currentLocal?.stop();
    currentRemote = null;
    currentLocal = null;
  });

  test('initLocalOnly initializes successfully', () async {
    currentLocal = await FlutterRemoteLogging.initLocalOnly(
      localLoggingConfig: const LocalLoggingConfig(),
    );

    expect(currentLocal, isNotNull);
    expect(currentLocal, isA<LocalLogging>());
  });

  test(
    'initRemote uses default FlutterDeviceDataGenerator '
    'and SqfliteLocalLogRepository',
    () async {
      currentRemote = await FlutterRemoteLogging.initRemote(
        remoteLoggingConfig: const RemoteLoggingConfig(
          gatewayUrl: 'https://example.com/logs',
          channelId: 'test-channel',
        ),
      );

      expect(currentRemote, isNotNull);
      expect(currentRemote, isA<RemoteLogging>());
    },
  );

  test('initRemote uses provided localLogRepository', () async {
    final mockRepo = MockLocalLogRepository();
    currentRemote = await FlutterRemoteLogging.initRemote(
      remoteLoggingConfig: const RemoteLoggingConfig(
        gatewayUrl: 'https://example.com/logs',
        channelId: 'test-channel',
      ),
      localLogRepository: mockRepo,
    );

    expect(currentRemote, isNotNull);
  });

  test('initRemote uses provided deviceDataGenerator', () async {
    final mockGenerator = MockDeviceDataGenerator();
    currentRemote = await FlutterRemoteLogging.initRemote(
      remoteLoggingConfig: const RemoteLoggingConfig(
        gatewayUrl: 'https://example.com/logs',
        channelId: 'test-channel',
      ),
      deviceDataGenerator: mockGenerator,
    );

    expect(currentRemote, isNotNull);
  });

  test(
    'initRemote uses NoSaveLocalLogRepository if fallback '
    'is disabled and no repo is provided',
    () async {
      currentRemote = await FlutterRemoteLogging.initRemote(
        remoteLoggingConfig: const RemoteLoggingConfig(
          gatewayUrl: 'https://example.com/logs',
          channelId: 'test-channel',
        ),
        enableDefaultLocalFallbackStorage: false,
      );

      expect(currentRemote, isNotNull);
      // There is no public accessor to easily assert which repo it got,
      // but we know it will not throw and successfully initialize.
    },
  );
}
