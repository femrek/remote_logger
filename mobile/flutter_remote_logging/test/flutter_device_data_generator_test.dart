import 'package:device_info_plus_platform_interface/device_info_plus_platform_interface.dart';
import 'package:flutter/services.dart';
import 'package:flutter_remote_logging/src/strategies/flutter_device_data_generator.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'mock_device_info_platform.dart';

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

    DeviceInfoPlatform.instance = MockDeviceInfoPlatform();
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
