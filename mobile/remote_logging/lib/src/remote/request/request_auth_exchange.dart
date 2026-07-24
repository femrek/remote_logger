import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/src/model/auth_exchange_request.dart';
import 'package:remote_logging/src/model/auth_exchange_response.dart';
import 'package:remote_logging/src/model/gateway_error.dart';

/// Request model to exchange authentication token
class RequestAuthExchange extends RequestCommand<AuthExchangeResponse> {
  /// Creates a [RequestAuthExchange].
  RequestAuthExchange({
    required this.request,
    required this.path,
    required this.channelId,
  });

  /// The auth exchange request payload
  final AuthExchangeRequest request;

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
  SchemaFactory<AuthExchangeResponse> get defaultResponseFactory =>
      const _AuthExchangeResponseFactory();
}

class _AuthExchangeResponseFactory
    extends JsonSchemaFactory<AuthExchangeResponse> {
  const _AuthExchangeResponseFactory();

  @override
  AuthExchangeResponse fromJson(dynamic json) =>
      AuthExchangeResponse.fromJson(json as Map<String, dynamic>);
}
