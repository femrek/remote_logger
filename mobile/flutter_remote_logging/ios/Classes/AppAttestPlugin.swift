import Flutter
import UIKit
import DeviceCheck
import CryptoKit
import Security

public class AppAttestPlugin: NSObject, FlutterPlugin {
  private static let channelName = "dev.femrek.flutter_remote_logging/attestation"
  private static let seKeyTag = "dev.femrek.flutter_remote_logging.se_key_data"
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
        result(SecureEnclave.isAvailable || DCAppAttestService.shared.isSupported)
      } else {
        result(false)
      }

    case "enroll":
      guard #available(iOS 14.0, *) else {
        result(FlutterError(code: "UNSUPPORTED", message: "iOS 14.0 or higher is required", details: nil))
        return
      }

      guard let args = call.arguments as? [String: Any],
            let challenge = args["challenge"] as? String,
            let challengeData = challenge.data(using: .utf8) else {
        result(FlutterError(code: "INVALID_ARGUMENTS", message: "challenge is required", details: nil))
        return
      }

      do {
        let pubKeyBase64: String
        let attestationBase64: String

        if SecureEnclave.isAvailable {
          let seKey = try SecureEnclave.P256.Signing.PrivateKey()
          self.saveToKeychain(key: AppAttestPlugin.seKeyTag, data: seKey.dataRepresentation)
          pubKeyBase64 = seKey.publicKey.x963Representation.base64EncodedString()
          let sig = try seKey.signature(for: challengeData)
          attestationBase64 = sig.derRepresentation.base64EncodedString()
        } else {
          let p256Key = P256.Signing.PrivateKey()
          self.saveToKeychain(key: AppAttestPlugin.seKeyTag, data: p256Key.rawRepresentation)
          pubKeyBase64 = p256Key.publicKey.x963Representation.base64EncodedString()
          let sig = try p256Key.signature(for: challengeData)
          attestationBase64 = sig.derRepresentation.base64EncodedString()
        }

        if DCAppAttestService.shared.isSupported {
          DCAppAttestService.shared.generateKey { [weak self] keyId, error in
            if let keyId = keyId {
              self?.saveToKeychain(key: AppAttestPlugin.keyIdTag, data: keyId.data(using: .utf8)!)
              let clientDataHash = Data(SHA256.hash(data: challengeData))
              DCAppAttestService.shared.attestKey(keyId, clientDataHash: clientDataHash) { attestation, attestError in
                if let attestation = attestation {
                  result([
                    "publicKey": pubKeyBase64,
                    "attestation": attestation.base64EncodedString()
                  ])
                  return
                }
                result([
                  "publicKey": pubKeyBase64,
                  "attestation": attestationBase64
                ])
              }
              return
            }
            result([
              "publicKey": pubKeyBase64,
              "attestation": attestationBase64
            ])
          }
        } else {
          result([
            "publicKey": pubKeyBase64,
            "attestation": attestationBase64
          ])
        }
      } catch {
        result(FlutterError(code: "ENROLL_FAILED", message: error.localizedDescription, details: nil))
      }

    case "prove":
      guard #available(iOS 14.0, *) else {
        result(FlutterError(code: "UNSUPPORTED", message: "iOS 14.0 or higher is required", details: nil))
        return
      }

      guard let args = call.arguments as? [String: Any],
            let challenge = args["challenge"] as? String,
            let challengeData = challenge.data(using: .utf8) else {
        result(FlutterError(code: "INVALID_ARGUMENTS", message: "challenge is required", details: nil))
        return
      }

      guard let keyData = loadFromKeychain(key: AppAttestPlugin.seKeyTag) else {
        result(FlutterError(code: "KEY_NOT_FOUND", message: "Hardware key not found in Keychain", details: nil))
        return
      }

      do {
        let signatureBase64: String
        if SecureEnclave.isAvailable {
          let seKey = try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: keyData)
          let signature = try seKey.signature(for: challengeData)
          signatureBase64 = signature.derRepresentation.base64EncodedString()
        } else {
          let p256Key = try P256.Signing.PrivateKey(rawRepresentation: keyData)
          let signature = try p256Key.signature(for: challengeData)
          signatureBase64 = signature.derRepresentation.base64EncodedString()
        }
        result(signatureBase64)
      } catch {
        result(FlutterError(code: "PROVE_FAILED", message: error.localizedDescription, details: nil))
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
      deleteFromKeychain(key: AppAttestPlugin.seKeyTag)
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

