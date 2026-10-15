import 'dart:async';

import 'package:dart_network_layer_core/dart_network_layer_core.dart';
import 'package:remote_logging/remote_logging.dart';
import 'package:remote_logging/src/model/auth_challenge_response.dart';
import 'package:remote_logging/src/model/auth_enroll_request.dart';
import 'package:remote_logging/src/model/auth_enroll_response.dart';
import 'package:remote_logging/src/model/auth_exchange_request.dart';
import 'package:remote_logging/src/remote/request/request_auth_challenge.dart';
import 'package:remote_logging/src/remote/request/request_auth_enroll.dart';
import 'package:remote_logging/src/remote/request/request_auth_exchange.dart';
import 'package:remote_logging/src/strategy/attestation_provider.dart';
import 'package:remote_logging/src/util/internal_logger.dart';

/// A service responsible for handling authentication exchange.
class AuthService {
  /// Creates an [AuthService].
  AuthService({
    required this.config,
    required this.deviceMetadata,
    required this.invoker,
    this.attestationProvider = const NoAttestationProvider(),
    this.getAttestationToken,
    this.onError,
  });

  /// Remote logging config
  final RemoteLoggingConfig config;

  /// Device metadata to include in the request
  final DeviceMetadata deviceMetadata;

  /// The network invoker to send the request
  final INetworkInvoker invoker;

  /// Hardware attestation provider
  final AttestationProvider attestationProvider;

  /// Optional legacy callback to fetch an attestation token
  final GetAttestationTokenCallback? getAttestationToken;

  /// Error callback when an error occurs during the authentication exchange
  final OnError? onError;

  String? _accessToken;
  DateTime? _accessExpiresAt;
  Future<void>? _ongoingExchange;

  /// Returns the current access token, or null if unauthenticated.
  String? get accessToken => _accessToken;

  /// Invalidates the currently cached access token, forcing a new exchange.
  void invalidate() {
    _accessToken = null;
    _accessExpiresAt = null;
  }

  /// Exchanges device attestation for JWT tokens with single-flight deduplication.
  Future<void> exchangeAuthToken() {
    if (_ongoingExchange != null) {
      return _ongoingExchange!;
    }
    return _ongoingExchange = _performExchange();
  }

  Future<void> _performExchange() async {
    try {
      final platform = deviceMetadata.os.toLowerCase() == 'ios'
          ? 'ios'
          : 'android';

      final gatewayUrl = config.gatewayUrlTrimmed;
      if (gatewayUrl.isEmpty) {
        internalLog.severe(
          'Gateway URL is not configured. Cannot exchange auth token.',
        );
        throw Exception('Gateway URL is not configured.');
      }

      final channelId = config.channelId;
      if (channelId.isEmpty) {
        internalLog.severe(
          'Channel ID is not configured. Cannot exchange auth token.',
        );
        throw Exception('Channel ID is not configured.');
      }

      final isHardwareAttestationSupported = await attestationProvider
          .isSupported();

      if (isHardwareAttestationSupported) {
        await _exchangeWithAttestation(gatewayUrl, channelId, platform);
      } else {
        await _exchangeDirect(gatewayUrl, channelId, platform);
      }
    } finally {
      _ongoingExchange = null;
    }
  }

  Future<void> _exchangeWithAttestation(
    String gatewayUrl,
    String channelId,
    String platform, {
    bool isRetry = false,
  }) async {
    // 1. Fetch challenge
    final challengeCommand = RequestAuthChallenge(
      path: '$gatewayUrl/auth/challenge',
      channelId: channelId,
    );
    final challengeRes = await invoker.send<AuthChallengeResponse>(
      challengeCommand,
    );
    final String challenge;
    switch (challengeRes) {
      case SuccessResponseResult<AuthChallengeResponse>():
        challenge = challengeRes.data.challenge;
      case SpecifiedResponseResult():
        final err = Exception('Failed to get auth challenge');
        onError?.call(err, StackTrace.current);
        throw err;
      case NetworkErrorResult():
        onError?.call(challengeRes.error, challengeRes.error.stackTrace);
        throw challengeRes.error;
    }

    // 2. Load or perform enrollment
    String? enrollmentToken = await attestationProvider.loadEnrollmentToken();
    if (enrollmentToken == null || enrollmentToken.isEmpty) {
      final material = await attestationProvider.enroll(challenge);
      final enrollReq = AuthEnrollRequest(
        platform: platform,
        deviceId: deviceMetadata.deviceId,
        publicKey: material.publicKey,
        attestation: material.attestation,
        challenge: challenge,
      );
      final enrollCommand = RequestAuthEnroll(
        request: enrollReq,
        path: '$gatewayUrl/auth/enroll',
        channelId: channelId,
      );
      final enrollRes = await invoker.send<AuthEnrollResponse>(enrollCommand);
      switch (enrollRes) {
        case SuccessResponseResult<AuthEnrollResponse>():
          final token = enrollRes.data.enrollmentToken;
          enrollmentToken = token;
          await attestationProvider.saveEnrollmentToken(token);
        case SpecifiedResponseResult():
          final err = Exception('Enrollment failed');
          onError?.call(err, StackTrace.current);
          throw err;
        case NetworkErrorResult():
          onError?.call(enrollRes.error, enrollRes.error.stackTrace);
          throw enrollRes.error;
      }
    }

    // 3. Generate session proof (signed challenge)
    final signature = await attestationProvider.prove(
      challenge,
      deviceMetadata.sessionId,
      deviceMetadata.deviceId,
    );

    // 4. Send exchange request
    final exchangeReq = AuthExchangeRequest(
      platform: platform,
      attestationToken: '',
      deviceId: deviceMetadata.deviceId,
      deviceMetadata: deviceMetadata,
      enrollmentToken: enrollmentToken,
      challenge: challenge,
      challengeSignature: signature,
    );

    final command = RequestAuthExchange(
      request: exchangeReq,
      path: '$gatewayUrl/auth/exchange',
      channelId: channelId,
    );

    final response = await invoker.send(command);
    switch (response) {
      case SuccessResponseResult():
        _accessToken = response.data.accessToken;
        _accessExpiresAt = DateTime.now().add(
          Duration(seconds: response.data.accessExpiresIn),
        );
      case SpecifiedResponseResult():
        if (!isRetry) {
          internalLog.warning(
            'Auth exchange failed with enrollment token; resetting key & retrying enrollment once.',
          );
          await attestationProvider.reset();
          return _exchangeWithAttestation(
            gatewayUrl,
            channelId,
            platform,
            isRetry: true,
          );
        }
        final err = Exception('Exchange with attestation failed');
        onError?.call(err, StackTrace.current);
        throw err;
      case NetworkErrorResult():
        onError?.call(response.error, response.error.stackTrace);
        throw response.error;
    }
  }

  Future<void> _exchangeDirect(
    String gatewayUrl,
    String channelId,
    String platform,
  ) async {
    final token = getAttestationToken != null
        ? (await getAttestationToken!()) ?? ''
        : '';

    final request = AuthExchangeRequest(
      platform: platform,
      attestationToken: token,
      deviceId: deviceMetadata.deviceId,
      deviceMetadata: deviceMetadata,
      devicePublicKey: config.appleDevicePublicKey,
    );

    final command = RequestAuthExchange(
      request: request,
      path: '$gatewayUrl/auth/exchange',
      channelId: channelId,
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
