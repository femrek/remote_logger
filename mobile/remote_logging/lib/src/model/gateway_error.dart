import 'dart:convert';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';

/// Error response from the gateway
class GatewayError extends Schema implements Exception {
  /// Constructs a [GatewayError].
  const GatewayError({
    required this.error,
    required this.code,
  });

  /// Factory constructor to parse from JSON
  factory GatewayError.fromJson(Map<String, dynamic> json) {
    if (!json.containsKey('error') || !json.containsKey('code')) {
      throw const FormatException('Missing required fields for GatewayError');
    }

    return GatewayError(
      error: json['error'] as String,
      code: json['code'] as String,
    );
  }

  /// A descriptive error message
  final String error;

  /// A machine-readable error code
  final String code;

  @override
  String toString() => 'GatewayError(code: $code, error: $error)';
}

class _GatewayErrorInvalidBody extends GatewayError {
  _GatewayErrorInvalidBody({required this.rawData})
    : super(
        error: '(Invalid request body)',
        code: '(invalid_body from server)',
      );

  final String rawData;

  @override
  String toString() =>
      'GatewayError(code: $code, error: $error, rawData: $rawData)';
}

/// The factory class to handle error responses from backend.
class GatewayErrorFactory extends DynamicSchemaFactory<GatewayError> {
  /// Constructs a [GatewayErrorFactory].
  const GatewayErrorFactory();

  @override
  GatewayError from(dynamic response) {
    try {
      if (response is Map<String, dynamic>) {
        return GatewayError.fromJson(response);
      }

      if (response is String) {
        return GatewayError.fromJson(
          jsonDecode(response) as Map<String, dynamic>,
        );
      }

      else {
        throw FormatException(
          'Response is not a valid JSON object: ${response.runtimeType}',
        );
      }
    } on Object catch (e, s) {
      if (e is FormatException || e is TypeError) {
        localLog.severe(
          'Failed to parse GatewayError from response: $response',
          e,
          s,
        );
      }
      return _GatewayErrorInvalidBody(rawData: response.toString());
    }
  }
}
