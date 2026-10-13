// Ignores redundant argument values for explicit test parameter setup.
// ignore_for_file: avoid_redundant_argument_values

import 'dart:convert';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:http_test_server/http_test_server.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

import 'helpers/test_helpers.dart';

void main() {
  test('logs below logSendLevel should not be sent', () async {
    var batchSent = false;
    final server = await TestServer.createHttpServer(
      events: [
        StandardServerEvent(
          matcher: (request) =>
              request.method == 'POST' && request.uri.path == '/auth/exchange',
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
      remoteLoggingLibraryLogPrintLevel: Level.FINEST,
    );

    final remoteLogging = await RemoteLoggingInit.initRemote(
      config: configWithFiltering,
      deviceDataGenerator: TestDeviceDataGenerator(),
    );
    final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

    logger('filterTest')
      ..fine('Fine level message - should not be sent')
      ..warning('Warning level message - should be sent');

    await Future.any<dynamic>([
      remoteLogging.start(invoker),
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
}
