/// Data required by POST /auth/enroll
class EnrollmentMaterial {
  /// Data required by POST /auth/enroll
  const EnrollmentMaterial({
    required this.publicKey,
    required this.attestation,
  });

  /// Base64-encoded client public key (SEC1 or SPKI).
  final String publicKey;

  /// Base64-encoded attestation statement (Android cert chain or iOS CBOR
  /// receipt).
  final String attestation;
}

/// Abstract interface for hardware attestation providers.
abstract interface class AttestationProvider {
  /// Whether hardware attestation is supported on this platform/device.
  Future<bool> isSupported();

  /// Creates and attests a new hardware key against the provided server
  /// challenge.
  Future<EnrollmentMaterial> enroll(String challenge);

  /// Signs the server challenge with the existing hardware key to prove
  /// possession.
  Future<String> prove(String challenge, String sessionId, String deviceId);

  /// Loads the persisted enrollment token from secure storage.
  Future<String?> loadEnrollmentToken();

  /// Saves the enrollment token to secure storage.
  Future<void> saveEnrollmentToken(String token);

  /// Deletes the hardware key and clears any persisted enrollment token.
  Future<void> reset();
}

/// Default no-op provider for platforms without hardware attestation support.
class NoAttestationProvider implements AttestationProvider {
  /// Default no-op provider for platforms without hardware attestation support.
  const NoAttestationProvider();

  @override
  Future<bool> isSupported() async => false;

  @override
  Future<EnrollmentMaterial> enroll(String challenge) async =>
      const EnrollmentMaterial(publicKey: '', attestation: '');

  @override
  Future<String> prove(
    String challenge,
    String sessionId,
    String deviceId,
  ) async => '';

  @override
  Future<String?> loadEnrollmentToken() async => null;

  @override
  Future<void> saveEnrollmentToken(String token) async {}

  @override
  Future<void> reset() async {}
}
