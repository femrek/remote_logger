import 'dart:io';
import 'dart:typed_data';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:device_info_plus_platform_interface/device_info_plus_platform_interface.dart';
import 'package:mocktail/mocktail.dart';

class MockLinuxDeviceInfo extends Mock implements LinuxDeviceInfo {
  @override
  Map<String, dynamic> get data => {
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
  @override
  String get name => 'Linux';
  @override
  String get version => '1.0';
  @override
  String get id => 'linux_id';
  @override
  List<String> get idLike => ['debian'];
  @override
  String get versionCodename => 'test';
  @override
  String get versionId => '1.0';
  @override
  String get prettyName => 'Test Linux';
  @override
  String get buildId => '1';
  @override
  String get variant => 'test';
  @override
  String get variantId => '1';
  @override
  String get machineId => '1';
}

class MockMacOsDeviceInfo extends Mock implements MacOsDeviceInfo {
  @override
  Map<String, dynamic> get data => {
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
  };
  @override
  String get computerName => 'Test Mac';
  @override
  String get hostName => 'test-mac';
  @override
  String get arch => 'arm64';
  @override
  String get model => 'MacBook';
  @override
  String get modelName => 'MacBook Pro';
  @override
  String get kernelVersion => 'Darwin';
  @override
  String get osRelease => '13.0';
  @override
  int get activeCPUs => 8;
  @override
  int get memorySize => 16000000000;
  @override
  int get cpuFrequency => 3200000000;
  @override
  String? get systemGUID => 'test-guid';
  @override
  int get majorVersion => 13;
  @override
  int get minorVersion => 0;
  @override
  int get patchVersion => 0;
}

class MockWindowsDeviceInfo extends Mock implements WindowsDeviceInfo {
  @override
  Map<String, dynamic> get data => {};
  @override
  int get numberOfCores => 4;
  @override
  String get computerName => 'WindowsTest';
  @override
  int get systemMemoryInMegabytes => 8000;
  @override
  String get userName => 'testUser';
  @override
  int get majorVersion => 10;
  @override
  int get minorVersion => 0;
  @override
  int get buildNumber => 19041;
  @override
  int get platformId => 2;
  @override
  String get csdVersion => '';
  @override
  int get servicePackMajor => 0;
  @override
  int get servicePackMinor => 0;
  @override
  int get suitMask => 0;
  @override
  int get productType => 1;
  @override
  int get reserved => 0;
  @override
  String get buildLab => 'test';
  @override
  String get buildLabEx => 'test';
  @override
  Uint8List get digitalProductId => Uint8List(0);
  @override
  String get displayVersion => '20H2';
  @override
  String get editionId => 'Professional';
  @override
  DateTime get installDate => DateTime.fromMillisecondsSinceEpoch(1600000000);
  @override
  String get productId => 'test';
  @override
  String get productName => 'Windows 10';
  @override
  String get registeredOwner => 'test';
  @override
  String get releaseId => '2009';
  @override
  String get deviceId => 'test';
}

class MockAndroidBuildVersion extends Mock implements AndroidBuildVersion {
  @override
  String get release => '13';
}

class MockAndroidDeviceInfo extends Mock implements AndroidDeviceInfo {
  @override
  Map<String, dynamic> get data => {
    'version': {
      'baseOS': 'android',
      'codename': 'REL',
      'incremental': '1',
      'previewSdkInt': 0,
      'release': '13',
      'sdkInt': 33,
      'securityPatch': '2023-01-01',
    },
    'board': 'test',
    'bootloader': 'test',
    'brand': 'Google',
    'device': 'test_device',
    'display': 'test',
    'fingerprint': 'test',
    'hardware': 'test',
    'host': 'test',
    'id': 'test',
    'manufacturer': 'Google',
    'model': 'Test Android',
    'product': 'test',
    'supported32BitAbis': <String>[],
    'supported64BitAbis': <String>[],
    'supportedAbis': <String>[],
    'tags': 'test',
    'type': 'test',
    'isPhysicalDevice': true,
    'systemFeatures': <String>[],
    'displayMetrics': {
      'widthPx': 1080.0,
      'heightPx': 1920.0,
      'xDpi': 400.0,
      'yDpi': 400.0,
    },
    'serialNumber': 'test',
  };
  @override
  AndroidBuildVersion get version => MockAndroidBuildVersion();
  @override
  String get model => 'Test Android';
  @override
  String get brand => 'Google';
  @override
  String get device => 'test_device';
  @override
  String get manufacturer => 'Google';
  @override
  bool get isPhysicalDevice => true;
}

class MockIosDeviceInfo extends Mock implements IosDeviceInfo {
  @override
  Map<String, dynamic> get data => {};
  @override
  String get systemVersion => '16.0';
  @override
  String get model => 'iPhone';
  @override
  String get name => 'Test iPhone';
  @override
  bool get isPhysicalDevice => true;
}

class MockDeviceInfoPlatform extends DeviceInfoPlatform {
  @override
  Future<BaseDeviceInfo> deviceInfo() async {
    if (Platform.isLinux) return MockLinuxDeviceInfo();
    if (Platform.isMacOS) return MockMacOsDeviceInfo();
    if (Platform.isWindows) return MockWindowsDeviceInfo();
    if (Platform.isAndroid) return MockAndroidDeviceInfo();
    if (Platform.isIOS) return MockIosDeviceInfo();
    return MockLinuxDeviceInfo();
  }
}
