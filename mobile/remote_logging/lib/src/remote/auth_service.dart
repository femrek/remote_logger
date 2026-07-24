import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/model/auth_exchange_request.dart';
import 'package:remote_logging/src/remote/request/request_auth_exchange.dart';

/// A service responsible for handling authentication exchange.
class AuthService {
  /// Creates an [AuthService].
  AuthService({
    required this.config,
    required this.deviceMetadata,
    required this.invoker,
    this.getAttestationToken,
    this.onError,
  });

  /// Remote logging config
  final RemoteLoggingConfig config;

  /// Device metadata to include in the request
  final DeviceMetadata deviceMetadata;

  /// The network invoker to send the request
  final INetworkInvoker invoker;

  /// Optional callback to fetch an attestation token
  final GetAttestationTokenCallback? getAttestationToken;

  /// Error callback when an error occurs during the authentication exchange
  final OnError? onError;

  String? _accessToken;
  DateTime? _accessExpiresAt;

  /// Returns the current access token, or null if unauthenticated.
  String? get accessToken => _accessToken;

  /// Exchanges device attestation for JWT tokens.
  Future<void> exchangeAuthToken() async {
    final platform = deviceMetadata.os.toLowerCase() == 'ios'
        ? 'ios'
        : 'android';

    final token = getAttestationToken != null
        ? (await getAttestationToken!()) ?? ''
        : '';

    final request = AuthExchangeRequest(
      platform: platform,
      attestationToken: token,
      deviceUuid: deviceMetadata.deviceId,
      deviceMetadata: deviceMetadata,
      devicePublicKey: config.appleDevicePublicKey,
    );

    // Ensure no trailing slash on the gatewayUrl
    final base = config.gatewayUrl.endsWith('/')
        ? config.gatewayUrl.substring(0, config.gatewayUrl.length - 1)
        : config.gatewayUrl;

    final command = RequestAuthExchange(
      request: request,
      path: '$base/auth/exchange',
      channelId: config.channelId,
    );

    final response = await invoker.send(command);

    switch (response) {
      case SuccessResponseResult():
        _accessToken = response.data.accessToken;
        _accessExpiresAt = DateTime.now().add(
          Duration(seconds: response.data.accessExpiresIn),
        );
      case SpecifiedResponseResult():
        final err = Exception('Unexpected response type');
        onError?.call(err, StackTrace.current);
        throw err;
      case NetworkErrorResult():
        onError?.call(response.error, response.error.stackTrace);
        throw response.error;
    }
  }

  /// Checks if the access token is valid and not expired.
  bool get hasValidToken {
    if (_accessToken == null || _accessExpiresAt == null) return false;
    return DateTime.now().isBefore(_accessExpiresAt!);
  }

  /// Returns a valid access token, refreshing if necessary.
  Future<String?> getValidAccessToken() async {
    if (!hasValidToken) {
      await exchangeAuthToken();
    }
    return _accessToken;
  }
}
