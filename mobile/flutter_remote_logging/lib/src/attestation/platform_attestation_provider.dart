import 'package:flutter/services.dart';
import 'package:remote_logging/remote_logging.dart';

/// Real platform hardware attestation provider backed by Android Keystore
/// Key Attestation and Apple DeviceCheck App Attest.
class PlatformAttestationProvider implements AttestationProvider {
  /// Creates a [PlatformAttestationProvider].
  const PlatformAttestationProvider({
    MethodChannel? channel,
  }) : _channel =
           channel ??
           const MethodChannel('dev.femrek.flutter_remote_logging/attestation');

  final MethodChannel _channel;

  @override
  Future<bool> isSupported() async {
    try {
      final supported = await _channel.invokeMethod<bool>('isSupported');
      return supported ?? false;
    } on Object {
      return false;
    }
  }

  @override
  Future<EnrollmentMaterial> enroll(String challenge) async {
    final result = await _channel.invokeMapMethod<String, dynamic>(
      'enroll',
      {'challenge': challenge},
    );

    if (result == null ||
        !result.containsKey('publicKey') ||
        !result.containsKey('attestation')) {
      throw PlatformException(
        code: 'ENROLL_FAILED',
        message: 'Invalid response from native enrollment',
      );
    }

    return EnrollmentMaterial(
      publicKey: result['publicKey'] as String,
      attestation: result['attestation'] as String,
    );
  }

  @override
  Future<String> prove(
    String challenge,
    String sessionId,
    String deviceId,
  ) async {
    final signature = await _channel.invokeMethod<String>(
      'prove',
      {
        'challenge': challenge,
        'sessionId': sessionId,
        'deviceId': deviceId,
      },
    );

    if (signature == null || signature.isEmpty) {
      throw PlatformException(
        code: 'PROVE_FAILED',
        message: 'Empty proof signature returned from platform',
      );
    }

    return signature;
  }

  @override
  Future<String?> loadEnrollmentToken() async {
    return _channel.invokeMethod<String>('loadEnrollmentToken');
  }

  @override
  Future<void> saveEnrollmentToken(String token) async {
    await _channel.invokeMethod<void>(
      'saveEnrollmentToken',
      {'token': token},
    );
  }

  @override
  Future<void> reset() async {
    await _channel.invokeMethod<void>('reset');
  }
}
