// ignore_for_file: avoid_redundant_argument_values test file,
// ignore_for_file: cascade_invocations

import 'dart:convert';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:http_test_server/http_test_server.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';
import 'package:uuid/uuid.dart';

void main() {
  group('RemoteLogging', () {
    group('basic', () {
      test('send log batch', () async {
        var serverSideExpectationsMet = false;
        final server = await TestServer.createHttpServer(
          events: [
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/auth/exchange',
              handler: (request) async {
                return jsonEncode({
                  'access_token': 'dummy-access',
                  'refresh_token': 'dummy-refresh',
                  'access_expires_in': 3600,
                  'refresh_expires_in': 86400,
                });
              },
              responseStatusCode: 200,
            ),
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/v1/logs/test-channel/batch',
              handler: (request) async {
                expect(
                  request.headers.value('authorization'),
                  'Bearer dummy-access',
                );
                final content = await request.map<String>((e) {
                  return String.fromCharCodes(e);
                }).join();
                final jsonData = (jsonDecode(content) as List<dynamic>)
                    .cast<Map<String, dynamic>>();
                final logMessages = jsonData
                    .map((e) => e['message'] as String)
                    .toList(growable: false);
                expect(logMessages.length, 2);
                expect(logMessages[0], 'First log message');
                expect(logMessages[1], 'Second log message');
                serverSideExpectationsMet = true;
                return '{}';
              },
              responseStatusCode: 200,
            ),
          ],
        );
        final baseUrl = 'http://localhost:${server.port}';

        final remoteLogger = await RemoteLogging.init(
          remoteLoggingConfig: _config.copyWith(gatewayUrl: baseUrl),
          deviceDataGenerator: TestDeviceDataGenerator(),
        );
        final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

        final log = logger('testLogger');
        log.info('First log message');
        log.warning('Second log message');

        await Future.any<dynamic>([
          remoteLogger.start(invoker),
          Future.doWhile(() async {
            await Future<void>.delayed(const Duration(milliseconds: 100));
            return !serverSideExpectationsMet;
          }),
          Future.delayed(const Duration(seconds: 15), () async {
            await server.close();
            fail('Timeout waiting for log batch to be sent');
          }),
        ]);

        if (!serverSideExpectationsMet) {
          fail('Server side expectations were not met');
        }

        await server.close();
      });
    });

    group('log level filtering', () {
      test('logs below logSendLevel should not be sent', () async {
        var batchSent = false;
        final server = await TestServer.createHttpServer(
          events: [
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/auth/exchange',
              handler: (request) async {
                return jsonEncode({
                  'access_token': 'dummy-access',
                  'refresh_token': 'dummy-refresh',
                  'access_expires_in': 3600,
                  'refresh_expires_in': 86400,
                });
              },
              responseStatusCode: 200,
            ),
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/v1/logs/test-channel/batch',
              handler: (request) async {
                final content = await request.map<String>((e) {
                  return String.fromCharCodes(e);
                }).join();
                final jsonData = (jsonDecode(content) as List<dynamic>)
                    .cast<Map<String, dynamic>>();
                // Should only have WARNING level log, not FINE
                expect(jsonData.length, 1);
                expect(jsonData[0]['level'] as String, 'WARNING');
                batchSent = true;
                return '{}';
              },
              responseStatusCode: 200,
            ),
          ],
        );
        final baseUrl = 'http://localhost:${server.port}';

        final configWithFiltering = RemoteLoggingConfig(
          gatewayUrl: baseUrl,
          channelId: 'test-channel',
          logSendLevel: Level.WARNING,
          logPrintLevel: Level.FINEST,
          localLogPrintLevel: Level.FINEST,
        );

        final remoteLogger = await RemoteLogging.init(
          remoteLoggingConfig: configWithFiltering,
          deviceDataGenerator: TestDeviceDataGenerator(),
        );
        final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

        final log = logger('filterTest');
        log.fine('Fine level message - should not be sent');
        log.warning('Warning level message - should be sent');

        await Future.any<dynamic>([
          remoteLogger.start(invoker),
          Future.doWhile(() async {
            await Future<void>.delayed(const Duration(milliseconds: 100));
            return !batchSent;
          }),
          Future.delayed(const Duration(seconds: 15), () async {
            await server.close();
            fail('Timeout waiting for log batch');
          }),
        ]);

        expect(batchSent, true);
        await server.close();
      });
    });

    group('configuration', () {
      test('remote logging disabled should not send logs', () async {
        var batchSent = false;
        final server = await TestServer.createHttpServer(
          events: [
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/auth/exchange',
              handler: (request) async {
                return jsonEncode({
                  'access_token': 'dummy-access',
                  'refresh_token': 'dummy-refresh',
                  'access_expires_in': 3600,
                  'refresh_expires_in': 86400,
                });
              },
              responseStatusCode: 200,
            ),
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/v1/logs/test-channel/batch',
              handler: (request) async {
                batchSent = true;
                return '{}';
              },
              responseStatusCode: 200,
            ),
          ],
        );
        final baseUrl = 'http://localhost:${server.port}';

        final disabledConfig = RemoteLoggingConfig(
          gatewayUrl: baseUrl,
          channelId: 'test-channel',
          enableRemoteLogging: false,
          logSendLevel: Level.FINEST,
          logPrintLevel: Level.FINEST,
          localLogPrintLevel: Level.FINEST,
        );

        final remoteLogger = await RemoteLogging.init(
          remoteLoggingConfig: disabledConfig,
          deviceDataGenerator: TestDeviceDataGenerator(),
        );
        final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

        final log = logger('disabledTest');
        log.info('This message should not be sent remotely');

        await remoteLogger.start(invoker);

        // Wait a bit to ensure no batch is sent
        await Future<void>.delayed(const Duration(seconds: 2));

        expect(batchSent, false);
        await server.close();
      });

      test('multiple log messages are batched correctly', () async {
        var batchCount = 0;
        final server = await TestServer.createHttpServer(
          events: [
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/auth/exchange',
              handler: (request) async {
                return jsonEncode({
                  'access_token': 'dummy-access',
                  'refresh_token': 'dummy-refresh',
                  'access_expires_in': 3600,
                  'refresh_expires_in': 86400,
                });
              },
              responseStatusCode: 200,
            ),
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/v1/logs/test-channel/batch',
              handler: (request) async {
                final content = await request.map<String>((e) {
                  return String.fromCharCodes(e);
                }).join();
                final jsonData = (jsonDecode(content) as List<dynamic>)
                    .cast<Map<String, dynamic>>();
                expect(jsonData.length, greaterThan(0));
                batchCount++;
                return '{}';
              },
              responseStatusCode: 200,
            ),
          ],
        );
        final baseUrl = 'http://localhost:${server.port}';

        final remoteLogger = await RemoteLogging.init(
          remoteLoggingConfig: _config.copyWith(gatewayUrl: baseUrl),
          deviceDataGenerator: TestDeviceDataGenerator(),
        );
        final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

        final log = logger('batchTest');
        for (var i = 0; i < 5; i++) {
          log.info('Batch message $i');
        }

        await Future.any<dynamic>([
          remoteLogger.start(invoker),
          Future.doWhile(() async {
            await Future<void>.delayed(const Duration(milliseconds: 100));
            return batchCount == 0;
          }),
          Future.delayed(const Duration(seconds: 15), () async {
            await server.close();
            fail('Timeout waiting for batch');
          }),
        ]);

        expect(batchCount, greaterThan(0));
        await server.close();
      });

      test('different log levels are captured', () async {
        var capturedLevels = <String>[];
        final server = await TestServer.createHttpServer(
          events: [
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/auth/exchange',
              handler: (request) async {
                return jsonEncode({
                  'access_token': 'dummy-access',
                  'refresh_token': 'dummy-refresh',
                  'access_expires_in': 3600,
                  'refresh_expires_in': 86400,
                });
              },
              responseStatusCode: 200,
            ),
            StandardServerEvent(
              matcher: (request) =>
                  request.method == 'POST' &&
                  request.uri.path == '/v1/logs/test-channel/batch',
              handler: (request) async {
                final content = await request.map<String>((e) {
                  return String.fromCharCodes(e);
                }).join();
                final jsonData = (jsonDecode(content) as List<dynamic>)
                    .cast<Map<String, dynamic>>();
                capturedLevels = jsonData
                    .map((e) => e['level'] as String)
                    .toList(growable: false);
                return '{}';
              },
              responseStatusCode: 200,
            ),
          ],
        );
        final baseUrl = 'http://localhost:${server.port}';

        final remoteLogger = await RemoteLogging.init(
          remoteLoggingConfig: _config.copyWith(gatewayUrl: baseUrl),
          deviceDataGenerator: TestDeviceDataGenerator(),
        );
        final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

        final log = logger('levelTest');
        log.severe('Error message');
        log.warning('Warning message');
        log.info('Info message');

        await Future.any<dynamic>([
          remoteLogger.start(invoker),
          Future.doWhile(() async {
            await Future<void>.delayed(const Duration(milliseconds: 100));
            return capturedLevels.isEmpty;
          }),
          Future.delayed(const Duration(seconds: 15), () async {
            await server.close();
            fail('Timeout waiting for logs');
          }),
        ]);

        expect(capturedLevels, isNotEmpty);
        expect(capturedLevels.contains('SEVERE'), true);
        expect(capturedLevels.contains('WARNING'), true);
        expect(capturedLevels.contains('INFO'), true);
        await server.close();
      });
    });
  });
}

RemoteLoggingConfig get _config => const RemoteLoggingConfig(
  gatewayUrl: 'http://localhost:8080',
  channelId: 'test-channel',
  localLogPrintLevel: Level.FINEST,
  logSendLevel: Level.FINEST,
  logPrintLevel: Level.FINEST,
);

class TestDeviceDataGenerator implements IDeviceDataGenerator {
  @override
  Future<void> init() async {
    _deviceIdCache = 'test-device-id';
    _sessionIdCache = _generateRandomId();
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
      os: 'test_os',
      osVersion: 'test_os_version',
      model: 'test_model',
      brand: 'test_brand',
      device: 'test_device',
      identifier: 'test_identifier',
      manufacturer: 'test_manufacturer',
      isPhysicalDevice: 'true',
    );
  }

  String _generateRandomId() {
    return const Uuid().v4();
  }
}
