import 'package:remote_logging/src/model/gateway_error.dart';
import 'package:test/test.dart';

void main() {
  group('GatewayError', () {
    test('fromJson parses error and code', () {
      final json = {
        'error': 'message',
        'code': 'BAD_REQUEST',
      };
      final error = GatewayError.fromJson(json);
      expect(error.error, 'message');
      expect(error.code, 'BAD_REQUEST');
    });

    test('code is a known enum value', () {
      final codes = [
        'BAD_REQUEST',
        'UNAUTHORIZED',
        'ATTESTATION_FAILED',
        'NOT_FOUND',
        'CHANNEL_NOT_FOUND',
        'MISSING_AUTH',
        'JWT_INVALID',
        'JWT_CHANNEL_MISMATCH',
        'JWT_APP_MISMATCH',
        'VALIDATION_ERROR',
        'INTERNAL_ERROR',
        'SERVICE_UNAVAILABLE',
      ];
      for (final code in codes) {
        final json = {
          'error': 'message',
          'code': code,
        };
        final error = GatewayError.fromJson(json);
        expect(error.code, code);
      }
    });

    test('fromJson handles unknown code gracefully', () {
      final json = {
        'error': 'message',
        'code': 'UNKNOWN_CODE_STRING',
      };
      final error = GatewayError.fromJson(json);
      expect(error.code, 'UNKNOWN_CODE_STRING');
    });
  });
}
