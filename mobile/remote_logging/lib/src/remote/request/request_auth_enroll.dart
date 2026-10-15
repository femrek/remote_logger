import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/src/model/auth_enroll_request.dart';
import 'package:remote_logging/src/model/auth_enroll_response.dart';
import 'package:remote_logging/src/model/gateway_error.dart';

/// Request command for POST /auth/enroll
class RequestAuthEnroll extends RequestCommand<AuthEnrollResponse> {
  /// Creates a [RequestAuthEnroll].
  RequestAuthEnroll({
    required this.request,
    required this.path,
    required this.channelId,
  });

  /// The auth enroll request payload
  final AuthEnrollRequest request;

  /// The channel id
  final String channelId;

  @override
  final String path;

  @override
  final HttpRequestMethod method = HttpRequestMethod.post;

  @override
  late final RequestSchema payload = request;

  @override
  Map<String, dynamic> get headers => {
    'X-Channel-Id': channelId,
    'Content-Type': 'application/json',
  };

  @override
  SchemaFactory<GatewayError> get defaultErrorResponseFactory =>
      const GatewayErrorFactory();

  @override
  SchemaFactory<AuthEnrollResponse> get defaultResponseFactory =>
      const _AuthEnrollResponseFactory();
}

class _AuthEnrollResponseFactory extends JsonSchemaFactory<AuthEnrollResponse> {
  const _AuthEnrollResponseFactory();

  @override
  AuthEnrollResponse fromJson(dynamic json) =>
      AuthEnrollResponse.fromJson(json);
}
