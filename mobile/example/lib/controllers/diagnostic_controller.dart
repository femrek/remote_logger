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
  DeviceMetadata? get deviceMetadata => _deviceMetadata;
  String? get storedEnrollmentToken => _storedEnrollmentToken;
  Map<String, dynamic>? get decodedTokenClaims => _decodedTokenClaims;
  String? get lastEnrollResult => _lastEnrollResult;
  String? get lastProveResult => _lastProveResult;
  List<ConsoleLogItem> get consoleLogs => List.unmodifiable(_consoleLogs);

  set useHardwareAttestation(bool value) {
    if (_useHardwareAttestation != value) {
      _useHardwareAttestation = value;
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
    notifyListeners();

    try {
      final provider = getAttestationProvider();
      final supported = await provider.isSupported();
      final token = await provider.loadEnrollmentToken();

      Map<String, dynamic>? claims;
      if (token != null && token.isNotEmpty) {
        claims = JwtHelper.tryDecodePayload(token);
      }

      final generator = FlutterDeviceDataGenerator();
      await generator.init();

      _hardwareSupported = supported;
      _storedEnrollmentToken = token;
      _decodedTokenClaims = claims;
      _deviceMetadata = generator.deviceMetadata;
    } on Object catch (e) {
      addConsoleLog('ERROR', 'Error checking hardware: $e');
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
      onMessage('Gateway URL and Channel ID cannot be empty');
      return;
    }

    _isInitializing = true;
    notifyListeners();

    addConsoleLog(
      'INFO',
      'Initializing RemoteLogging for $gateway [$channel]...',
    );

    try {
      if (_remoteLogger != null) {
        await _remoteLogger!.stop(flush: false);
        _remoteLogger = null;
      }

      final config = RemoteLoggingConfig(
        gatewayUrl: gateway,
        channelId: channel,
        logSendLevel: Level.ALL,
      );

      final attestationProvider = getAttestationProvider();

      final loggerInstance = await FlutterRemoteLogging.initRemote(
        remoteLoggingConfig: config,
        attestationProvider: attestationProvider,
        onLog: (record) {
          addConsoleLog(
            record.level.name,
            '[${record.loggerName}] ${record.message}',
            error: record.error,
          );
        },
        onError: (error, stackTrace) {
          addConsoleLog(
            'ERROR',
            'Logger error: $error',
            error: error,
            stackTrace: stackTrace,
          );
        },
      );

      final invoker = DioNetworkInvoker.fromBaseUrl(gateway);
      unawaited(loggerInstance.start(invoker));

      _remoteLogger = loggerInstance;
      addConsoleLog('SUCCESS', 'RemoteLogging started successfully!');
      onMessage('RemoteLogging started successfully!');
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
    if (_remoteLogger == null) return;
    try {
      await _remoteLogger!.stop(flush: false);
      _remoteLogger = null;
      addConsoleLog('INFO', 'RemoteLogging stopped.');
      notifyListeners();
    } on Object catch (e) {
      addConsoleLog('ERROR', 'Error stopping logger: $e');
    }
  }

  /// Flushes pending logs immediately to the gateway.
  Future<void> flushLogs({
    required void Function(String message) onMessage,
  }) async {
    if (_remoteLogger == null) {
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

    try {
      final provider = getAttestationProvider();
      await provider.reset();
      addConsoleLog('HARDWARE', 'Hardware key and enrollment token wiped.');
      onMessage('Hardware state reset successfully.');
      _lastEnrollResult = null;
      _lastProveResult = null;
      await checkHardwareSupportAndMetadata();
    } on Object catch (e) {
      addConsoleLog('ERROR', 'Reset failed: $e');
      onMessage('Reset failed: $e');
    } finally {
      _isAttestationBusy = false;
      notifyListeners();
    }
  }

  /// Emits an application log entry.
  void emitLog(Level level, String msg, {Object? error, StackTrace? st}) {
    if (_remoteLogger == null) return;

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
