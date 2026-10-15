import 'package:dart_network_layer_core/dart_network_layer_core.dart';

/// Response payload for GET /auth/challenge
class AuthChallengeResponse extends Schema {
  /// Creates an [AuthChallengeResponse].
  const AuthChallengeResponse({
    required this.challenge,
    required this.expiresIn,
  });

  /// Factory constructor to create [AuthChallengeResponse] from JSON.
  factory AuthChallengeResponse.fromJson(dynamic json) {
    if (json is! Map<String, dynamic>) {
      throw const FormatException('Expected Map<String, dynamic>');
    }
    return AuthChallengeResponse(
      challenge: json['challenge'] as String,
      expiresIn: (json['expires_in'] as num).toInt(),
    );
  }

  /// The cryptographic challenge string
  final String challenge;

  /// Challenge validity in seconds
  final int expiresIn;
}
