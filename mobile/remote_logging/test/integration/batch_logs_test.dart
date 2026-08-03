// Ignores redundant argument values for explicit test parameter setup.
// ignore_for_file: avoid_redundant_argument_values

import 'dart:convert';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:http_test_server/http_test_server.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

import 'helpers/test_helpers.dart';

void main() {
  test('multiple log messages are batched correctly', () async {
      var batchCount = 0;
      var totalLogsReceived = 0;
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
              totalLogsReceived += jsonData.length;
              batchCount++;
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

      final log = logger('batchTest');
      for (var i = 0; i < 5; i++) {
        log.info('Batch message $i');
      }

      await Future.any<dynamic>([
        remoteLogging.start(invoker),
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
      expect(totalLogsReceived, 5);
      await server.close();
  });
}
