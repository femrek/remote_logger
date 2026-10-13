// ignore_for_file: avoid_redundant_argument_values see params clearly in test.

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:mocktail/mocktail.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/model/auth_exchange_response.dart';
import 'package:test/test.dart';

// RemoteLoggingInit._isInitialized is shared static state, so every test group
// must call tearDown to reset it via stop() / initLocal stop().

class _FakeAuthCommand extends Fake
    implements RequestCommand<AuthExchangeResponse> {}

class _MockNetworkInvoker extends Mock implements INetworkInvoker {}

void main() {
  setUpAll(() {
    registerFallbackValue(_FakeAuthCommand());
  });

  // Minimal config used across all tests.
  const config = RemoteLoggingConfig(
    gatewayUrl: 'http://test.example.com',
    channelId: 'test-channel',
    enableRemoteLogging: false, // avoid real network traffic in unit tests
  );

  group('RemoteLoggingInit.initRemote', () {
    late RemoteLogging logging;

    setUp(() async {
      logging = await RemoteLoggingInit.initRemote(
        config: config,
        deviceDataGenerator: AnonymousDeviceDataGenerator(),
      );
    });

    tearDown(() async {
      await logging.stop(flush: false);
    });

    test('returns a RemoteLogging instance', () {
      expect(logging, isA<RemoteLogging>());
    });

    test('config getter returns the provided configuration', () {
      expect(logging.config.channelId, equals('test-channel'));
    });

    test(
      'config setter updates Logger.root to the wider of the two levels',
      () {
        // logPrintLevel defaults to FINEST (widest), logSendLevel to INFO.
        // getBiggerScopeLevel returns FINEST (lower value = finer granularity).
        // To observe a change, set both levels to something narrower:
        logging.config = config.copyWith(
          logSendLevel: Level.SEVERE,
          logPrintLevel: Level.SEVERE,
        );
        expect(Logger.root.level, equals(Level.SEVERE));
      },
    );

    test('calling initRemote twice throws StateError', () async {
      expect(
        () => RemoteLoggingInit.initRemote(
          config: config,
          deviceDataGenerator: AnonymousDeviceDataGenerator(),
        ),
        throwsA(isA<StateError>()),
      );
    });

    test('flush() completes without error', () async {
      await expectLater(logging.flush(), completes);
    });

    test('stop(flush: false) completes without error', () async {
      await expectLater(logging.stop(flush: false), completes);
      // Prevent tearDown from calling stop again on an already-stopped instance
      // by re-initialising (guard is reset after stop).
      logging = await RemoteLoggingInit.initRemote(
        config: config,
        deviceDataGenerator: AnonymousDeviceDataGenerator(),
      );
    });

    test('stop() resets init flag so initRemote can be called again', () async {
      await logging.stop(flush: false);

      // Should NOT throw — guard was reset.
      final second = await RemoteLoggingInit.initRemote(
        config: config,
        deviceDataGenerator: AnonymousDeviceDataGenerator(),
      );
      addTearDown(() => second.stop(flush: false));
      expect(second, isA<RemoteLogging>());

      // Re-assign so the regular tearDown doesn't double-stop.
      logging = second;
    });
  });

  group('RemoteLoggingInit.initLocal', () {
    late LocalLogging localLogging;

    setUp(() {
      localLogging = RemoteLoggingInit.initLocal(
        config: const LocalLoggingConfig(),
      );
    });

    tearDown(() {
      localLogging.stop();
    });

    test('returns a LocalLogging instance', () {
      expect(localLogging, isA<LocalLogging>());
    });

    test('config getter returns the provided configuration', () {
      expect(localLogging.config.logPrintLevel, equals(Level.FINEST));
    });

    test('config setter updates the Logger.root level', () {
      localLogging.config = localLogging.config.copyWith(
        logPrintLevel: Level.WARNING,
      );
      expect(Logger.root.level, equals(Level.WARNING));
    });

    test('calling initLocal while already initialized throws StateError', () {
      expect(
        () => RemoteLoggingInit.initLocal(
          config: const LocalLoggingConfig(),
        ),
        throwsA(isA<StateError>()),
      );
    });

    test('stop() resets init flag so initLocal can be called again', () {
      localLogging.stop();

      final second = RemoteLoggingInit.initLocal(
        config: const LocalLoggingConfig(),
      );
      expect(second, isA<LocalLogging>());
      second.stop();

      // Re-assign so the regular tearDown doesn't double-stop.
      localLogging = RemoteLoggingInit.initLocal(
        config: const LocalLoggingConfig(),
      );
    });
  });

  group('RemoteLogging.start eager session startup exchange', () {
    test('eagerly calls invoker to exchange auth token on start', () async {
      final mockInvoker = _MockNetworkInvoker();
      when(() => mockInvoker.send<AuthExchangeResponse>(any())).thenAnswer(
        (_) async => SuccessResponseResult<AuthExchangeResponse>(
          data: const AuthExchangeResponse(
            accessToken: 'test-access',
            refreshToken: 'test-refresh',
            accessExpiresIn: 900,
            refreshExpiresIn: 604800,
          ),
          statusCode: 200,
          headers: {},
        ),
      );

      final logging = await RemoteLoggingInit.initRemote(
        config: const RemoteLoggingConfig(
          gatewayUrl: 'http://test.example.com',
          channelId: 'test-channel',
          enableRemoteLogging: true,
        ),
        deviceDataGenerator: AnonymousDeviceDataGenerator(),
      );

      await logging.start(mockInvoker);
      verify(() => mockInvoker.send<AuthExchangeResponse>(any())).called(1);
      await logging.stop(flush: false);
    });

    test('network error on start notifies onError without throwing', () async {
      final mockInvoker = _MockNetworkInvoker();
      Object? capturedError;
      when(() => mockInvoker.send<AuthExchangeResponse>(any())).thenAnswer(
        (_) async => NetworkErrorResult<AuthExchangeResponse>(
          error: const NetworkError(
            message: 'connection failed',
            stackTrace: StackTrace.empty,
          ),
        ),
      );

      final logging = await RemoteLoggingInit.initRemote(
        config: const RemoteLoggingConfig(
          gatewayUrl: 'http://test.example.com',
          channelId: 'test-channel',
          enableRemoteLogging: true,
        ),
        deviceDataGenerator: AnonymousDeviceDataGenerator(),
        onError: (err, _) {
          capturedError = err;
        },
      );

      // Must complete normally without throwing
      await expectLater(logging.start(mockInvoker), completes);
      expect(capturedError, isNotNull);
      await logging.stop(flush: false);
    });
  });
}
