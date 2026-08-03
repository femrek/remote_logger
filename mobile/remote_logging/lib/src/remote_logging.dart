import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/remote/auth_service.dart';
import 'package:remote_logging/src/remote/log_sender_service.dart';
import 'package:remote_logging/src/util/helpers.dart';
import 'package:remote_logging/src/util/internal_logger.dart';

/// Initializer for [RemoteLogging] and [LocalLogging].
abstract final class RemoteLoggingInit {
  static bool _isInitialized = false;

  /// Initializes remote logging with the specified configuration and device
  /// data generator.
  static Future<RemoteLogging> initRemote({
    required RemoteLoggingConfig config,
    required IDeviceDataGenerator deviceDataGenerator,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    ILocalLogRepository? localLogRepository,
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

  /// Initializes local logging with the specified configuration.
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
}

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
    required LogSenderService logSenderService,
    required RemoteLoggingConfig config,
    required IDeviceDataGenerator deviceDataGenerator,
    OnLog? onLog,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    OnError? onError,
  }) : _deviceDataGenerator = deviceDataGenerator,
       _logSenderService = logSenderService,
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

  static bool _initialized = false;

  final LogSenderService _logSenderService;
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

  /// Initializes the logging system. Call this method before using any logging
  /// features.
  static Future<RemoteLogging> _init({
    required RemoteLoggingConfig remoteLoggingConfig,
    required IDeviceDataGenerator deviceDataGenerator,
    GetAttestationTokenCallback? getAttestationTokenCallback,
    ILocalLogRepository? localLogRepository,
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
    _initialized = true;

    return instance;
  }

  /// Starts the log sender service to send logs to the remote server.
  ///
  /// DO NOT AWAIT THIS FUNCTION. It is designed to run indefinitely, sending
  /// logs as they are generated.
  Future<void> start(INetworkInvoker invoker) async {
    internalLog.config('Starting RemoteLogging with config: $config');
    if (!_initialized) {
      throw StateError(
        'LoggerInitializer.init() must be called'
        ' before LoggerInitializer.start()',
      );
    }

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
      deviceMetadata: metadata,
      authService: authService,
    );
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

/// Manages local logging configuration and log listeners.
///
/// Sets up root logger listeners and handles log record formatting
/// and filtering.
class LocalLogging {
  /// Creates a [LocalLogging] instance with the provided configuration and
  /// optional log callback.
  ///
  /// The local only variant of [RemoteLogging]. Do not use [LocalLogging] and
  /// [RemoteLogging] together, as they will conflict with each other.
  ///
  /// [config] is required and defines the logging behavior.
  /// [onLog] is an optional callback that will be invoked for each log record.
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

  void _onLog(LogRecord record) {
    if (isLoggable(record, _config.logPrintLevel)) {
      _config.logPrinter(record);
    }
  }
}
