// Ignores redundant argument values for explicit test parameter setup.
// ignore_for_file: avoid_redundant_argument_values

import 'dart:convert';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:http_test_server/http_test_server.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

import 'helpers/test_helpers.dart';

void main() {
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
        remoteLoggingLibraryLogPrintLevel: Level.FINEST,
      );

      final remoteLogging = await RemoteLoggingInit.initRemote(
        config: disabledConfig,
        deviceDataGenerator: TestDeviceDataGenerator(),
      );
      final invoker = DioNetworkInvoker.fromBaseUrl(baseUrl);

      logger('disabledTest').info('This message should not be sent remotely');

      // start() returns immediately when remote logging is disabled
      await remoteLogging.start(invoker);

      // Brief wait to confirm no delayed delivery occurs
      await Future<void>.delayed(const Duration(milliseconds: 200));

      expect(batchSent, false);
      await server.close();
  });
}
