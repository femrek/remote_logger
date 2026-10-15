import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/src/model/device_metadata.dart';

/// Request payload for authentication exchange
class AuthExchangeRequest extends JsonRequestSchema {
  /// Constructs an [AuthExchangeRequest].
  const AuthExchangeRequest({
    required this.platform,
    required this.attestationToken,
    required this.deviceId,
    required this.deviceMetadata,
    this.devicePublicKey,
    this.enrollmentToken,
    this.challenge,
    this.challengeSignature,
  }) : assert(
         platform == 'android' || platform == 'ios',
         'Platform must be android or ios',
       );

  /// The platform of the device (android or ios)
  final String platform;

  /// Attestation token provided by the device
  final String attestationToken;

  /// Unique device identifier
  final String deviceId;

  /// Metadata about the device
  final DeviceMetadata deviceMetadata;

  /// Public key of the device, required for iOS
  final String? devicePublicKey;

  /// 90-day server-signed enrollment token
  final String? enrollmentToken;

  /// Stateless challenge string from GET /auth/challenge
  final String? challenge;

  /// Hardware key signature over challenge
  final String? challengeSignature;

  @override
  Map<String, dynamic> toJsonPayload() => {
    'platform': platform,
    'attestation_token': attestationToken,
    'device_id': deviceId,
    'device_metadata': deviceMetadata.toJsonPayload(),
    if (devicePublicKey != null) 'device_public_key': devicePublicKey,
    if (enrollmentToken != null) 'enrollment_token': enrollmentToken,
    if (challenge != null) 'challenge': challenge,
    if (challengeSignature != null) 'challenge_signature': challengeSignature,
  };
}
