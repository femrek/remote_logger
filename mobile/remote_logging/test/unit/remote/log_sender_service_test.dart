// Rationale: explicit default values make test intent clearer;
// unnecessary_lambdas false-positives on mocktail thenAnswer calls.
// ignore_for_file: avoid_redundant_argument_values, unnecessary_lambdas

import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:mocktail/mocktail.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/remote/auth_service.dart';
import 'package:remote_logging/src/remote/log_sender_service.dart';
import 'package:test/test.dart';

// ---------------------------------------------------------------------------
// Fakes & Mocks
// ---------------------------------------------------------------------------

class _FakeRequestCommand extends Fake
    implements RequestCommand<IgnoredSchema> {}

class MockINetworkInvoker extends Mock implements INetworkInvoker {}

class MockILocalLogRepository extends Mock implements LocalLogRepository {}

/// Invoker that should never be called — used as a placeholder inside
/// [_StubAuthService] constructor so auth never hits the network.
class _NeverInvoker implements INetworkInvoker {
  @override
  Future<NetworkResult<T>> send<T extends Schema>(
    RequestCommand<T> request,
  ) async {
    throw StateError('_NeverInvoker.send() should not be called');
  }
}

/// Concrete AuthService whose [getValidAccessToken] never calls the network.
class _StubAuthService extends AuthService {
  _StubAuthService()
      : super(
          config: const RemoteLoggingConfig(
            gatewayUrl: 'http://stub.test',
            channelId: 'stub',
          ),
          deviceMetadata: const DeviceMetadata(
            timestamp: '2024-01-01T00:00:00Z',
            deviceId: 'stub-device',
            sessionId: 'stub-session',
            os: 'test',
            osVersion: '0',
            model: 'stub',
            brand: 'stub',
            device: 'stub',
            identifier: 'stub',
            manufacturer: 'stub',
            isPhysicalDevice: 'false',
          ),
          invoker: _NeverInvoker(),
        );

  @override
  Future<String?> getValidAccessToken() async => 'test-token';
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const _testConfig = RemoteLoggingConfig(
  gatewayUrl: 'http://test.com',
  channelId: 'ch',
);

const _disabledConfig = RemoteLoggingConfig(
  gatewayUrl: 'http://test.com',
  channelId: 'ch',
  enableRemoteLogging: false,
);

SuccessResponseResult<IgnoredSchema> _successResult() =>
    SuccessResponseResult<IgnoredSchema>(
      data: const IgnoredSchema(),
      statusCode: 200,
      headers: {},
    );

NetworkErrorResult<IgnoredSchema> _networkErrorResult() =>
    NetworkErrorResult<IgnoredSchema>(
      error: const NetworkError(
        message: 'test error',
        stackTrace: StackTrace.empty,
      ),
    );

LogRecord _makeRecord({String message = 'test message'}) => LogRecord(
      Level.INFO,
      message,
      'TestLogger',
    );

LogSenderService _makeService({
  RemoteLoggingConfig config = _testConfig,
  LocalLogRepository? localRepository,
  void Function(Object, StackTrace)? onError,
}) =>
    LogSenderService(
      config: config,
      localRepository: localRepository,
      onError: onError,
    );

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

void main() {
  late MockINetworkInvoker mockInvoker;
  late _StubAuthService stubAuthService;

  setUpAll(() {
    registerFallbackValue(_FakeRequestCommand());
  });

  setUp(() {
    mockInvoker = MockINetworkInvoker();
    stubAuthService = _StubAuthService();
  });

  group('LogSenderService', () {
    group('flush() before start()', () {
      test('is a no-op and does not call invoker.send()', () async {
        final service = _makeService();

        await (service..enqueue(_makeRecord())).flush();

        verifyNever(() => mockInvoker.send<IgnoredSchema>(any()));
      });
    });

    group('enqueue() + flush()', () {
      test('causes invoker.send() to be called once', () async {
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async => _successResult());

        final service = _makeService();
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.flush();

        verify(() => mockInvoker.send<IgnoredSchema>(any())).called(1);
      });
    });

    group('after successful send', () {
      test('localRepository.removeLogs() is called', () async {
        final mockRepo = MockILocalLogRepository();
        when(() => mockRepo.saveLogs(any())).thenAnswer((_) async {});
        when(() => mockRepo.getUnsentLogs())
            .thenAnswer((_) async => <ServerLog>[]);
        when(() => mockRepo.removeLogs(any())).thenAnswer((_) async {});
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async => _successResult());

        final service = _makeService(localRepository: mockRepo);
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.flush();

        // Give the unawaited removeLogs call time to complete.
        await Future<void>.delayed(Duration.zero);

        verify(() => mockRepo.removeLogs(any())).called(1);
      });
    });

    group('after network error', () {
      test('the batch is stored in failed requests', () async {
        var callCount = 0;
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async {
          callCount++;
          if (callCount == 1) return _networkErrorResult();
          return _successResult();
        });

        final service = _makeService();
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.flush(); // fails → stored in _failedBatches

        // Second flush triggers _resendFailedRequests → invoker called again.
        await service.flush();

        expect(callCount, 2); // 1 fail + 1 retry
      });
    });

    group('retry cycle', () {
      test('retries the failed batch on the next flush cycle', () async {
        var callCount = 0;
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async {
          callCount++;
          if (callCount == 1) return _networkErrorResult();
          return _successResult();
        });

        final service = _makeService();
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.flush(); // fails → 1 batch in _failedBatches

        await service.flush(); // retries and succeeds

        expect(callCount, 2);
      });

      test('successful retry removes the batch from failed requests', () async {
        var callCount = 0;
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async {
          callCount++;
          if (callCount == 1) return _networkErrorResult();
          return _successResult();
        });

        final service = _makeService();
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.flush(); // fails → batch stored

        await service.flush(); // retry succeeds → batch removed
        await service.flush(); // queue empty, no failed batches → no new calls

        expect(callCount, 2); // only initial fail + one retry
      });
    });

    group('when enableRemoteLogging is false', () {
      test('start() does not call invoker even after enqueue + flush',
          () async {
        final service = _makeService(config: _disabledConfig);
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.flush();

        verifyNever(() => mockInvoker.send<IgnoredSchema>(any()));
      });
    });

    group('maxPendingLogCount eviction', () {
      test('oldest batch is evicted when limit would be exceeded', () async {
        const tightConfig = RemoteLoggingConfig(
          gatewayUrl: 'http://test.com',
          channelId: 'ch',
          maxPendingLogCount: 1,
        );

        // Phase 1: always fail so batches accumulate / get evicted.
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async => _networkErrorResult());

        final service = _makeService(config: tightConfig);
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord(message: 'log 1'));
        await service.flush(); // fails → batch-1 stored

        service.enqueue(_makeRecord(message: 'log 2'));
        await service.flush(); // fails → batch-1 evicted, batch-2 stored

        // Phase 2: switch to success and run a retry.
        var retryCount = 0;
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async {
          retryCount++;
          return _successResult();
        });

        // Only the surviving batch (batch-2) should be retried.
        await service.flush();
        expect(retryCount, 1);
      });
    });

    group('local repository interactions', () {
      test(
          'getUnsentLogs() is called on start() and those logs are sent '
          'immediately', () async {
        final mockRepo = MockILocalLogRepository();
        final unsentLog = ServerLog(
          id: 'unsent-1',
          timestamp: DateTime(2024),
          loggerName: 'test',
          message: 'unsent',
          level: 'INFO',
          zone: 'root',
        );

        when(() => mockRepo.getUnsentLogs())
            .thenAnswer((_) async => [unsentLog]);
        when(() => mockRepo.removeLogs(any())).thenAnswer((_) async {});
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async => _successResult());

        final service = _makeService(localRepository: mockRepo);
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        verify(() => mockRepo.getUnsentLogs()).called(1);
        // start() calls _sendAll() immediately — the unsent log is dispatched.
        verify(() => mockInvoker.send<IgnoredSchema>(any())).called(1);
      });

      test('saveLogs() is called on enqueue()', () async {
        final mockRepo = MockILocalLogRepository();
        when(() => mockRepo.saveLogs(any())).thenAnswer((_) async {});
        when(() => mockRepo.getUnsentLogs())
            .thenAnswer((_) async => <ServerLog>[]);
        when(() => mockRepo.removeLogs(any())).thenAnswer((_) async {});
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async => _successResult());

        final service = _makeService(localRepository: mockRepo);
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());

        // Give the unawaited saveLogs call time to complete.
        await Future<void>.delayed(Duration.zero);

        verify(() => mockRepo.saveLogs(any())).called(1);
      });
    });

    group('stop()', () {
      test('stop(flush: true) calls flush before cancelling the subscription',
          () async {
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async => _successResult());

        final service = _makeService();
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.stop(flush: true);

        verify(() => mockInvoker.send<IgnoredSchema>(any())).called(1);
      });

      test('stop(flush: false) cancels immediately without calling send',
          () async {
        when(() => mockInvoker.send<IgnoredSchema>(any()))
            .thenAnswer((_) async => _successResult());

        final service = _makeService();
        await service.start(
          invoker: mockInvoker,
          authService: stubAuthService,
        );

        service.enqueue(_makeRecord());
        await service.stop(flush: false);

        verifyNever(() => mockInvoker.send<IgnoredSchema>(any()));
      });
    });
  });
}
