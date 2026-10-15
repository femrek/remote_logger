import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/src/model/auth_challenge_response.dart';
import 'package:remote_logging/src/model/gateway_error.dart';

/// Request command for GET /auth/challenge
class RequestAuthChallenge extends RequestCommand<AuthChallengeResponse> {
  /// Creates a [RequestAuthChallenge].
  RequestAuthChallenge({
    required this.path,
    required this.channelId,
  });

  @override
  final String path;

  /// The channel id
  final String channelId;

  @override
  final HttpRequestMethod method = HttpRequestMethod.get;

  @override
  RequestSchema get payload => const EmptyRequestSchema();

  @override
  Map<String, dynamic> get headers => {
    'X-Channel-Id': channelId,
  };

  @override
  SchemaFactory<GatewayError> get defaultErrorResponseFactory =>
      const GatewayErrorFactory();

  @override
  SchemaFactory<AuthChallengeResponse> get defaultResponseFactory =>
      const _AuthChallengeResponseFactory();
}

class _AuthChallengeResponseFactory
    extends JsonSchemaFactory<AuthChallengeResponse> {
  const _AuthChallengeResponseFactory();

  @override
  AuthChallengeResponse fromJson(dynamic json) =>
      AuthChallengeResponse.fromJson(json);
}
