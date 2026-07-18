import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';

/// Request model to send a batch of log data to the server
class RequestLogBatch extends RequestCommand<IgnoredSchema> {
  /// Creates a [RequestLogBatch] with the given list of [logs].
  RequestLogBatch({
    required this.logs,
    required this.path,
    this.token,
  });

  /// the data to be sent.
  final List<ServerLog> logs;

  /// Optional JWT access token
  final String? token;

  @override
  final String path;

  @override
  HttpRequestMethod get method => HttpRequestMethod.post;

  @override
  late final RequestSchema payload = _RequestLogBatchPayload(logs: logs);

  @override
  Map<String, dynamic> get headers => {
    if (token != null) 'Authorization': 'Bearer $token',
  };

  @override
  SchemaFactory<Schema> get defaultErrorResponseFactory =>
      IgnoredSchema.factory;

  @override
  SchemaFactory<IgnoredSchema> get defaultResponseFactory =>
      IgnoredSchema.factory;
}

class _RequestLogBatchPayload extends JsonRequestSchema {
  _RequestLogBatchPayload({
    required this.logs,
  });

  final List<ServerLog> logs;

  @override
  List<Map<String, dynamic>> toJsonPayload() {
    return logs.map((e) => e.toJsonPayload()).toList();
  }
}
