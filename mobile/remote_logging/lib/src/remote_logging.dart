import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/remote/auth_service.dart';
import 'package:remote_logging/src/remote/i_log_sender_service.dart';
import 'package:remote_logging/src/remote/log_sender_service.dart';
import 'package:remote_logging/src/util/helpers.dart';
import 'package:remote_logging/src/util/internal_logger.dart';

/// Initializer for [RemoteLogging] and [LocalLogging].
///
/// Use [initRemote] to set up remote log transmission, or [initLocal] for
/// console-only logging without any network transport. Only one may be
/// initialized at a time.
abstract final class RemoteLoggingInit {
  static bool _isInitialized = false;

  /// Initializes remote logging with the specified configuration and device
  /// data generator.
  ///
  /// Throws a [StateError] if called more than once without calling
  /// [RemoteLogging.stop] first.
  static Future<RemoteLogging> initRemote({
    required RemoteLoggingConfig config,
    required IDeviceDataGenerator deviceDataGenerator,
    ILocalLogRepository localLogRepository = const NoSaveLocalLogRepository(),
    GetAttestationTokenCallback? getAttestationTokenCallback,
    OnLog? onLog,
    OnError? onError,
  }) async {
    if (_isInitialized) {
      throw StateError('RemoteLogging is already initialized');
    }
    _isInitialized = true;

    return RemoteLogging._init(
      remoteLoggingConfig: config,
      deviceDataGenerator: deviceDataGenerator,
      getAttestationTokenCallback: getAttestationTokenCallback,
      localLogRepository: localLogRepository,
      onLog: onLog,
      onError: onError,
    );
  }

  /// Initializes local-only logging with the specified configuration.
  ///
  /// Throws a [StateError] if called more than once without calling
  /// [LocalLogging.stop] first.
  static LocalLogging initLocal({
    required LocalLoggingConfig config,
    OnLog? onLog,
  }) {
    if (_isInitialized) {
      throw StateError('LocalLogging is already initialized');
    }
    _isInitialized = true;

    return LocalLogging._internal(
      config: config,
      onLog: onLog,
    );
  }

  /// Resets the initialization flag. Called internally by [RemoteLogging.stop]
  /// and [LocalLogging.stop].
  static void _reset() => _isInitialized = false;
}

// Level.OFF
// Level.SHOUT
// Level.SEVERE
// Level.WARNING
// Level.INFO
// Level.CONFIG
// Level.FINE
// Level.FINER
// Level.FINEST
/// Manages remote logging: collects [LogRecord]s and transmits them to the
/// remote gateway in batches.
///
/// Obtain an instance via [RemoteLoggingInit.initRemote].
final class RemoteLogging {
  RemoteLogging._internal({
    required ILogSenderService logSenderService,
    required RemoteLoggingConfig config,
    required IDeviceDataGenerator deviceDataGenerator,
    OnLog? onLog,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    OnError? onError,
  }) : _logSenderService = logSenderService,
       _deviceDataGenerator = deviceDataGenerator,
       _config = config,
       _getAttestationTokenCallback = getAttestationTokenCallback,
       _onError = onError {
    Logger.root.level = getBiggerScopeLevel(
      _config.logSendLevel,
      _config.logPrintLevel,
    );
    Logger.root.onRecord.listen((record) {
      if (onLog != null) onLog(record);
      _onLog(record);
    });
  }

  final ILogSenderService _logSenderService;
  final IDeviceDataGenerator _deviceDataGenerator;
  final OnError? _onError;
  final GetAttestationTokenCallback? _getAttestationTokenCallback;
  RemoteLoggingConfig _config;

  /// Gets the current configuration.
  RemoteLoggingConfig get config => _config;

  /// Sets the configuration dynamically, updating the root logger level.
  set config(RemoteLoggingConfig newConfig) {
    if (_config != newConfig) {
      _config = newConfig;
      Logger.root.level = getBiggerScopeLevel(
        _config.logSendLevel,
        _config.logPrintLevel,
      );
      internalLog.config('RemoteLogging config updated to: $_config');
    } else {
      internalLog.config(
        'RemoteLogging config set to the same value: $_config',
      );
    }
  }

  static Future<RemoteLogging> _init({
    required RemoteLoggingConfig remoteLoggingConfig,
    required IDeviceDataGenerator deviceDataGenerator,
    required ILocalLogRepository localLogRepository,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    OnLog? onLog,
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
      onError: onError,
    );

    initInternalLogger(remoteLoggingConfig);

    await deviceDataGenerator.init();

    return instance;
  }

  /// Starts the log sender service to send logs to the remote server.
  ///
  /// DO NOT AWAIT THIS FUNCTION. It is designed to run until [stop] is called.
  Future<void> start(INetworkInvoker invoker) async {
    internalLog.config('Starting RemoteLogging with config: $config');

    if (!config.enableRemoteLogging) {
      internalLog.warning(
        'Remote logging is disabled. '
        'Logs will not be sent to remote endpoints.',
      );
      return;
    }

    final metadata = _deviceDataGenerator.deviceMetadata;
    final authService = AuthService(
      config: config,
      deviceMetadata: metadata,
      invoker: invoker,
      getAttestationToken: _getAttestationTokenCallback,
      onError: _onError,
    );

    await _logSenderService.start(
      invoker: invoker,
      authService: authService,
    );
  }

  /// Stops the log sender service and resets the initialization state so
  /// [RemoteLoggingInit.initRemote] can be called again if needed.
  ///
  /// If [flush] is `true` (the default), any pending logs in the queue are
  /// sent before the service stops.
  Future<void> stop({bool flush = true}) async {
    await _logSenderService.stop(flush: flush);
    RemoteLoggingInit._reset();
  }

  void _onLog(LogRecord record) {
    if (isLoggable(record, _config.logSendLevel)) {
      _logSenderService.enqueue(record);
    }

    if (isLoggable(record, _config.logPrintLevel)) {
      _config.logPrinter(record);
    }
  }

  /// Flushes any pending logs immediately. Useful for testing or when shutting
  /// down.
  Future<void> flush() async {
    await _logSenderService.flush();
  }
}

/// Manages local-only logging: prints [LogRecord]s to the console without any
/// network transport.
///
/// Use this instead of [RemoteLogging] when you don't need remote transmission.
/// Do not use [LocalLogging] and [RemoteLogging] together — they will conflict
/// by both registering listeners on [Logger.root].
///
/// Obtain an instance via [RemoteLoggingInit.initLocal].
class LocalLogging {
  LocalLogging._internal({
    required LocalLoggingConfig config,
    OnLog? onLog,
  }) : _config = config {
    Logger.root.level = _config.logPrintLevel;

    Logger.root.onRecord.listen((record) {
      if (onLog != null) onLog(record);
      _onLog(record);
    });

    initInternalLogger(config);
  }

  LocalLoggingConfig _config;

  /// Gets the current configuration.
  LocalLoggingConfig get config => _config;

  /// Sets the configuration dynamically, updating the root logger level.
  set config(LocalLoggingConfig newConfig) {
    if (_config != newConfig) {
      _config = newConfig;
      Logger.root.level = _config.logPrintLevel;
      internalLog.config('LocalLogging config updated to: $_config');
    } else {
      internalLog.config(
        'LocalLogging config set to the same value: $_config',
      );
    }
  }

  /// Stops local logging and resets the initialization state so
  /// [RemoteLoggingInit.initLocal] can be called again if needed.
  void stop() {
    RemoteLoggingInit._reset();
  }

  void _onLog(LogRecord record) {
    if (isLoggable(record, _config.logPrintLevel)) {
      _config.logPrinter(record);
    }
  }
}
