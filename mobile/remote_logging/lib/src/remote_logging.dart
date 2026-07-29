import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/remote/auth_service.dart';
import 'package:remote_logging/src/remote/log_sender_service.dart';
import 'package:remote_logging/src/util/log_record_extension.dart';

export 'package:logging/logging.dart';

/// A singleton logger that logs messages locally only.
///
/// Unlike [logger], this logger does not send logs to remote services. And this
/// cannot be named; it is always named 'LOCAL_ONLY_LOGGER'.
///
/// Example usage:
/// ```dart
/// import 'package:menu_app/product/logging/remote_logging.dart';
///
/// class MyClass {
///   void doSomething() {
///     localLog.info('Doing something');
///   }
/// }
final Logger localLog = _LocalLog()._logger;

/// A simple logging utility that wraps around the `logging` package.
/// It provides methods for logging messages at various levels (shout, severe,
/// warning, info, config, fine, finer, finest) and checks if the level is
/// loggable before logging the message.
///
/// Example usage:
/// ```dart
/// import 'package:menu_app/product/logging/remote_logging.dart';
///
/// class MyClass {
///   final _log = logger('MyApp');
///
///   void doSomething() {
///     _log.info('Doing something');
///   }
/// }
/// ```
///
/// Example usage with mixin (only for class members, not for static methods and
/// constructors):
/// ```dart
/// import 'package:menu_app/product/logging/remote_logging.dart';
///
/// class MyClass with LoggerMixin<MyClass> {
///   void doSomething() {
///     log.info('Doing something');
///   }
/// }
/// ```
Logger logger(String name) => Logger(name);

/// Callback type for taking an action when a log record is created.
///
/// Do not use for sending or printing logs; they are already handled by the
/// logging system. This is only for additional actions like analytics or
/// custom processing.
typedef OnLog = void Function(LogRecord record);

/// Callback type for handling background errors during log transmission.
///
/// This is used to report network errors or unexpected exceptions that occur
/// when the logging service is processing and sending log batches.
typedef OnError = void Function(Object error, StackTrace stackTrace);

/// Callback to fetch attestation token for authentication.
///
/// Can be null if the remote server is configured to not require attestation.
typedef GetAttestationTokenCallback = Future<String?> Function();

// Level.OFF
// Level.SHOUT
// Level.SEVERE
// Level.WARNING
// Level.INFO
// Level.CONFIG
// Level.FINE
// Level.FINER
// Level.FINEST
/// Application logger initializer that sets up logging configurations.
final class RemoteLogging {
  RemoteLogging._internal({
    required this.logSenderService,
    required RemoteLoggingConfig config,
    required this.deviceDataGenerator,
    OnLog? onLog,
    GetAttestationTokenCallback? getAttestationTokenCallback,
  }) : _config = config,
       _getAttestationTokenCallback = getAttestationTokenCallback {
    Logger.root.level = _getBiggerScopeLevel(
      _config.logSendLevel,
      _config.logPrintLevel,
    );
    Logger.root.onRecord.listen((record) {
      if (onLog != null) onLog(record);
      _onLog(record);
    });
  }

  RemoteLogging._internalLocalOnly({
    required RemoteLoggingConfig config,
    OnLog? onLog,
  }) : _config = config,
       _getAttestationTokenCallback = null,
       logSenderService = null,
       deviceDataGenerator = null {
    Logger.root.level = _getBiggerScopeLevel(
      _config.logSendLevel,
      _config.logPrintLevel,
    );

    Logger.root.onRecord.listen((record) {
      if (onLog != null) onLog(record);
      _onLog(record);
    });
  }

  /// The service responsible for sending logs to the remote server.
  final LogSenderService? logSenderService;

  /// The provider for device data, providing metadata for logs.
  final IDeviceDataGenerator? deviceDataGenerator;

  static bool _initialized = false;

  RemoteLoggingConfig _config;

  /// Gets the current configuration.
  RemoteLoggingConfig get config => _config;

  final GetAttestationTokenCallback? _getAttestationTokenCallback;

  /// Sets the configuration dynamically, updating the root logger level.
  set config(RemoteLoggingConfig newConfig) {
    if (_config != newConfig) {
      _config = newConfig;
      Logger.root.level = _getBiggerScopeLevel(
        _config.logSendLevel,
        _config.logPrintLevel,
      );
      localLog.config('RemoteLogging config updated to: $_config');
    } else {
      localLog.config('RemoteLogging config set to the same value: $_config');
    }
  }

  /// Initializes the logging system. Call this method before using any logging
  /// features.
  static Future<RemoteLogging> init({
    required RemoteLoggingConfig remoteLoggingConfig,
    required IDeviceDataGenerator deviceDataGenerator,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    ILocalLogRepository? localLogRepository,
    OnLog? onLog,
    OnLog? onLocalOnlyLog,
    OnError? onError,
  }) async {
    final logSenderService = LogSenderService(
      config: remoteLoggingConfig,
      deviceDataGenerator: deviceDataGenerator,
      localRepository: localLogRepository,
      onError: onError,
    );

    final instance = RemoteLogging._internal(
      logSenderService: logSenderService,
      config: remoteLoggingConfig,
      deviceDataGenerator: deviceDataGenerator,
      getAttestationTokenCallback: getAttestationTokenCallback,
      onLog: onLog,
    );

    _LocalLog().init(remoteLoggingConfig.localLogPrintLevel, onLocalOnlyLog);

    await deviceDataGenerator.init();
    _initialized = true;

    return instance;
  }

  /// unlike [init], initialize only for local usage.
  static RemoteLogging initLocalOnly({
    required RemoteLoggingConfig config,
    OnLog? onLog,
    OnLog? onLocalOnlyLog,
    OnError? onError,
  }) {
    final instance = RemoteLogging._internalLocalOnly(
      config: config,
      onLog: onLog,
    );

    _LocalLog().init(config.localLogPrintLevel, onLocalOnlyLog);

    return instance;
  }

  /// Optional error callback. Set dynamically or passed via `init`.
  OnError? onError;

  /// Starts the log sender service to send logs to the remote server.
  ///
  /// DO NOT AWAIT THIS FUNCTION. It is designed to run indefinitely, sending
  /// logs as they are generated.
  Future<void> start(INetworkInvoker invoker) async {
    localLog.config('Starting RemoteLogging with config: $config');
    if (!_initialized) {
      throw StateError(
        'LoggerInitializer.init() must be called'
        ' before LoggerInitializer.start()',
      );
    }

    if (!config.enableRemoteLogging) {
      localLog.warning(
        'Remote logging is disabled. '
        'Logs will not be sent to remote endpoints.',
      );
      return;
    }

    final logSenderService = this.logSenderService;
    if (logSenderService == null) {
      localLog.severe('Log sender service is not initialized.');
      return;
    }

    final deviceDataGenerator = this.deviceDataGenerator;
    if (deviceDataGenerator == null) {
      localLog.severe('Device data generator is not initialized.');
      return;
    }

    final metadata = deviceDataGenerator.deviceMetadata;
    final authService = AuthService(
      config: config,
      deviceMetadata: metadata,
      invoker: invoker,
      getAttestationToken: _getAttestationTokenCallback,
      onError: onError ?? logSenderService.onError,
    );

    await logSenderService.start(
      invoker: invoker,
      deviceMetadata: metadata,
      authService: authService,
    );
  }

  void _onLog(LogRecord record) {
    if (_isLoggable(record, _config.logSendLevel)) {
      logSenderService?.enqueue(record);
    }

    if (_isLoggable(record, _config.logPrintLevel)) {
      _printRecord(record);
    }
  }

  /// Flushes any pending logs immediately. Useful for testing or when shutting
  /// down.
  Future<void> flush() async {
    if (logSenderService == null) {
      localLog.severe('Log sender service is not initialized.');
      return;
    }
    await logSenderService?.flush();
  }
}

class _LocalLog {
  factory _LocalLog() => _instance ??= _LocalLog._internal();

  _LocalLog._internal();

  /// Initializes the local logger with the specified log level
  void init(Level level, OnLog? onLocalLog) {
    _logger = Logger.detached('LOCAL_ONLY_LOGGER');
    _logger.level = level;
    _logger.onRecord.listen((record) {
      onLocalLog?.call(record);
      _printRecord(record);
    });
  }

  static _LocalLog? _instance;
  late Logger _logger;
}

void _printRecord(LogRecord record) {
  print(
    '${record.time.toIso8601String()} '
    '[${record.zoneName}] '
    '[${_s7(record.level.name)}] '
    '${record.loggerName}: '
    '${record.message}',
  );

  if (record.error != null) {
    print('ERROR: ${record.error}');
  }
  if (record.stackTrace != null) {
    print('${record.stackTrace}');
  }
}

String _s7(String str) {
  if (str.length >= 7) {
    return str;
  } else {
    return str + ' ' * (7 - str.length);
  }
}

bool _isLoggable(LogRecord log, Level requiredLevel) {
  return log.level >= requiredLevel;
}

Level _getBiggerScopeLevel(Level level1, Level level2) {
  return level1.value < level2.value ? level1 : level2;
}
