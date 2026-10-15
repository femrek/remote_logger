import 'package:dart_network_layer_core/dart_network_layer_core.dart';

/// Response payload for POST /auth/enroll
class AuthEnrollResponse extends Schema {
  /// Creates an [AuthEnrollResponse].
  const AuthEnrollResponse({
    required this.enrollmentToken,
    required this.expiresIn,
  });

  /// Factory constructor to create [AuthEnrollResponse] from JSON.
  factory AuthEnrollResponse.fromJson(dynamic json) {
    if (json is! Map<String, dynamic>) {
      throw const FormatException('Expected Map<String, dynamic>');
    }
    return AuthEnrollResponse(
      enrollmentToken: json['enrollment_token'] as String,
      expiresIn: (json['expires_in'] as num).toInt(),
    );
  }

  /// 90-day server-signed enrollment token
  final String enrollmentToken;

  /// Enrollment token validity in seconds
  final int expiresIn;
}
