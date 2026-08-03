import 'dart:io';

import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:uuid/uuid.dart';

/// A Flutter implementation of [IDeviceDataGenerator] that uses
/// `device_info_plus`, `package_info_plus`, and `shared_preferences` to
/// automatically collect device data.
class FlutterDeviceDataGenerator implements IDeviceDataGenerator {
  late final String _deviceId;
  late final String _sessionId;
  late final DeviceMetadata _deviceMetadata;

  static const String _deviceIdKey = 'flutter_remote_logging_device_id';
  static const Uuid _uuid = Uuid();

  @override
  Future<void> init() async {
    _sessionId = _uuid.v4();

    final prefs = await SharedPreferences.getInstance();
    var storedDeviceId = prefs.getString(_deviceIdKey);
    if (storedDeviceId == null) {
      storedDeviceId = _uuid.v4();
      await prefs.setString(_deviceIdKey, storedDeviceId);
    }
    _deviceId = storedDeviceId;

    final deviceInfo = DeviceInfoPlugin();
    final packageInfo = await PackageInfo.fromPlatform();

    var os = 'unknown';
    var osVersion = 'unknown';
    var model = 'unknown';
    var brand = 'unknown';
    var device = 'unknown';
    var manufacturer = 'unknown';
    var isPhysicalDevice = true;

    if (kIsWeb) {
      os = 'web';
      final webBrowserInfo = await deviceInfo.webBrowserInfo;
      osVersion = webBrowserInfo.appVersion ?? 'unknown';
      model = webBrowserInfo.browserName.name;
      brand = webBrowserInfo.vendor ?? 'unknown';
      device = webBrowserInfo.platform ?? 'unknown';
      manufacturer = 'unknown';
    } else {
      if (Platform.isAndroid) {
        os = 'android';
        final androidInfo = await deviceInfo.androidInfo;
        osVersion = androidInfo.version.release;
        model = androidInfo.model;
        brand = androidInfo.brand;
        device = androidInfo.device;
        manufacturer = androidInfo.manufacturer;
        isPhysicalDevice = androidInfo.isPhysicalDevice;
      } else if (Platform.isIOS) {
        os = 'ios';
        final iosInfo = await deviceInfo.iosInfo;
        osVersion = iosInfo.systemVersion;
        model = iosInfo.model;
        brand = 'Apple';
        device = iosInfo.name;
        manufacturer = 'Apple';
        isPhysicalDevice = iosInfo.isPhysicalDevice;
      } else if (Platform.isMacOS) {
        os = 'macos';
        final macOsInfo = await deviceInfo.macOsInfo;
        osVersion =
            '${macOsInfo.majorVersion}.${macOsInfo.minorVersion}.'
            '${macOsInfo.patchVersion}';
        model = macOsInfo.model;
        brand = 'Apple';
        device = macOsInfo.computerName;
        manufacturer = 'Apple';
      } else if (Platform.isWindows) {
        os = 'windows';
        final windowsInfo = await deviceInfo.windowsInfo;
        osVersion =
            '${windowsInfo.majorVersion}.${windowsInfo.minorVersion}.'
            '${windowsInfo.buildNumber}';
        model = windowsInfo.productName;
        brand = 'Microsoft';
        device = windowsInfo.computerName;
        manufacturer = 'Microsoft';
      } else if (Platform.isLinux) {
        os = 'linux';
        final linuxInfo = await deviceInfo.linuxInfo;
        osVersion = linuxInfo.version ?? linuxInfo.versionId ?? 'unknown';
        model = linuxInfo.name;
        brand = 'unknown';
        device = linuxInfo.prettyName;
        manufacturer = 'unknown';
      }
    }

    _deviceMetadata = DeviceMetadata(
      timestamp: DateTime.now().toUtc().toIso8601String(),
      deviceId: _deviceId,
      sessionId: _sessionId,
      os: os,
      osVersion: osVersion,
      model: model,
      brand: brand,
      device: device,
      identifier: packageInfo.packageName,
      manufacturer: manufacturer,
      isPhysicalDevice: isPhysicalDevice.toString(),
    );
  }

  @override
  String get deviceId => _deviceId;

  @override
  DeviceMetadata get deviceMetadata => _deviceMetadata;

  @override
  String get sessionId => _sessionId;
}
