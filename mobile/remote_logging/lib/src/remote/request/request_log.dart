import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';

/// Request model to send log data to the server
class RequestLog extends RequestCommand<IgnoredSchema> {
  /// Creates a [RequestLog] with the given [logData].
  RequestLog({
    required this.logData,
    required this.path,
  });

  /// The log data to be sent.
  final ServerLog logData;

  @override
  final String path;

  @override
  late final ServerLog payload = logData;

  @override
  final HttpRequestMethod method = HttpRequestMethod.post;

  @override
  SchemaFactory<Schema> get defaultErrorResponseFactory =>
      IgnoredSchema.factory;

  @override
  SchemaFactory<IgnoredSchema> get defaultResponseFactory =>
      IgnoredSchema.factory;
}
