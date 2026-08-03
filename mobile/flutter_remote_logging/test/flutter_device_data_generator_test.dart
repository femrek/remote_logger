import 'package:flutter/services.dart';
import 'package:flutter_remote_logging/src/strategies/flutter_device_data_generator.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUp(() {
    SharedPreferences.setMockInitialValues({});
    PackageInfo.setMockInitialValues(
      appName: 'test_app',
      packageName: 'com.example.test',
      version: '1.0.0',
      buildNumber: '1',
      buildSignature: 'signature',
    );

    // Mock device_info_plus channel to prevent MissingPluginException
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
              // Windows
              'numberOfCores': 4,
              'systemMemoryInMegabytes': 8000,
              'userName': 'testUser',
              'buildNumber': 19041,
              'platformId': 2,
              'csdVersion': '',
              'servicePackMajor': 0,
              'servicePackMinor': 0,
              'suitMask': 0,
              'productType': 1,
              'reserved': 0,
              'buildLab': 'test',
              'buildLabEx': 'test',
              'digitalProductId': <int>[],
              'displayVersion': '20H2',
              'editionId': 'Professional',
              'installDate': 1600000000,
              'productId': 'test',
              'productName': 'Windows 10',
              'registeredOwner': 'test',
              'releaseId': '2009',
              'deviceId': 'test',
            };
          },
        );
  });

  test('init generates a new sessionId', () async {
    final generator1 = FlutterDeviceDataGenerator();
    await generator1.init();

    final generator2 = FlutterDeviceDataGenerator();
    await generator2.init();

    expect(generator1.sessionId, isNotEmpty);
    expect(generator2.sessionId, isNotEmpty);
    expect(generator1.sessionId, isNot(generator2.sessionId));
  });

  test('init generates and stores a new deviceId if none exists', () async {
    final generator = FlutterDeviceDataGenerator();
    await generator.init();

    expect(generator.deviceId, isNotEmpty);

    final prefs = await SharedPreferences.getInstance();
    expect(
      prefs.getString('flutter_remote_logging_device_id'),
      generator.deviceId,
    );
  });

  test('init retrieves existing deviceId from SharedPreferences', () async {
    SharedPreferences.setMockInitialValues({
      'flutter_remote_logging_device_id': 'existing_device_id',
    });

    final generator = FlutterDeviceDataGenerator();
    await generator.init();

    expect(generator.deviceId, 'existing_device_id');
  });

  test('deviceMetadata is populated correctly', () async {
    final generator = FlutterDeviceDataGenerator();
    await generator.init();

    final metadata = generator.deviceMetadata;

    expect(metadata.deviceId, generator.deviceId);
    expect(metadata.sessionId, generator.sessionId);
    expect(metadata.identifier, 'com.example.test');
    expect(metadata.timestamp, isNotEmpty);
    expect(metadata.os, isNotEmpty);
    expect(metadata.osVersion, isNotEmpty);
    expect(metadata.model, isNotEmpty);
    expect(metadata.brand, isNotEmpty);
    expect(metadata.device, isNotEmpty);
    expect(metadata.manufacturer, isNotEmpty);
  });
}
