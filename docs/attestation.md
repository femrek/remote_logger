# 🛡️ Hardware Attestation Setup & Activation Guide

This guide explains how **Remote Logger** verifies mobile device authenticity using **Android Keystore Key Attestation** and **Apple DeviceCheck App Attest**, and how to configure, roll out, and activate attestation across your mobile apps and backend channels.

---

## 📑 Table of Contents

- [1. How Attestation Works](#1-how-attestation-works)
  - [Two-Tier Token Lifecycle](#two-tier-token-lifecycle)
  - [Attestation Modes (`off` · `monitor` · `enforce`)](#attestation-modes-off--monitor--enforce)
  - [Attestation Trust Tiers (`att` Claim)](#attestation-trust-tiers-att-claim)
- [2. Backend Prerequisites (ECDSA P-256 Keys)](#2-backend-prerequisites-ecdsa-p-256-keys)
- [3. Android Setup](#3-android-setup)
  - [Option A (Recommended): Hardware Keystore Attestation (TEE / StrongBox)](#option-a-recommended-hardware-keystore-attestation-tee--strongbox)
  - [Option B (Optional): Google Play Integrity API](#option-b-optional-google-play-integrity-api)
- [4. iOS Host App Setup (Apple App Attest & Secure Enclave)](#4-ios-host-app-setup-apple-app-attest--secure-enclave)
- [5. Activating Attestation in Consul KV](#5-activating-attestation-in-consul-kv)
  - [Option A: Via Logger Admin UI (`:8082`)](#option-a-via-logger-admin-ui-8082)
  - [Option B: Via Consul HTTP API](#option-b-via-consul-http-api)
- [6. Mobile Client SDK Configuration](#6-mobile-client-sdk-configuration)
  - [Production (Real Hardware Attestation)](#production-real-hardware-attestation)
  - [Simulators, Emulators & Automated Tests](#simulators-emulators--automated-tests)
- [7. Recommended Rollout Strategy (`off` → `monitor` → `enforce`)](#7-recommended-rollout-strategy-off--monitor--enforce)
- [8. Backend Verifier Implementation & Production Readiness Checklist](#8-backend-verifier-implementation--production-readiness-checklist)

---

## 1. How Attestation Works

Remote Logger uses a **stateless, two-tier cryptographic protocol** handled by `auth_worker` so that expensive hardware attestation operations occur only once every 90 days per device, while every app session proves key possession using a fast ECDSA P-256 signature.

```
Mobile App (PlatformAttestationProvider)                  logger_gateway          auth_worker
────────────────────────────────────────                  ──────────────          ───────────
1. GET /auth/challenge (X-Channel-Id) ──────────────────────────▶─────────────────▶ Creates 5-min HMAC challenge
                                      ◀──────────────────────────◀───────────────── { challenge, expires_in: 300 }

2. [First run or expired enrollment token]
   Generate hardware P-256 key (Android KeyStore / iOS Secure Enclave)
   POST /auth/enroll (X-Channel-Id)   ──────────────────────────▶─────────────────▶ Verifies challenge & hardware
   { platform, device_id, public_key,                                               attestation statement per mode;
     attestation, challenge }                                                       mints 90-day enrollment_token
                                      ◀──────────────────────────◀───────────────── { enrollment_token, expires_in }

3. [Every app session start]
   Sign challenge with hardware private key (`prove()`)
   POST /auth/exchange (X-Channel-Id) ──────────────────────────▶─────────────────▶ Verifies challenge + enrollment_token
   { platform, device_id, device_metadata,                                          + P-256 signature over challenge;
     enrollment_token, challenge,                                                   mints session access_token &
     challenge_signature }                                                          publishes SystemStartupEvent
                                      ◀──────────────────────────◀───────────────── { access_token, access_expires_in }
```

### Two-Tier Token Lifecycle

| Token | Endpoint | Default Lifespan | Purpose |
|---|---|---|---|
| **Challenge** | `GET /auth/challenge` | `300s` (5 min) | Stateless server-signed nonce (`<uuid>.<exp>.<sha256_sig>`) preventing replay of enrollment or session proofs. |
| **Enrollment JWT** | `POST /auth/enroll` | `7,776,000s` (90 days) | ES256 JWT (`token_type: "enrollment"`) binding `sub` (`device_id`), `app`, `plt`, `att`, and `pub_key` (the device's hardware-backed P-256 public key). Stored in Android `SharedPreferences` or iOS Keychain. |
| **Access JWT** | `POST /auth/exchange` | `86,400s` (24h) or `900s` (15m) | ES256 JWT (`token_type: "access"`) presented as `Authorization: Bearer <token>` on `/v1/logs/{channel}` and `/v1/logs/{channel}/batch`. |

### Attestation Modes (`off` · `monitor` · `enforce`)

Each channel configures Android (`android_attestation_mode`) and iOS (`ios_attestation_mode`) independently:

| Mode | Behavior at `/auth/enroll` and `/auth/exchange` | When to Use |
|---|---|---|
| **`off`** *(default)* | Skips attestation verification completely. Mints tokens with `att = "none"`. | Local development, CI pipelines, and simulator/emulator channels. |
| **`monitor`** | Runs full attestation verification. If verification **succeeds**, mints tokens with `att = "tee"` (or hardware tier). If verification **fails**, logs a warning and still mints tokens with `att = "none"`. | Staging and initial production rollout to verify client configuration without dropping logs. |
| **`enforce`** | Runs full attestation verification. If verification **fails**, rejects the request with `403 Forbidden` (`ATTESTATION_FAILED`). | Production channels after confirming clean metrics in `monitor` mode. |

### Attestation Trust Tiers (`att` Claim)

When `logger_gateway` validates an `access` JWT on log ingestion routes, it extracts `claims.att` and injects the internal `x-attestation` header upstream. `validation_worker` attaches this value as the `attestation` field on every indexed log document in OpenSearch (`mobile-logs-{app_id}`):
- `"strongbox"` / `"tee"` / `"secure_enclave"` — Hardware-backed attestation verified.
- `"none"` — Unverified session (channel mode is `off`, or attestation failed while in `monitor` mode, or legacy direct exchange).

---

## 2. Backend Prerequisites (ECDSA P-256 Keys)

`auth_worker` signs challenges, enrollment tokens, and access tokens using an ECDSA P-256 private key (`GATEWAY_PRIVATE_KEY_PATH`), and `logger_gateway` verifies access tokens using the corresponding public key (`GATEWAY_PUBLIC_KEY_PATH`).

Generate a production key pair:

```bash
openssl ecparam -name prime256v1 -genkey -noout -out private.pem
openssl ec -in private.pem -pubout -out public.pem
```

- Mount `private.pem` into **`auth_worker`** (`GATEWAY_PRIVATE_KEY_PATH=/etc/logger/keys/private.pem`).
- Mount `public.pem` into **`logger_gateway`** (`GATEWAY_PUBLIC_KEY_PATH=/etc/logger/keys/public.pem`).

---

## 3. Android Setup

You can verify Android devices using **Option A (Hardware Keystore Attestation — Recommended)** or **Option B (Google Play Integrity API)**.

### Option A (Recommended): Hardware Keystore Attestation (TEE / StrongBox)

This is the **default mechanism already built into [`KeyAttestationPlugin.kt`](../mobile/flutter_remote_logging/android/src/main/kotlin/dev/femrek/flutter_remote_logging/KeyAttestationPlugin.kt)**.
- Generates a hardware-backed EC P-256 (`secp256r1`) key pair inside **StrongBox** (API 28+) or **TEE** with `.setAttestationChallenge(challenge)`.
- Produces an X.509 certificate chain (`attestation` in `/auth/enroll`) verified directly by `auth_worker` against Google's Hardware Attestation Root CA.
- **No Google Cloud project, service account, or API quota is required.**

#### Step A.1: Set Minimum SDK Version
Ensure `minSdkVersion` is **24** (Android 7.0) or higher in `android/app/build.gradle`.

#### Step A.2: Get Your Android Signing Certificate SHA-256 Digest
The hardware attestation certificate embeds your app's package name (`app_id`) and signing certificate SHA-256 digest inside its `AttestationApplicationId` ASN.1 extension.
1. **For Local / Upload / Internal Keystores:**
   Extract the SHA-256 fingerprint using `keytool`:
   ```bash
   keytool -list -v -keystore your-release-key.jks -alias your-key-alias | grep "SHA256:"
   ```
2. **For Google Play Store Builds (if using Play App Signing):**
   Go to **Google Play Console → Test and release → App integrity → App signing** and copy the **SHA-256 certificate fingerprint** under *App signing key certificate*.
3. **Format for Consul KV:**
   Normalize the fingerprint to lowercase hex (or colon-separated hex as expected by your verifier) and add it to `android_signing_cert_sha256` in Consul KV:
   ```bash
   echo "AB:CD:12:..." | tr -d ':' | tr '[:upper:]' '[:lower:]'
   ```

---

### Option B (Optional): Google Play Integrity API

Use this option only if you use the legacy direct `/auth/exchange` flow (`attestation_token`) or want to add an extra cloud Play Integrity check alongside hardware keystore attestation to detect unrevoked leaked keyboxes.

#### Step B.1: Create a Google Cloud Project & Enable Play Integrity API
1. Open the **Google Cloud Console** (`https://console.cloud.google.com/`).
2. Create a new project or select your existing project.
3. On the **Project Dashboard**, copy the numeric **Project Number** (e.g., `123456789012` — *not* the string Project ID).
   - Set this as `google_project_number` in Consul KV.
4. Go to **APIs & Services → Library**, search for **Google Play Integrity API**, and click **Enable**.

#### Step B.2: Create a Service Account for `auth_worker`
1. In Google Cloud Console, navigate to **IAM & Admin → Service Accounts**.
2. Click **+ Create Service Account** (e.g., `auth-worker-integrity-verifier`) and grant **Service Usage Consumer** (`roles/serviceusage.serviceUsageConsumer`).
3. Open the service account → **Keys → Add Key → Create new key → JSON**.
4. Store the downloaded JSON key as a Kubernetes Secret and mount it into `auth_worker`.

#### Step B.3: Link Google Cloud Project in Google Play Console
1. Open **Google Play Console** (`https://play.google.com/console/`) and select your Android app.
2. Navigate to **Test and release → App integrity** (or **Setup → App integrity**).
3. Next to **Play Integrity API**, click **Link a Cloud project** and select the Google Cloud project from Step B.1.

---

## 4. iOS Host App Setup (Apple App Attest & Secure Enclave)

`flutter_remote_logging` includes [`AppAttestPlugin.swift`](../mobile/flutter_remote_logging/ios/Classes/AppAttestPlugin.swift), which uses Apple **Secure Enclave** (`SecureEnclave.P256.Signing.PrivateKey`) and **DeviceCheck App Attest** (`DCAppAttestService`).

### Step 4.1: Enable App Attest in Apple Developer Portal
1. Sign in to the **Apple Developer Portal** (`https://developer.apple.com/account`).
2. Copy your 10-character **Team ID** (e.g., `ABCDE12345`) from the top-right account header.
   - You will set this as `apple_team_id` in Consul KV.
3. Navigate to **Certificates, Identifiers & Profiles → Identifiers** and select your app's **App ID** (matching `app_id`, e.g., `com.company.app`).
4. Scroll down the **Capabilities** list, check **App Attest**, and click **Save**.
5. Regenerate your provisioning profiles if you manage profiles manually (Xcode Automatic Signing refreshes them automatically).

### Step 4.2: Add the App Attest Capability in Xcode
Because a Flutter plugin cannot add entitlements to the host app automatically, you must enable it on your iOS Runner target:
1. Ensure your iOS deployment target is **iOS 14.0** or higher in `ios/Podfile` and Xcode.
2. Open `ios/Runner.xcworkspace` in **Xcode**.
3. Select the **Runner** target → **Signing & Capabilities** tab.
4. Click **+ Capability** and add **App Attest**.
5. In `ios/Runner/Runner.entitlements`, verify the `com.apple.developer.devicecheck.appattest-environment` key matches your build type:
   ```xml
   <?xml version="1.0" encoding="UTF-8"?>
   <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
   <plist version="1.0">
   <dict>
       <!-- Use "development" for Xcode debug builds; "production" for TestFlight & App Store -->
       <key>com.apple.developer.devicecheck.appattest-environment</key>
       <string>development</string>
   </dict>
   </plist>
   ```
6. Set `apple_app_attest_env` in Consul KV to match (`"development"` or `"production"`).

---

## 5. Activating Attestation in Consul KV

Channel attestation policies live in HashiCorp Consul KV under `gateway/config/channels/{channel_id}`. `auth_worker` watches this prefix via Consul HTTP blocking queries and hot-reloads changes in real time with **zero downtime**.

### Option A: Via Logger Admin UI (`:8082`)

1. Open `http://localhost:8082` (or your deployed `logger_admin` URL) and sign in with your `GATEWAY_ADMIN_TOKEN`.
2. Add or edit your target channel:
   - **Channel ID**: e.g., `mobile-app-prod`
   - **App ID**: Your application bundle/package identifier (e.g., `com.company.app`)
   - **Google Project Number**: Your GCP project number (required when Android mode is `monitor` or `enforce`)
   - **Apple Team ID**: Your 10-character Apple Team ID (required when iOS mode is `monitor` or `enforce`)
   - **Android Mode**: Select `Off`, `Monitor`, or `Enforce`
   - **iOS Mode**: Select `Off`, `Monitor`, or `Enforce`
3. Click **Save Changes**.

### Option B: Via Consul HTTP API

Write the channel configuration JSON directly to Consul KV:

```bash
curl -X PUT "http://localhost:8500/v1/kv/gateway/config/channels/mobile-app-prod" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "mobile-app-prod",
    "app_id": "com.company.app",
    "android_attestation_mode": "monitor",
    "ios_attestation_mode": "monitor",
    "google_project_number": "123456789012",
    "apple_team_id": "ABCDE12345",
    "android_signing_cert_sha256": [
      "ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67"
    ],
    "apple_app_attest_env": "production"
  }'
```

---

## 6. Mobile Client SDK Configuration

### Production (Real Hardware Attestation)

When using `FlutterRemoteLogging.initRemote`, **`PlatformAttestationProvider` is enabled by default**. You do not need to pass a custom callback:

```dart
import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  const gatewayUrl = 'https://logger.example.com';

  final remoteLogger = await FlutterRemoteLogging.initRemote(
    remoteLoggingConfig: const RemoteLoggingConfig(
      gatewayUrl: gatewayUrl,
      channelId: 'mobile-app-prod',
    ),
    // Defaults to `const PlatformAttestationProvider()`
  );

  // Start the background sender (triggers /auth/challenge -> /auth/enroll -> /auth/exchange)
  final invoker = DioNetworkInvoker.fromBaseUrl(gatewayUrl);
  remoteLogger.start(invoker);

  runApp(const MyApp());
}
```

### Simulators, Emulators & Automated Tests

Hardware-backed attestation keys (`DCAppAttestService` / Android hardware Keystore) are unavailable in iOS Simulators, uncertified emulators, and CI test runners. For development channels (configured with `android_attestation_mode: "off"` and `ios_attestation_mode: "off"`), pass `FakeAttestationProvider()` or `NoAttestationProvider()`:

```dart
final remoteLogger = await FlutterRemoteLogging.initRemote(
  remoteLoggingConfig: const RemoteLoggingConfig(
    gatewayUrl: 'http://localhost:8080',
    channelId: 'example-app-dev',
  ),
  attestationProvider: FakeAttestationProvider(),
);
```

---

## 7. Recommended Rollout Strategy (`off` → `monitor` → `enforce`)

1. **Stage 1 — Development (`off`)**:
   Keep `example-app-dev` set to `off` so local emulators, simulators, and CI tests work without hardware keys.
2. **Stage 2 — Shadow Validation (`monitor`)**:
   Set your production channel's `android_attestation_mode` and `ios_attestation_mode` to `monitor`.
   - Test on real physical devices using the interactive diagnostic app in [`mobile/example`](../mobile/example/README.md).
   - Query OpenSearch (`mobile-logs-*`) and inspect the `attestation` field across incoming logs (`"tee"` vs `"none"`), and monitor `auth_worker` logs for any attestation warnings.
3. **Stage 3 — Strict Enforcement (`enforce`)**:
   Once legitimate physical devices consistently produce `attestation: "tee"` (or `"strongbox"` / `"secure_enclave"`), switch the channel mode to `enforce` in Logger Admin UI. Any unverified or tampered client will immediately receive `403 Forbidden` during `/auth/enroll` and `/auth/exchange`.

---

## 8. Backend Verifier Implementation & Production Readiness Checklist

> [!IMPORTANT]
> Before switching a production channel to `enforce`, ensure the following backend verifier implementation items are completed in [`auth_worker/src/auth/attestation.rs`](../auth_worker/src/auth/attestation.rs):

### 8.1 Android Option A (Recommended): Keystore Key Attestation Verifier (`POST /auth/enroll`)
1. **Parse Certificate Chain:** Base64-decode `req.attestation` into the DER X.509 certificate chain (`cert[0]` = leaf key certificate, `cert[n-1]` = root CA).
2. **Verify Signatures & Root CA:** Validate each certificate signature up to the pinned **Google Hardware Attestation Root CA** public key.
3. **Check Keybox Revocation List:** Verify that no serial number in the chain appears on Google's revocation list (`https://android.googleapis.com/attestation/status`).
4. **Parse ASN.1 Key Attestation Extension (`1.3.6.1.4.1.11129.2.1.17`):**
   - Confirm `attestationChallenge == req.challenge`.
   - Confirm `attestationSecurityLevel` is `TrustedEnvironment (1)` or `StrongBox (2)`.
   - Confirm `rootOfTrust.verifiedBootState == Verified (0)` and `deviceLocked == true`.
   - Extract `AttestationApplicationId` and verify the package name equals `channel.app_id` and its signing certificate digest matches `channel.android_signing_cert_sha256`.
5. **Public Key Binding:** Confirm `cert[0]`'s SubjectPublicKeyInfo matches `req.public_key`.

### 8.2 Android Option B (Optional): Google Play Integrity API Verifier (`GooglePlayIntegrityVerifier`)
1. **Acquire OAuth2 Token:** Use the Google Cloud Service Account JSON (from Step B.2) to mint a short-lived Bearer token for scope `https://www.googleapis.com/auth/playintegrity`.
2. **Call `decodeIntegrityToken`:**
   ```http
   POST https://playintegrity.googleapis.com/v1/{app_id}:decodeIntegrityToken
   Authorization: Bearer <google_oauth_token>
   Content-Type: application/json

   { "integrity_token": "<token>" }
   ```
3. **Validate Verdict (`tokenPayloadExternal`):**
   - `requestDetails.requestPackageName == channel.app_id`
   - `requestDetails.nonce == expected_challenge`
   - `appIntegrity.appRecognitionVerdict == "PLAY_RECOGNIZED"`
   - `appIntegrity.certificateSha256Digest` intersects `channel.android_signing_cert_sha256`
   - `deviceIntegrity.deviceRecognitionVerdict` contains `"MEETS_DEVICE_INTEGRITY"` or `"MEETS_STRONG_INTEGRITY"`

### 8.3 Apple App Attest Verifier (`AppleAppAttestVerifier`)
1. **Decode CBOR Attestation Object:** Base64-decode `req.attestation` and parse the CBOR map (`fmt == "apple-appattest"`, `attStmt.x5c`, `authData`).
2. **Verify Certificate Chain:** Validate `attStmt.x5c` up to the pinned **Apple App Attest Root CA**.
3. **Verify Nonce Extension (`1.2.840.113635.100.8.2`):**
   - Compute `clientDataHash = SHA256(req.challenge)` and `nonce = SHA256(authData || clientDataHash)`.
   - Confirm the leaf certificate (`x5c[0]`) extension `1.2.840.113635.100.8.2` equals `nonce`.
4. **Verify Authenticator Data (`authData`):**
   - `rpIdHash == SHA256("{apple_team_id}.{app_id}")`
   - `signCount == 0`
   - `aaguid` matches `channel.apple_app_attest_env` (`appattestdevelop` for `"development"`, `appattest\0\0\0\0\0\0\0` for `"production"`).

### 8.4 Production Key Rotation
- [ ] Replace `docker/keys/private.pem` and `docker/keys/public.pem` with freshly generated production ECDSA P-256 keys (Section 2).

