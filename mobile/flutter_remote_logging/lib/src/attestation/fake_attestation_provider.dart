import 'package:remote_logging/remote_logging.dart';

/// Fake attestation provider for testing, desktop, simulator, and CI
/// environments.
class FakeAttestationProvider implements AttestationProvider {
  /// Creates a [FakeAttestationProvider].
  FakeAttestationProvider({
    this.supported = true,
    this.initialToken,
  }) : _token = initialToken;

  /// Whether this provider reports itself as supported.
  bool supported;

  /// Initial or current enrollment token.
  final String? initialToken;

  String? _token;

  /// The number of times [enroll] has been invoked.
  int enrollCallCount = 0;

  /// The number of times [prove] has been invoked.
  int proveCallCount = 0;

  /// The number of times [reset] has been invoked.
  int resetCallCount = 0;

  @override
  Future<bool> isSupported() async => supported;

  @override
  Future<EnrollmentMaterial> enroll(String challenge) async {
    enrollCallCount++;
    return const EnrollmentMaterial(
      publicKey: 'fake-sec1-p256-public-key-base64',
      attestation: 'fake-hardware-attestation-blob-base64',
    );
  }

  @override
  Future<String> prove(
    String challenge,
    String sessionId,
    String deviceId,
  ) async {
    proveCallCount++;
    return 'fake-p256-signature-over-challenge-base64';
  }

  @override
  Future<String?> loadEnrollmentToken() async => _token;

  @override
  Future<void> saveEnrollmentToken(String token) async {
    _token = token;
  }

  @override
  Future<void> reset() async {
    resetCallCount++;
    _token = null;
  }
}
