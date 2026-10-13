// Ignores redundant argument values for explicit test parameter setup.
// ignore_for_file: avoid_redundant_argument_values

import 'dart:convert';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:http_test_server/http_test_server.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

import 'helpers/test_helpers.dart';

void main() {
  test('send log batch', () async {
    var serverSideExpectationsMet = false;
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

    final remoteLogging = await RemoteLoggingInit.initRemote(
      config: testConfig.copyWith(gatewayUrl: baseUrl),
      deviceDataGenerator: TestDeviceDataGenerator(),
    );
    final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

    logger('testLogger')
      ..info('First log message')
      ..warning('Second log message');

    await Future.any<dynamic>([
      remoteLogging.start(invoker),
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
}
