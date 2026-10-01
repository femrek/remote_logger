import 'package:remote_logging/remote_logging.dart';

/// A class that generates and stores a unique device ID using secure storage.
/// The ID persists across app launches but is unique per installation.
abstract interface class DeviceDataGenerator {
  /// Initializer of [DeviceDataGenerator]. must read or create [deviceId] and
  /// must create [sessionId].
  ///
  /// After [init] is completed, [deviceId], [sessionId] and [deviceMetadata]
  /// are available.
  Future<void> init();

  /// Unique identifier for the device. This have to be randomly generated once
  /// and persisted across app launches. Read if exist in persistent
  /// storage or generate a new one and save it in [init].
  ///
  /// Must be accessible after [init] is completed.
  String get deviceId;

  /// Unique identifier for the current session. This have to be randomly
  /// generated each time the app is launched.
  ///
  /// Must be accessible after [init] is completed.
  String get sessionId;

  /// Metadata about the device.
  ///
  /// Must be accessible after [init] is completed.
  DeviceMetadata get deviceMetadata;
}
