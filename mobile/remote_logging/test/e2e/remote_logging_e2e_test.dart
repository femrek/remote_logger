// ignore_for_file: cascade_invocations just a test
@Tags(['e2e'])
library;

import 'dart:async';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';
import 'package:uuid/uuid.dart';

void main() {
  group('RemoteLogging E2E Tests', () {
    test(
      'Successfully authenticate and send log batch to remote dev server',
      () async {
        // Configuration pointing to the actual dev server
        const config = RemoteLoggingConfig(
          gatewayUrl: 'https://hs-logger.femrek.dev',
          channelId: 'test-channel',
          logSendLevel: Level.ALL,
        );

        Object? caughtError;

        final remoteLogger = await RemoteLoggingInit.initRemote(
          config: config,
          deviceDataGenerator: _E2eDeviceDataGenerator(),
          onError: (error, stackTrace) {
            caughtError = error;
            print('E2E Test Caught Error: $error');
          },
        );

        // The DioNetworkInvoker configured with your actual host
        final invoker = DioNetworkInvoker.fromBaseUrl(
          'https://hs-logger.femrek.dev',
        );

        // Start the log sender service
        unawaited(remoteLogger.start(invoker));

        final log = logger('e2eTestLogger');
        log.info('E2E Test: First remote log message');
        log.warning('E2E Test: Second remote log message');

        // Flush the logs instead of arbitrary delays
        await remoteLogger.flush();

        // Ensure no errors were caught during the process
        expect(
          caughtError,
          isNull,
          reason: 'Expected no network errors during log transmission',
        );
      },
    );
  });
}

class _E2eDeviceDataGenerator implements IDeviceDataGenerator {
  @override
  Future<void> init() async {
    _deviceIdCache = 'e2e-device-${const Uuid().v4()}';
    _sessionIdCache = const Uuid().v4();
    _metadataCache = await _collect(_deviceIdCache, _sessionIdCache);
  }

  @override
  String get deviceId => _deviceIdCache;

  @override
  String get sessionId => _sessionIdCache;

  @override
  DeviceMetadata get deviceMetadata => _metadataCache;

  late final String _deviceIdCache;
  late final String _sessionIdCache;
  late final DeviceMetadata _metadataCache;

  Future<DeviceMetadata> _collect(String deviceId, String sessionId) async {
    return DeviceMetadata(
      timestamp: DateTime.now().toUtc().toIso8601String(),
      deviceId: deviceId,
      sessionId: sessionId,
      os: 'e2e_test_os',
      osVersion: '1.0.0',
      model: 'e2e_model',
      brand: 'e2e_brand',
      device: 'e2e_device',
      identifier: 'e2e_identifier',
      manufacturer: 'e2e_manufacturer',
      isPhysicalDevice: 'false',
    );
  }
}
