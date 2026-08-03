// ignore_for_file: avoid_redundant_argument_values test file

import 'dart:convert';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:http_test_server/http_test_server.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

import 'helpers/test_helpers.dart';

void main() {
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

      final remoteLogging = await RemoteLoggingInit.initRemote(
        config: testConfig.copyWith(gatewayUrl: baseUrl),
        deviceDataGenerator: TestDeviceDataGenerator(),
      );
      final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

      logger('levelTest')
        ..severe('Error message')
        ..warning('Warning message')
        ..info('Info message');

      await Future.any<dynamic>([
        remoteLogging.start(invoker),
        Future.doWhile(() async {
          await Future<void>.delayed(const Duration(milliseconds: 100));
          return capturedLevels.isEmpty;
        }),
        Future.delayed(const Duration(seconds: 15), () async {
          await server.close();
          fail('Timeout waiting for logs');
        }),
      ]);

      expect(capturedLevels, hasLength(3));
      expect(capturedLevels, contains('SEVERE'));
      expect(capturedLevels, contains('WARNING'));
      expect(capturedLevels, contains('INFO'));
      await server.close();
  });
}
