import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/model/auth_challenge_response.dart';
import 'package:remote_logging/src/model/auth_enroll_response.dart';
import 'package:remote_logging/src/model/auth_exchange_response.dart';
import 'package:remote_logging/src/model/gateway_error.dart';
import 'package:remote_logging/src/remote/auth_service.dart';
import 'package:remote_logging/src/remote/request/request_auth_challenge.dart';
import 'package:remote_logging/src/remote/request/request_auth_enroll.dart';
import 'package:remote_logging/src/remote/request/request_auth_exchange.dart';
import 'package:test/test.dart';

class MockInvoker implements INetworkInvoker {
  MockInvoker({
    this.challengeHandler,
    this.enrollHandler,
    this.exchangeHandler,
  });

  FutureOr<ResponseResult<AuthChallengeResponse>> Function(
    RequestAuthChallenge command,
  )?
  challengeHandler;

  FutureOr<ResponseResult<AuthEnrollResponse>> Function(
    RequestAuthEnroll command,
  )?
  enrollHandler;

  FutureOr<ResponseResult<AuthExchangeResponse>> Function(
    RequestAuthExchange command,
  )?
  exchangeHandler;

  final List<RequestCommand<dynamic>> sentCommands = [];

  @override
  Future<ResponseResult<T>> send<T extends Schema>(
    RequestCommand<T> command,
  ) async {
    sentCommands.add(command);
    if (command is RequestAuthChallenge) {
      if (challengeHandler != null) {
        return (await challengeHandler!(command as RequestAuthChallenge))
            as ResponseResult<T>;
      }
      return SuccessResponseResult<AuthChallengeResponse>(
            statusCode: 200,
            headers: {},
            data: const AuthChallengeResponse(
              challenge: 'test-challenge.123.sig',
              expiresIn: 300,
            ),
          )
          as ResponseResult<T>;
    }
    if (command is RequestAuthEnroll) {
      if (enrollHandler != null) {
        return (await enrollHandler!(command as RequestAuthEnroll))
            as ResponseResult<T>;
      }
      return SuccessResponseResult<AuthEnrollResponse>(
            statusCode: 200,
            headers: {},
            data: const AuthEnrollResponse(
              enrollmentToken: 'mock.enrollment.token',
              expiresIn: 7776000,
            ),
          )
          as ResponseResult<T>;
    }
    if (command is RequestAuthExchange) {
      if (exchangeHandler != null) {
        return (await exchangeHandler!(command as RequestAuthExchange))
            as ResponseResult<T>;
      }
      return SuccessResponseResult<AuthExchangeResponse>(
            statusCode: 200,
            headers: {},
            data: const AuthExchangeResponse(
              accessToken: 'jwt.access.token',
              accessExpiresIn: 86400,
            ),
          )
          as ResponseResult<T>;
    }
    throw UnimplementedError('Command $command not handled');
  }
}

class TestAttestationProvider implements AttestationProvider {
  bool supported = true;
  bool throwOnFirstProve = false;
  String? storedToken;
  int enrollCallCount = 0;
  int proveCallCount = 0;
  int resetCallCount = 0;

  @override
  Future<bool> isSupported() async => supported;

  @override
  Future<EnrollmentMaterial> enroll(String challenge) async {
    enrollCallCount++;
    return const EnrollmentMaterial(
      publicKey: 'mock-pub-key-b64',
      attestation: 'mock-attestation-statement',
    );
  }

  @override
  Future<String> prove(
    String challenge,
    String sessionId,
    String deviceId,
  ) async {
    proveCallCount++;
    if (throwOnFirstProve && proveCallCount == 1) {
      throw Exception('KEY_NOT_FOUND: Hardware key invalidated');
    }
    return 'mock-p256-signature-b64';
  }

  @override
  Future<String?> loadEnrollmentToken() async => storedToken;

  @override
  Future<void> saveEnrollmentToken(String token) async {
    storedToken = token;
  }

  @override
  Future<void> reset() async {
    resetCallCount++;
    storedToken = null;
  }
}

void main() {
  const config = RemoteLoggingConfig(
    gatewayUrl: 'https://logger.example.com',
    channelId: 'test-channel',
  );

  const metadata = DeviceMetadata(
    timestamp: '2026-10-14T00:00:00Z',
    deviceId: 'device-123',
    sessionId: 'session-abc',
    os: 'android',
    osVersion: '14',
    model: 'Pixel 8',
    brand: 'Google',
    device: 'husky',
    identifier: 'husky',
    manufacturer: 'Google',
    isPhysicalDevice: 'true',
  );

  group('AuthService Hardware Attestation Flow', () {
    test(
      'full flow when supported: challenge -> enroll -> prove -> exchange',
      () async {
        final invoker = MockInvoker();
        final provider = TestAttestationProvider();

        final authService = AuthService(
          config: config,
          deviceMetadata: metadata,
          invoker: invoker,
          attestationProvider: provider,
        );

        await authService.exchangeAuthToken();

        expect(authService.hasValidToken, isTrue);
        expect(authService.accessToken, 'jwt.access.token');
        expect(provider.enrollCallCount, 1);
        expect(provider.proveCallCount, 1);
        expect(provider.storedToken, 'mock.enrollment.token');

        // Check the 3 commands sent in sequence
        expect(invoker.sentCommands.length, 3);
        expect(invoker.sentCommands[0], isA<RequestAuthChallenge>());
        expect(invoker.sentCommands[1], isA<RequestAuthEnroll>());
        expect(invoker.sentCommands[2], isA<RequestAuthExchange>());

        final exchangeCmd = invoker.sentCommands[2] as RequestAuthExchange;
        expect(exchangeCmd.request.enrollmentToken, 'mock.enrollment.token');
        expect(exchangeCmd.request.challenge, 'test-challenge.123.sig');
        expect(
          exchangeCmd.request.challengeSignature,
          'mock-p256-signature-b64',
        );
      },
    );

    test('reuses existing enrollment token without re-enrolling', () async {
      final invoker = MockInvoker();
      final provider = TestAttestationProvider()
        ..storedToken = 'pre-existing.token';

      final authService = AuthService(
        config: config,
        deviceMetadata: metadata,
        invoker: invoker,
        attestationProvider: provider,
      );

      await authService.exchangeAuthToken();

      expect(authService.hasValidToken, isTrue);
      expect(provider.enrollCallCount, 0); // Reused token!
      expect(provider.proveCallCount, 1);

      // Sent challenge + exchange only (no enroll)
      expect(invoker.sentCommands.length, 2);
      expect(invoker.sentCommands[0], isA<RequestAuthChallenge>());
      expect(invoker.sentCommands[1], isA<RequestAuthExchange>());

      final exchangeCmd = invoker.sentCommands[1] as RequestAuthExchange;
      expect(exchangeCmd.request.enrollmentToken, 'pre-existing.token');
    });

    test(
      'resets and retries enrollment once if exchange with token fails (403)',
      () async {
        final invoker = MockInvoker();
        final provider = TestAttestationProvider()..storedToken = 'stale.token';

        var exchangeAttempts = 0;
        invoker.exchangeHandler = (cmd) {
          exchangeAttempts++;
          if (exchangeAttempts == 1) {
            return SpecifiedResponseResult<AuthExchangeResponse>(
              statusCode: 403,
              headers: {},
              data: const GatewayError(error: 'Forbidden', code: 'FORBIDDEN'),
              type: GatewayError,
            );
          }
          return SuccessResponseResult<AuthExchangeResponse>(
            statusCode: 200,
            headers: {},
            data: const AuthExchangeResponse(
              accessToken: 'retried.token',
              accessExpiresIn: 86400,
            ),
          );
        };

        final authService = AuthService(
          config: config,
          deviceMetadata: metadata,
          invoker: invoker,
          attestationProvider: provider,
        );

        await authService.exchangeAuthToken();

        expect(authService.hasValidToken, isTrue);
        expect(authService.accessToken, 'retried.token');
        expect(provider.resetCallCount, 1);
        expect(provider.enrollCallCount, 1);
        expect(exchangeAttempts, 2);
      },
    );

    test(
      'resets and retries enrollment once if prove() throws (key lost locally)',
      () async {
        final invoker = MockInvoker();
        final provider = TestAttestationProvider()
          ..storedToken = 'cached.token.without.key'
          ..throwOnFirstProve = true;

        final authService = AuthService(
          config: config,
          deviceMetadata: metadata,
          invoker: invoker,
          attestationProvider: provider,
        );

        await authService.exchangeAuthToken();

        expect(authService.hasValidToken, isTrue);
        expect(authService.accessToken, 'jwt.access.token');
        expect(provider.resetCallCount, 1);
        expect(provider.enrollCallCount, 1);
        expect(provider.proveCallCount, 2);
      },
    );

    test(
      'does NOT reset hardware key on 5xx server error during exchange',
      () async {
        final invoker = MockInvoker();
        final provider = TestAttestationProvider()
          ..storedToken = 'valid.90day.token';

        invoker.exchangeHandler = (cmd) {
          return SpecifiedResponseResult<AuthExchangeResponse>(
            statusCode: 503,
            headers: {},
            data: const GatewayError(
              error: 'Service Unavailable',
              code: 'KAFKA_UNAVAILABLE',
            ),
            type: GatewayError,
          );
        };

        final authService = AuthService(
          config: config,
          deviceMetadata: metadata,
          invoker: invoker,
          attestationProvider: provider,
        );

        await expectLater(
          authService.exchangeAuthToken(),
          throwsA(isA<Exception>()),
        );

        expect(provider.resetCallCount, 0);
        expect(provider.enrollCallCount, 0);
        expect(provider.storedToken, 'valid.90day.token');
      },
    );

    test(
      'falls back to direct exchange when attestation is not supported',
      () async {
        final invoker = MockInvoker();
        final provider = TestAttestationProvider()..supported = false;

        final authService = AuthService(
          config: config,
          deviceMetadata: metadata,
          invoker: invoker,
          attestationProvider: provider,
          getAttestationToken: () async => 'legacy-token',
        );

        await authService.exchangeAuthToken();

        expect(authService.hasValidToken, isTrue);
        expect(provider.enrollCallCount, 0);
        expect(provider.proveCallCount, 0);

        expect(invoker.sentCommands.length, 1);
        final exchangeCmd = invoker.sentCommands[0] as RequestAuthExchange;
        expect(exchangeCmd.request.attestationToken, 'legacy-token');
        expect(exchangeCmd.request.enrollmentToken, isNull);
      },
    );
  });
}
