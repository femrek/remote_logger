import 'package:flutter_remote_logging/src/flutter_device_data_generator.dart';
import 'package:flutter_remote_logging/src/sqflite_local_log_repository.dart';
import 'package:remote_logging/remote_logging.dart';

export 'package:remote_logging/remote_logging.dart';
export 'src/flutter_device_data_generator.dart';
export 'src/sqflite_local_log_repository.dart';

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
    ILocalLogRepository? localLogRepository,
    bool enableLocalFallbackStorage = true,
    OnLog? onRemoteLog,
    OnLog? onLocalLog,
    OnError? onError,
  }) async {
    var repo = localLogRepository;

    if (enableLocalFallbackStorage && repo == null) {
      final sqfliteRepo = SqfliteLocalLogRepository();
      try {
        await sqfliteRepo.init();
        repo = sqfliteRepo;
      } on Object catch (e, st) {
        localLog.warning(
          'Failed to initialize SqfliteLocalLogRepository',
          e,
          st
        );
      }
    }

    return RemoteLogging.init(
      remoteLoggingConfig: remoteLoggingConfig,
      deviceDataGenerator: deviceDataGenerator ?? FlutterDeviceDataGenerator(),
      getAttestationTokenCallback: getAttestationTokenCallback,
      localLogRepository: repo,
      onRemoteLog: onRemoteLog,
      onLocalLog: onLocalLog,
      onError: onError,
    );
  }
}
