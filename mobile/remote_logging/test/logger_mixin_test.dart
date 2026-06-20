import 'package:remote_logging/remote_logging.dart';
import 'package:test/test.dart';

void main() {
  group('logger mixin test', () {
    test(
      'should compile and log correctly when classes use the mixin',
      () async {
        final logMessages = <String>[];
        final subscription = Logger.root.onRecord.listen((record) {
          logMessages.add(record.message);
        });

        final loggableClass = _TestLoggerMixin()..foo();
        final constLoggableClass = const _TestLoggerMixinConst()..foo();

        expect(loggableClass.log.name, '_TestLoggerMixin');
        expect(constLoggableClass.log.name, '_TestLoggerMixinConst');
        expect(
          logMessages,
          contains('TEST LOG MESSAGE FROM _TestLoggerMixin'),
        );
        expect(
          logMessages,
          contains('TEST LOG MESSAGE FROM _TestLoggerMixinConst'),
        );

        await subscription.cancel();
      },
    );
  });
}

class _TestLoggerMixin with LoggerMixin<_TestLoggerMixin> {
  void foo() {
    log.info('TEST LOG MESSAGE FROM _TestLoggerMixin');
  }
}

class _TestLoggerMixinConst with LoggerMixinConst<_TestLoggerMixinConst> {
  const _TestLoggerMixinConst();

  void foo() {
    log.info('TEST LOG MESSAGE FROM _TestLoggerMixinConst');
  }
}
