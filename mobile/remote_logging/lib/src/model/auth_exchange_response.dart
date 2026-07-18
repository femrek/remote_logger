import 'package:dart_network_layer_core/dart_network_layer_core.dart';

/// Response payload from authentication exchange
class AuthExchangeResponse extends Schema {
  /// Constructs an [AuthExchangeResponse].
  const AuthExchangeResponse({
    required this.accessToken,
    required this.refreshToken,
    required this.accessExpiresIn,
    required this.refreshExpiresIn,
  });

  /// Factory constructor to parse from JSON
  factory AuthExchangeResponse.fromJson(Map<String, dynamic> json) {
    if (!json.containsKey('access_token') ||
        !json.containsKey('refresh_token') ||
        !json.containsKey('access_expires_in') ||
        !json.containsKey('refresh_expires_in')) {
      throw const FormatException(
        'Missing required fields for AuthExchangeResponse',
      );
    }

    return AuthExchangeResponse(
      accessToken: json['access_token'] as String,
      refreshToken: json['refresh_token'] as String,
      accessExpiresIn: json['access_expires_in'] as int,
      refreshExpiresIn: json['refresh_expires_in'] as int,
    );
  }

  /// JWT access token
  final String accessToken;

  /// JWT refresh token
  final String refreshToken;

  /// Number of seconds until the access token expires
  final int accessExpiresIn;

  /// Number of seconds until the refresh token expires
  final int refreshExpiresIn;
}
