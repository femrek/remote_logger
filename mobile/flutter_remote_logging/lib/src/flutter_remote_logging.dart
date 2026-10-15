import 'package:flutter_remote_logging/src/attestation/platform_attestation_provider.dart';
import 'package:flutter_remote_logging/src/strategies/flutter_device_data_generator.dart';
import 'package:flutter_remote_logging/src/strategies/sqflite_local_log_repository.dart';
import 'package:flutter_remote_logging/src/util/internal_logger.dart';
import 'package:remote_logging/remote_logging.dart';

/// A Flutter-specific wrapper for [RemoteLogging] that automatically
/// handles device data generation using `FlutterDeviceDataGenerator`.
abstract final class FlutterRemoteLogging {
  FlutterRemoteLogging._();

  /// Initializes the remote logging system for Flutter apps.
  ///
  /// This method automatically instantiates a [FlutterDeviceDataGenerator]
  /// and passes it to the underlying [RemoteLogging] implementation, saving
  /// developers from having to implement their own device data generators.
  ///
  /// Provide an [attestationProvider] (defaults to [PlatformAttestationProvider])
  /// or a legacy [getAttestationTokenCallback].
  static Future<RemoteLogging> initRemote({
    required RemoteLoggingConfig remoteLoggingConfig,
    DeviceDataGenerator? deviceDataGenerator,
    AttestationProvider? attestationProvider,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    LocalLogRepository? localLogRepository,
    bool enableDefaultLocalFallbackStorage = true,
    OnLog? onLog,
    OnError? onError,
  }) async {
    var repo = localLogRepository;

    if (enableDefaultLocalFallbackStorage && repo == null) {
      final sqfliteRepo = SqfliteLocalLogRepository();
      try {
        await sqfliteRepo.init();
        repo = sqfliteRepo;
      } on Object catch (e, s) {
        internalLog.warning(
          'Failed to initialize SqfliteLocalLogRepository',
          e,
          s,
        );
      }
    }

    return RemoteLoggingInit.initRemote(
      config: remoteLoggingConfig,
      deviceDataGenerator: deviceDataGenerator ?? FlutterDeviceDataGenerator(),
      attestationProvider:
          attestationProvider ?? const PlatformAttestationProvider(),
      getAttestationTokenCallback: getAttestationTokenCallback,
      localLogRepository: repo ?? const NoSaveLocalLogRepository(),
      onLog: onLog,
      onError: onError,
    );
  }

  /// Initializes the local-only logging system for Flutter apps.
  static Future<LocalLogging> initLocalOnly({
    required LocalLoggingConfig localLoggingConfig,
    OnLog? onLog,
    OnError? onError,
  }) async {
    return RemoteLoggingInit.initLocal(
      config: localLoggingConfig,
      onLog: onLog,
    );
  }
}
