import 'package:remote_logging/src/model/auth_exchange_response.dart';
import 'package:test/test.dart';

void main() {
  group('AuthExchangeResponse', () {
    test('fromJson parses all 4 required fields', () {
      final json = {
        'access_token': 'access',
        'refresh_token': 'refresh',
        'access_expires_in': 3600,
        'refresh_expires_in': 7200,
      };
      final response = AuthExchangeResponse.fromJson(json);
      expect(response.accessToken, 'access');
      expect(response.refreshToken, 'refresh');
      expect(response.accessExpiresIn, 3600);
      expect(response.refreshExpiresIn, 7200);
    });

    test('access_expires_in is parsed as int', () {
      final json = {
        'access_token': 'access',
        'refresh_token': 'refresh',
        'access_expires_in': 3600,
        'refresh_expires_in': 7200,
      };
      final response = AuthExchangeResponse.fromJson(json);
      expect(response.accessExpiresIn, isA<int>());
    });

    test('refresh_expires_in is parsed as int', () {
      final json = {
        'access_token': 'access',
        'refresh_token': 'refresh',
        'access_expires_in': 3600,
        'refresh_expires_in': 7200,
      };
      final response = AuthExchangeResponse.fromJson(json);
      expect(response.refreshExpiresIn, isA<int>());
    });

    test('fromJson throws on missing required field', () {
      final json = {
        'refresh_token': 'refresh',
        'access_expires_in': 3600,
        'refresh_expires_in': 7200,
      };
      expect(
        () => AuthExchangeResponse.fromJson(json),
        throwsA(isA<FormatException>()),
      );
    });

    test('fromJson handles extra unknown fields gracefully', () {
      final json = {
        'access_token': 'access',
        'refresh_token': 'refresh',
        'access_expires_in': 3600,
        'refresh_expires_in': 7200,
        'extra_field': 'value',
      };
      final response = AuthExchangeResponse.fromJson(json);
      expect(response.accessToken, 'access');
    });
  });
}
