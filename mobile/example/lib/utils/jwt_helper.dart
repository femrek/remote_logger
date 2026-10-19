import 'dart:convert';

/// Helper utility to decode unverified JWT claims for diagnostics and UI display.
abstract final class JwtHelper {
  /// Decodes and returns the payload map of a JWT token, or `null` if invalid.
  static Map<String, dynamic>? tryDecodePayload(String token) {
    try {
      final parts = token.split('.');
      if (parts.length < 2) return null;
      final normalized = base64Url.normalize(parts[1]);
      final payload = utf8.decode(base64Url.decode(normalized));
      return jsonDecode(payload) as Map<String, dynamic>;
    } on Object {
      return null;
    }
  }
}
