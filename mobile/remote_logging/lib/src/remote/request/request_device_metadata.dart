import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';

/// Request model to send device metadata to the server
class RequestDeviceMetadata extends RequestCommand<IgnoredSchema> {
  /// Creates a [RequestDeviceMetadata] with the given [metadata].
  RequestDeviceMetadata({
    required this.metadata,
    required this.path,
  });

  /// The device metadata to be sent.
  final DeviceMetadata metadata;

  @override
  final String path;

  @override
  final HttpRequestMethod method = HttpRequestMethod.post;

  @override
  late final DeviceMetadata payload = metadata;

  @override
  SchemaFactory<Schema> get defaultErrorResponseFactory =>
      IgnoredSchema.factory;

  @override
  SchemaFactory<IgnoredSchema> get defaultResponseFactory =>
      IgnoredSchema.factory;
}
