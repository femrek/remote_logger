import Flutter
import UIKit
import DeviceCheck
import CryptoKit
import Security

public class AppAttestPlugin: NSObject, FlutterPlugin {
  private static let channelName = "dev.femrek.flutter_remote_logging/attestation"
  private static let keyIdTag = "dev.femrek.flutter_remote_logging.attest_key_id"
  private static let tokenTag = "dev.femrek.flutter_remote_logging.enrollment_token"

  public static func register(with registrar: FlutterPluginRegistrar) {
    let channel = FlutterMethodChannel(name: channelName, binaryMessenger: registrar.messenger())
    let instance = AppAttestPlugin()
    registrar.addMethodCallDelegate(instance, channel: channel)
  }

  public func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "isSupported":
      if #available(iOS 14.0, *) {
        result(DCAppAttestService.shared.isSupported)
      } else {
        result(false)
      }

    case "enroll":
      guard #available(iOS 14.0, *), DCAppAttestService.shared.isSupported else {
        result(FlutterError(code: "UNSUPPORTED", message: "DCAppAttestService is not supported", details: nil))
        return
      }

      guard let args = call.arguments as? [String: Any],
            let challenge = args["challenge"] as? String,
            let challengeData = challenge.data(using: .utf8) else {
        result(FlutterError(code: "INVALID_ARGUMENTS", message: "challenge is required", details: nil))
        return
      }

      DCAppAttestService.shared.generateKey { [weak self] keyId, error in
        if let error = error {
          result(FlutterError(code: "KEY_GEN_FAILED", message: error.localizedDescription, details: nil))
          return
        }
        guard let keyId = keyId else {
          result(FlutterError(code: "KEY_GEN_FAILED", message: "Failed to generate keyId", details: nil))
          return
        }

        self?.saveToKeychain(key: AppAttestPlugin.keyIdTag, data: keyId.data(using: .utf8)!)

        let clientDataHash = Data(SHA256.hash(data: challengeData))
        DCAppAttestService.shared.attestKey(keyId, clientDataHash: clientDataHash) { attestation, attestError in
          if let attestError = attestError {
            result(FlutterError(code: "ATTEST_FAILED", message: attestError.localizedDescription, details: nil))
            return
          }
          guard let attestation = attestation else {
            result(FlutterError(code: "ATTEST_FAILED", message: "Attestation blob is nil", details: nil))
            return
          }

          result([
            "publicKey": keyId,
            "attestation": attestation.base64EncodedString()
          ])
        }
      }

    case "prove":
      guard #available(iOS 14.0, *), DCAppAttestService.shared.isSupported else {
        result(FlutterError(code: "UNSUPPORTED", message: "DCAppAttestService is not supported", details: nil))
        return
      }

      guard let args = call.arguments as? [String: Any],
            let challenge = args["challenge"] as? String,
            let challengeData = challenge.data(using: .utf8) else {
        result(FlutterError(code: "INVALID_ARGUMENTS", message: "challenge is required", details: nil))
        return
      }

      guard let keyIdData = loadFromKeychain(key: AppAttestPlugin.keyIdTag),
            let keyId = String(data: keyIdData, encoding: .utf8) else {
        result(FlutterError(code: "KEY_NOT_FOUND", message: "Attestation keyId not found in Keychain", details: nil))
        return
      }

      let clientDataHash = Data(SHA256.hash(data: challengeData))
      DCAppAttestService.shared.generateAssertion(keyId, clientDataHash: clientDataHash) { assertion, error in
        if let error = error {
          result(FlutterError(code: "ASSERTION_FAILED", message: error.localizedDescription, details: nil))
          return
        }
        guard let assertion = assertion else {
          result(FlutterError(code: "ASSERTION_FAILED", message: "Assertion data is nil", details: nil))
          return
        }

        result(assertion.base64EncodedString())
      }

    case "loadEnrollmentToken":
      if let data = loadFromKeychain(key: AppAttestPlugin.tokenTag),
         let token = String(data: data, encoding: .utf8) {
        result(token)
      } else {
        result(nil)
      }

    case "saveEnrollmentToken":
      guard let args = call.arguments as? [String: Any],
            let token = args["token"] as? String,
            let tokenData = token.data(using: .utf8) else {
        result(FlutterError(code: "INVALID_ARGUMENTS", message: "token is required", details: nil))
        return
      }
      saveToKeychain(key: AppAttestPlugin.tokenTag, data: tokenData)
      result(nil)

    case "reset":
      deleteFromKeychain(key: AppAttestPlugin.keyIdTag)
      deleteFromKeychain(key: AppAttestPlugin.tokenTag)
      result(nil)

    default:
      result(FlutterMethodNotImplemented)
    }
  }

  // MARK: - Keychain Helpers

  private func saveToKeychain(key: String, data: Data) {
    deleteFromKeychain(key: key)
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrAccount as String: key,
      kSecValueData as String: data,
      kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    ]
    SecItemAdd(query as CFDictionary, nil)
  }

  private func loadFromKeychain(key: String) -> Data? {
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrAccount as String: key,
      kSecReturnData as String: kCFBooleanTrue as Any,
      kSecMatchLimit as String: kSecMatchLimitOne
    ]
    var dataTypeRef: AnyObject?
    let status = SecItemCopyMatching(query as CFDictionary, &dataTypeRef)
    if status == errSecSuccess {
      return dataTypeRef as? Data
    }
    return nil
  }

  private func deleteFromKeychain(key: String) {
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrAccount as String: key
    ]
    SecItemDelete(query as CFDictionary)
  }
}
