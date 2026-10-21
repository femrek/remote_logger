import 'dart:async';
import 'dart:convert';

import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';
import 'package:remote_logging_example/models/console_log_item.dart';
import 'package:remote_logging_example/utils/jwt_helper.dart';

/// Controller managing connection state, hardware attestation testing,
/// and live diagnostic console logs.
class DiagnosticController extends ChangeNotifier {
  /// Creates a [DiagnosticController].
  DiagnosticController() {
    unawaited(checkHardwareSupportAndMetadata());
  }

  final TextEditingController gatewayUrlController = TextEditingController(
    text: 'https://hs-logger.femrek.dev',
  );
  final TextEditingController channelIdController = TextEditingController(
    text: 'test-channel',
  );
  final TextEditingController customLogController = TextEditingController();
  final ScrollController logScrollController = ScrollController();

  final Logger _appLogger = Logger('ExampleApp');

  bool _useHardwareAttestation = true;
  RemoteLogging? _remoteLogger;
  bool _isInitializing = false;
  bool _isFlushing = false;
  bool _isAttestationBusy = false;

  bool? _hardwareSupported;
  String? _hardwareCheckError;
  DeviceMetadata? _deviceMetadata;
  String? _storedEnrollmentToken;
  Map<String, dynamic>? _decodedTokenClaims;
  String? _lastEnrollResult;
  String? _lastProveResult;

  final List<ConsoleLogItem> _consoleLogs = [];

  // Getters
  bool get useHardwareAttestation => _useHardwareAttestation;
  bool get isConnected => _remoteLogger != null;
  bool get isInitializing => _isInitializing;
  bool get isFlushing => _isFlushing;
  bool get isAttestationBusy => _isAttestationBusy;

  bool? get hardwareSupported => _hardwareSupported;
  String? get hardwareCheckError => _hardwareCheckError;
  DeviceMetadata? get deviceMetadata => _deviceMetadata;
  String? get storedEnrollmentToken => _storedEnrollmentToken;
  Map<String, dynamic>? get decodedTokenClaims => _decodedTokenClaims;
  String? get lastEnrollResult => _lastEnrollResult;
  String? get lastProveResult => _lastProveResult;
  List<ConsoleLogItem> get consoleLogs => List.unmodifiable(_consoleLogs);

  set useHardwareAttestation(bool value) {
    if (_useHardwareAttestation != value) {
      _useHardwareAttestation = value;
      addConsoleLog(
        'HARDWARE',
        'Attestation mode changed to: ${value ? "Hardware (PlatformAttestationProvider)" : "No Attestation (NoAttestationProvider)"}',
      );
      notifyListeners();
      unawaited(checkHardwareSupportAndMetadata());
    }
  }

  AttestationProvider getAttestationProvider() {
    if (_useHardwareAttestation) {
      return const PlatformAttestationProvider();
    }
    return const NoAttestationProvider();
  }

  /// Inspects device metadata and hardware attestation support.
  Future<void> checkHardwareSupportAndMetadata() async {
    _isAttestationBusy = true;
    _hardwareCheckError = null;
    notifyListeners();

    final provider = getAttestationProvider();
    addConsoleLog(
      'HARDWARE',
      'Checking hardware attestation support with ${provider.runtimeType}...',
    );

    try {
      bool supported = false;
      try {
        supported = await provider.isSupported();
        addConsoleLog(
          'HARDWARE',
          '${provider.runtimeType}.isSupported() returned: $supported',
        );
      } on Object catch (e, s) {
        addConsoleLog(
          'ERROR',
          'provider.isSupported() threw exception: $e',
          error: e,
          stackTrace: s,
        );
      }

      String? token;
      try {
        token = await provider.loadEnrollmentToken();
        if (token != null && token.isNotEmpty) {
          addConsoleLog(
            'HARDWARE',
            'Found stored enrollment token (${token.length} chars).',
          );
        } else {
          addConsoleLog(
            'HARDWARE',
            'No stored enrollment token found.',
          );
        }
      } on Object catch (e, s) {
        addConsoleLog(
          'ERROR',
          'provider.loadEnrollmentToken() threw exception: $e',
          error: e,
          stackTrace: s,
        );
      }

      Map<String, dynamic>? claims;
      if (token != null && token.isNotEmpty) {
        try {
          claims = JwtHelper.tryDecodePayload(token);
          if (claims != null) {
            addConsoleLog(
              'HARDWARE',
              'Decoded token claims: ${claims.keys.join(", ")}',
            );
          }
        } on Object catch (e, s) {
          addConsoleLog(
            'ERROR',
            'Failed to decode enrollment token claims: $e',
            error: e,
            stackTrace: s,
          );
        }
      }

      try {
        final generator = FlutterDeviceDataGenerator();
        await generator.init();
        _deviceMetadata = generator.deviceMetadata;
        addConsoleLog(
          'INFO',
          'Device metadata collected: ${_deviceMetadata?.os} ${_deviceMetadata?.osVersion} (${_deviceMetadata?.model})',
        );
      } on Object catch (e, s) {
        addConsoleLog(
          'ERROR',
          'Failed to collect device metadata: $e',
          error: e,
          stackTrace: s,
        );
      }

      _hardwareSupported = supported;
      _storedEnrollmentToken = token;
      _decodedTokenClaims = claims;
      addConsoleLog(
        'HARDWARE',
        'Hardware check complete: status=${supported ? "SUPPORTED" : "UNSUPPORTED"}.',
      );
    } on Object catch (e, s) {
      _hardwareSupported = false;
      _hardwareCheckError = e.toString();
      addConsoleLog(
        'ERROR',
        'Unexpected error checking hardware: $e',
        error: e,
        stackTrace: s,
      );
    } finally {
      _isAttestationBusy = false;
      notifyListeners();
    }
  }

  /// Initializes the remote logger.
  Future<void> initRemoteLogger({
    required void Function(String message) onMessage,
  }) async {
    final gateway = gatewayUrlController.text.trim();
    final channel = channelIdController.text.trim();

    if (gateway.isEmpty || channel.isEmpty) {
      addConsoleLog(
        'WARNING',
        'Initialization aborted: Gateway URL and Channel ID cannot be empty.',
      );
      onMessage('Gateway URL and Channel ID cannot be empty');
      return;
    }

    _isInitializing = true;
    notifyListeners();

    addConsoleLog(
      'INFO',
      'Starting RemoteLogging initialization: Gateway=$gateway, Channel=$channel',
    );

    try {
      if (_remoteLogger != null) {
        addConsoleLog('INFO', 'Stopping active RemoteLogger instance...');
        await _remoteLogger!.stop(flush: false);
        _remoteLogger = null;
      }

      final config = RemoteLoggingConfig(
        gatewayUrl: gateway,
        channelId: channel,
        logSendLevel: Level.ALL,
      );

      final attestationProvider = getAttestationProvider();
      addConsoleLog(
        'INFO',
        'Configuring logger with provider: ${attestationProvider.runtimeType}',
      );

      final loggerInstance = await FlutterRemoteLogging.initRemote(
        remoteLoggingConfig: config,
        attestationProvider: attestationProvider,
        onLog: (record) {
          addConsoleLog(
            record.level.name,
            '[${record.loggerName}] ${record.message}',
            error: record.error,
            stackTrace: record.stackTrace,
          );
        },
        onError: (error, stackTrace) {
          addConsoleLog(
            'ERROR',
            'Logger internal error: $error',
            error: error,
            stackTrace: stackTrace,
          );
        },
      );

      addConsoleLog('INFO', 'Starting network invoker for $gateway...');
      final invoker = DioNetworkInvoker.fromBaseUrl(gateway);
      unawaited(
        loggerInstance.start(invoker).catchError((Object error, StackTrace st) {
          addConsoleLog(
            'ERROR',
            'Background logger service failed: $error',
            error: error,
            stackTrace: st,
          );
        }),
      );

      _remoteLogger = loggerInstance;
      addConsoleLog('SUCCESS', 'RemoteLogging started successfully!');
      onMessage('RemoteLogging started successfully!');
      addConsoleLog('INFO', 'Refreshing hardware support and tokens...');
      await checkHardwareSupportAndMetadata();
    } on Object catch (e, s) {
      addConsoleLog(
        'ERROR',
        'Initialization failed: $e',
        error: e,
        stackTrace: s,
      );
      onMessage('Failed to initialize: $e');
    } finally {
      _isInitializing = false;
      notifyListeners();
    }
  }

  /// Stops the remote logger service.
  Future<void> stopRemoteLogger() async {
    if (_remoteLogger == null) {
      addConsoleLog('WARNING', 'stopRemoteLogger called but no logger is running.');
      return;
    }
    addConsoleLog('INFO', 'Stopping remote logger service...');
    try {
      await _remoteLogger!.stop(flush: false);
      _remoteLogger = null;
      addConsoleLog('INFO', 'RemoteLogging stopped successfully.');
      notifyListeners();
    } on Object catch (e, s) {
      addConsoleLog('ERROR', 'Error stopping logger: $e', error: e, stackTrace: s);
    }
  }

  /// Flushes pending logs immediately to the gateway.
  Future<void> flushLogs({
    required void Function(String message) onMessage,
  }) async {
    if (_remoteLogger == null) {
      addConsoleLog('WARNING', 'Flush requested but logger is not initialized.');
      onMessage('Initialize logger first!');
      return;
    }

    _isFlushing = true;
    notifyListeners();

    addConsoleLog('INFO', 'Flushing pending logs to gateway...');

    try {
      await _remoteLogger!.flush();
      addConsoleLog('SUCCESS', 'Flush completed successfully.');
      onMessage('Logs flushed successfully!');
      await checkHardwareSupportAndMetadata();
    } on Object catch (e, s) {
      addConsoleLog('ERROR', 'Flush failed: $e', error: e, stackTrace: s);
      onMessage('Flush error: $e');
    } finally {
      _isFlushing = false;
      notifyListeners();
    }
  }

  /// Runs standalone hardware enrollment test.
  Future<void> testHardwareEnroll() async {
    _isAttestationBusy = true;
    notifyListeners();

    final provider = getAttestationProvider();
    if (!await provider.isSupported()) {
      _lastEnrollResult =
          'Hardware attestation is disabled or unsupported on this provider.';
      addConsoleLog(
        'WARNING',
        'Hardware enroll skipped: provider reports isSupported() == false.',
      );
      _isAttestationBusy = false;
      notifyListeners();
      return;
    }

    final dummyChallenge =
        'test-challenge-${DateTime.now().millisecondsSinceEpoch}';

    addConsoleLog(
      'HARDWARE',
      'Calling enroll() with challenge: $dummyChallenge...',
    );

    try {
      final material = await provider.enroll(dummyChallenge);
      final pubKeyBytes = base64Decode(material.publicKey);

      _lastEnrollResult =
          'Public Key (${pubKeyBytes.length} bytes):\n${material.publicKey}\n\n'
          'Attestation (${material.attestation.length} chars):\n'
          '${material.attestation.substring(0, material.attestation.length > 100 ? 100 : material.attestation.length)}...';

      addConsoleLog(
        'HARDWARE',
        'Enroll succeeded! Key size: ${pubKeyBytes.length}B',
      );
    } on Object catch (e, s) {
      _lastEnrollResult = 'Failed: $e';
      addConsoleLog(
        'ERROR',
        'Hardware enroll failed: $e',
        error: e,
        stackTrace: s,
      );
    } finally {
      _isAttestationBusy = false;
      notifyListeners();
    }
  }

  /// Runs standalone hardware challenge signing test.
  Future<void> testHardwareProve() async {
    _isAttestationBusy = true;
    notifyListeners();

    final provider = getAttestationProvider();
    if (!await provider.isSupported()) {
      _lastProveResult =
          'Hardware attestation is disabled or unsupported on this provider.';
      addConsoleLog(
        'WARNING',
        'Hardware prove skipped: provider reports isSupported() == false.',
      );
      _isAttestationBusy = false;
      notifyListeners();
      return;
    }

    final dummyChallenge =
        'proof-challenge-${DateTime.now().millisecondsSinceEpoch}';
    final sessionId = _deviceMetadata?.sessionId ?? 'dummy-session';
    final deviceId = _deviceMetadata?.deviceId ?? 'dummy-device';

    addConsoleLog(
      'HARDWARE',
      'Calling prove() with challenge: $dummyChallenge...',
    );

    try {
      final sig = await provider.prove(dummyChallenge, sessionId, deviceId);
      final sigBytes = base64Decode(sig);

      _lastProveResult = 'Signature (${sigBytes.length} bytes):\n$sig';
      addConsoleLog(
        'HARDWARE',
        'Prove succeeded! Sig size: ${sigBytes.length}B',
      );
    } on Object catch (e, s) {
      _lastProveResult = 'Failed: $e';
      addConsoleLog(
        'ERROR',
        'Hardware prove failed: $e',
        error: e,
        stackTrace: s,
      );
    } finally {
      _isAttestationBusy = false;
      notifyListeners();
    }
  }

  /// Wipes hardware key aliases and stored enrollment token.
  Future<void> resetHardware({
    required void Function(String message) onMessage,
  }) async {
    _isAttestationBusy = true;
    notifyListeners();

    addConsoleLog('HARDWARE', 'Resetting hardware key and stored enrollment token...');
    try {
      final provider = getAttestationProvider();
      await provider.reset();
      addConsoleLog('SUCCESS', 'Hardware key and enrollment token wiped successfully.');
      onMessage('Hardware state reset successfully.');
      _lastEnrollResult = null;
      _lastProveResult = null;
      await checkHardwareSupportAndMetadata();
    } on Object catch (e, s) {
      addConsoleLog('ERROR', 'Reset failed: $e', error: e, stackTrace: s);
      onMessage('Reset failed: $e');
    } finally {
      _isAttestationBusy = false;
      notifyListeners();
    }
  }

  /// Emits an application log entry.
  void emitLog(Level level, String msg, {Object? error, StackTrace? st}) {
    if (_remoteLogger == null) {
      addConsoleLog(
        'WARNING',
        'emitLog ignored: RemoteLogger is not initialized.',
      );
      return;
    }

    addConsoleLog(
      level.name,
      'Emit [${level.name}]: $msg',
      error: error,
      stackTrace: st,
    );

    if (level == Level.FINE) {
      _appLogger.fine(msg);
    } else if (level == Level.INFO) {
      _appLogger.info(msg);
    } else if (level == Level.WARNING) {
      _appLogger.warning(msg);
    } else if (level == Level.SEVERE) {
      _appLogger.severe(msg, error, st);
    } else if (level == Level.SHOUT) {
      _appLogger.shout(msg);
    }
  }

  /// Adds a log event to the live console.
  void addConsoleLog(
    String tag,
    String message, {
    Object? error,
    StackTrace? stackTrace,
  }) {
    // Print to IDE / terminal console (flutter run, logcat, Xcode)
    debugPrint('[$tag] $message${error != null ? " | Error: $error" : ""}');
    if (stackTrace != null) {
      debugPrint('[$tag] StackTrace: $stackTrace');
    }

    final item = ConsoleLogItem(
      timestamp: DateTime.now(),
      tag: tag,
      message: message,
      error: error?.toString(),
      stackTrace: stackTrace?.toString(),
    );

    _consoleLogs.add(item);
    if (_consoleLogs.length > 500) {
      _consoleLogs.removeAt(0);
    }
    notifyListeners();

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (logScrollController.hasClients) {
        logScrollController.animateTo(
          logScrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeOut,
        );
      }
    });
  }

  /// Clears live console history.
  void clearLogs() {
    _consoleLogs.clear();
    notifyListeners();
  }

  @override
  void dispose() {
    gatewayUrlController.dispose();
    channelIdController.dispose();
    customLogController.dispose();
    logScrollController.dispose();
    super.dispose();
  }
}
