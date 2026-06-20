import 'package:remote_logging/remote_logging.dart';

/// A mixin that provides a `late final` [Logger] instance for the class `T`.
///
/// The logger is named after the class `T.toString()`.
///
/// Example usage:
/// ```dart
/// import 'package:remote_logging/remote_logging.dart';
///
/// class MyClass with LoggerMixin<MyClass> {
///   void doSomething() {
///     log.info('Doing something');
///   }
/// }
/// ```
///
/// See also:
/// * [LoggerMixinConst] for classes that need to remain constant.
mixin LoggerMixin<T> {
  /// The [Logger] instance for this class.
  late final Logger log = logger(T.toString());
}

/// A mixin that provides a [Logger] getter for the class `T`.
///
/// Use this mixin when the class needs to remain constant, as it provides
/// the logger through a getter rather than a `late final` field.
///
/// Example usage:
/// ```dart
/// import 'package:remote_logging/remote_logging.dart';
///
/// class MyConstantClass with LoggerMixinConst<MyConstantClass> {
///   const MyConstantClass();
///
///   void doSomething() {
///     log.info('Doing something');
///   }
/// }
/// ```
///
/// See also:
/// * [LoggerMixin] for classes that can use a `late final` field. Prefer
/// [LoggerMixin] if constant classes are not needed.
mixin LoggerMixinConst<T> {
  /// Returns the [Logger] instance for this class.
  Logger get log => logger(T.toString());
}
