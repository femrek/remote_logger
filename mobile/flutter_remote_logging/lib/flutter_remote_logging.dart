import 'package:flutter_remote_logging/src/flutter_device_data_generator.dart';
import 'package:remote_logging/remote_logging.dart';

export 'package:remote_logging/remote_logging.dart';
export 'src/flutter_device_data_generator.dart';

/// A Flutter-specific wrapper for [RemoteLogging] that automatically
/// handles device data generation using `FlutterDeviceDataGenerator`.
class FlutterRemoteLogging {
  FlutterRemoteLogging._();

  /// Initializes the remote logging system for Flutter apps.
  ///
  /// This method automatically instantiates a [FlutterDeviceDataGenerator]
  /// and passes it to the underlying [RemoteLogging] implementation, saving
  /// developers from having to implement their own device data generators.
  ///
  /// Provide a [getAttestationTokenCallback] if your backend requires an
  /// app attestation token (e.g. Firebase App Check).
  static Future<RemoteLogging> init({
    required RemoteLoggingConfig remoteLoggingConfig,
    IDeviceDataGenerator? deviceDataGenerator,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    OnLog? onRemoteLog,
    OnLog? onLocalLog,
    OnError? onError,
  }) async {
    return RemoteLogging.init(
      remoteLoggingConfig: remoteLoggingConfig,
      deviceDataGenerator: deviceDataGenerator ?? FlutterDeviceDataGenerator(),
      getAttestationTokenCallback: getAttestationTokenCallback,
      onRemoteLog: onRemoteLog,
      onLocalLog: onLocalLog,
      onError: onError,
    );
  }
}
