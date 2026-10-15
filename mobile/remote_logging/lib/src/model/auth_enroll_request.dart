import 'package:dart_network_layer_core/dart_network_layer_core.dart';

/// Request payload for POST /auth/enroll
class AuthEnrollRequest extends JsonRequestSchema {
  /// Creates an [AuthEnrollRequest].
  const AuthEnrollRequest({
    required this.platform,
    required this.deviceId,
    required this.publicKey,
    required this.attestation,
    required this.challenge,
  });

  /// The platform (android or ios)
  final String platform;

  /// The device UUID
  final String deviceId;

  /// Base64 public key
  final String publicKey;

  /// Base64 attestation statement
  final String attestation;

  /// Challenge string from GET /auth/challenge
  final String challenge;

  @override
  Map<String, dynamic> toJsonPayload() => {
    'platform': platform,
    'device_id': deviceId,
    'public_key': publicKey,
    'attestation': attestation,
    'challenge': challenge,
  };
}
