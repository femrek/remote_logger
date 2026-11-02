# 🛡️ Hardware Attestation Setup & Activation Guide

This guide explains how **Remote Logger** verifies mobile device authenticity using **Android Keystore Key Attestation** and **Apple DeviceCheck App Attest**, and how to configure, roll out, and activate attestation across your mobile apps and backend channels.

---

## 📑 Table of Contents

- [1. Why Use Hardware Attestation?](#1-why-use-hardware-attestation)
  - [Core Problems with Client-Side Log Ingestion](#core-problems-with-client-side-log-ingestion)
  - [How Hardware Attestation Solves Them](#how-hardware-attestation-solves-them)
  - [Zero Cloud API Dependencies & Total Privacy](#zero-cloud-api-dependencies--total-privacy)
  - [Disabled by Default (Server & Mobile Config)](#disabled-by-default-server--mobile-config)
- [2. How Attestation Works](#2-how-attestation-works)
  - [Two-Tier Token Lifecycle](#two-tier-token-lifecycle)
  - [Attestation Modes (`off` · `monitor` · `enforce`)](#attestation-modes-off--monitor--enforce)
  - [Attestation Trust Tiers (`att` Claim)](#attestation-trust-tiers-att-claim)
- [3. Backend Prerequisites (ECDSA P-256 Keys)](#3-backend-prerequisites-ecdsa-p-256-keys)
- [4. Android Setup (Hardware Keystore Attestation — TEE / StrongBox)](#4-android-setup-hardware-keystore-attestation--tee--strongbox)
- [5. iOS Host App Setup (Apple App Attest & Secure Enclave)](#5-ios-host-app-setup-apple-app-attest--secure-enclave)
- [6. Activating Attestation in Consul KV](#6-activating-attestation-in-consul-kv)
  - [Option A: Via Logger Admin UI (`:8082`)](#option-a-via-logger-admin-ui-8082)
  - [Option B: Via Consul HTTP API](#option-b-via-consul-http-api)
- [7. Mobile Client SDK Configuration](#7-mobile-client-sdk-configuration)
  - [Production (Real Hardware Attestation)](#production-real-hardware-attestation)
  - [Simulators, Emulators & Automated Tests](#simulators-emulators--automated-tests)
- [8. Recommended Rollout Strategy (`off` → `monitor` → `enforce`)](#8-recommended-rollout-strategy-off--monitor--enforce)
- [9. Backend Verifier Architecture & Production Readiness Checklist](#9-backend-verifier-architecture--production-readiness-checklist)

---

## 1. Why Use Hardware Attestation?

### Core Problems with Client-Side Log Ingestion

Unlike internal microservices running behind a private VPC, mobile client telemetry endpoints must be publicly accessible on the open internet so mobile applications can stream diagnostic events from anywhere in the world.

In traditional ingestion architectures, apps use an embedded API key or static shared secret to authenticate with the server. However:
- **Keys Are Easily Extracted:** Any motivated attacker can decompile an APK/IPA, reverse-engineer strings, or run a proxy tool (e.g., Charles, Proxyman, mitmproxy) to intercept the API key.
- **Log Poisoning & Fraud:** Once an API key is exposed, an attacker can write a lightweight script or botnet to flood the ingestion pipeline with fake logs, falsified error reports, and misleading analytics events.
- **Denial of Service & Storage Exhaustion:** Unchecked client endpoints can be inundated with millions of synthetic payloads, degrading Kafka broker performance and exhausting OpenSearch storage capacity.
- **Fabricated Device Metadata:** Attackers can report arbitrary device models, forged operating system versions, and fake crash traces that skew engineering prioritization.

---

### How Hardware Attestation Solves Them

Hardware attestation moves trust away from extractable software secrets and roots it inside the device's physical security chip:
- **Android**: **Android Keystore Key Attestation**, backed by a hardware **TEE (Trusted Execution Environment)** or dedicated **StrongBox Keymaster** (tamper-resistant security module).
- **iOS**: **Apple DeviceCheck App Attest** and the **Apple Secure Enclave**.

When hardware attestation is enabled:
1. **Unexportable Private Keys:** The mobile app generates a cryptographic P-256 key pair inside the hardware chip. The private key cannot be extracted from the device even if the operating system is rooted, jailbroken, or inspected in memory.
2. **Cryptographic Proof of App Identity:** The operating system produces an attestation statement signed directly by the hardware manufacturer (Google or Apple Root CA). This statement cryptographically binds the generated key to:
   - The authentic application package name or bundle ID (`app_id`).
   - The developer's signing certificate hash or Apple Team ID.
   - The hardware security tier (e.g. StrongBox, TEE, or Secure Enclave).
3. **Defense Against Replay & Tampering:** The server issues an ephemeral HMAC challenge for every enrollment and session handshake. The device proves it owns the hardware key by signing that challenge, eliminating replay attacks.

---

### Zero Cloud API Dependencies & Total Privacy

Most mobile integrity solutions (such as Google Play Integrity API or Firebase App Check) require sending runtime attestation requests to Google or third-party servers, introducing:
- Network roundtrips to external third-party cloud infrastructure.
- Daily or monthly quota limitations and potential billing charges.
- External dependencies that can cause telemetry drops during third-party outages.

**Remote Logger verifies hardware statements 100% offline.** `auth_worker` parses the X.509 certificate chain (Android) or App Attest CBOR receipt (Apple) and checks the signatures locally against pinned manufacturer Root CAs. No external network requests are made during token exchange.

---

### Disabled by Default (Server & Mobile Config)

> [!NOTE]
> **Hardware attestation is disabled (`off`) by default across both backend configurations and mobile development workflows.**

#### Why is it disabled by default?
- **Developer Ergonomics:** Android emulators, iOS simulators, and desktop development targets do not contain physical hardware security chips (StrongBox / Secure Enclave) or valid manufacturer root certificate chains.
- **Automated CI/CD Tests:** Automated end-to-end tests, unit tests, and continuous integration runners need to exchange tokens and stream logs without physical mobile devices attached.
- **Smooth Onboarding:** Teams can deploy the pipeline and immediately test log ingestion without first provisioning Apple Developer Portal entitlements or setting up Android signing keys.

#### How it is configured:
- **Server Configuration:** In Consul KV, all newly created channels and default seed channels (such as `example-app-dev`) initialize with:
  ```json
  "android_attestation_mode": "off",
  "ios_attestation_mode": "off"
  ```
  In `off` mode, `auth_worker` skips hardware statement checks entirely and mints access tokens stamped with `att = "none"`.
- **Mobile SDK Configuration:** When testing on simulators, emulators, or in local development environments, mobile applications configure `NoAttestationProvider()`:
  ```dart
  attestationProvider: const NoAttestationProvider(),
  ```
  Once you are ready to enforce hardware integrity on physical production devices, you switch channel modes to `monitor` and then `enforce` without any server redeployments or downtime (see [Section 8](#8-recommended-rollout-strategy-off--monitor--enforce)).

---

## 2. How Attestation Works

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
| **`monitor`** | Runs full attestation verification. If verification **succeeds**, mints tokens with the verified hardware tier (`"strongbox"`, `"tee"`, or `"secure_enclave"`). If verification **fails**, logs a warning and still mints tokens with `att = "none"`. | Staging and initial production rollout to verify client configuration without dropping logs. |
| **`enforce`** | Runs full attestation verification. If verification **fails**, rejects the request with `403 Forbidden` (`ATTESTATION_FAILED`). | Production channels after confirming clean metrics in `monitor` mode. |

### Attestation Trust Tiers (`att` Claim)

When `logger_gateway` validates an `access` JWT on log ingestion routes, it extracts `claims.att` and injects the internal `x-attestation` header upstream. `validation_worker` attaches this value as the `attestation` field on every indexed log document in OpenSearch (`mobile-logs-{app_id}`):
- `"strongbox"` — Android hardware key backed by dedicated StrongBox secure element (`attestationSecurityLevel == 2`).
- `"tee"` — Android hardware key backed by Trusted Execution Environment (`attestationSecurityLevel == 1`).
- `"secure_enclave"` — iOS hardware key backed by Apple Secure Enclave / DeviceCheck App Attest.
- `"none"` — Unverified session (channel mode is `off`, or attestation failed while in `monitor` mode, or legacy direct exchange without attestation).

---

## 3. Backend Prerequisites (ECDSA P-256 Keys)

`auth_worker` signs challenges, enrollment tokens, and access tokens using an ECDSA P-256 private key (`GATEWAY_PRIVATE_KEY_PATH`), and `logger_gateway` verifies access tokens using the corresponding public key (`GATEWAY_PUBLIC_KEY_PATH`).

Generate a production key pair:

```bash
openssl ecparam -name prime256v1 -genkey -noout -out private.pem
openssl ec -in private.pem -pubout -out public.pem
```

- Mount `private.pem` into **`auth_worker`** (`GATEWAY_PRIVATE_KEY_PATH=/etc/logger/keys/private.pem`).
- Mount `public.pem` into **`logger_gateway`** (`GATEWAY_PUBLIC_KEY_PATH=/etc/logger/keys/public.pem`).

---

## 4. Android Setup (Hardware Keystore Attestation — TEE / StrongBox)

Android attestation uses the offline **Android Keystore Key Attestation** mechanism built into [`KeyAttestationPlugin.kt`](../mobile/flutter_remote_logging/android/src/main/kotlin/dev/femrek/flutter_remote_logging/KeyAttestationPlugin.kt) and verified by [`AndroidKeyAttestationVerifier`](../auth_worker/src/auth/attestation.rs):
- Generates a hardware-backed EC P-256 (`secp256r1`) key pair inside **StrongBox** (API 28+) or **TEE** with `.setAttestationChallenge(challenge)`.
- Produces an X.509 PEM certificate chain (`attestation` in `/auth/enroll`) verified offline by `auth_worker` against Google's Hardware Attestation Root CAs.
- **No Google Cloud project, service account, or external API quota is required.**

### Step 4.1: Set Minimum SDK Version
Ensure `minSdkVersion` is **24** (Android 7.0) or higher in `android/app/build.gradle`.

### Step 4.2: Optional — Restrict Allowed APK Signing Certificate SHA-256 Digests
The Android hardware attestation certificate embeds your app's package name (`app_id`) and APK signing certificate SHA-256 digest inside its `AttestationApplicationId` ASN.1 extension (`1.3.6.1.4.1.11129.2.1.17`).

- **Default (`android_signing_cert_sha256: []`)**: `auth_worker` always verifies the X.509 chain up to Google's Hardware Attestation Root CA, checks that `attestationSecurityLevel` is `TrustedEnvironment (1)` or `StrongBox (2)`, verifies `attestationChallenge`, and enforces that the attested package name matches `channel.app_id`. Leaving `android_signing_cert_sha256` empty skips the APK signing certificate hash check so both debug and release builds work out-of-the-box.
- **Strict Production Pinning (Recommended for Release Channels)**: Populate `android_signing_cert_sha256` with one or more allowed SHA-256 digests (lowercase/uppercase hex, with or without colons) to block repackaged APKs signed with unauthorized keys:
  1. **For Local / Upload / Internal Keystores:**
     ```bash
     keytool -list -v -keystore your-release-key.jks -alias your-key-alias | grep "SHA256:"
     ```
  2. **For Google Play Store Builds (Play App Signing):**
     Go to **Google Play Console → Test and release → App integrity → App signing** and copy the **SHA-256 certificate fingerprint** under *App signing key certificate*.

---

## 5. iOS Host App Setup (Apple App Attest & Secure Enclave)

`flutter_remote_logging` includes [`AppAttestPlugin.swift`](../mobile/flutter_remote_logging/ios/Classes/AppAttestPlugin.swift), which supports two hardware-backed modes verified by [`AppleAppAttestVerifier`](../auth_worker/src/auth/attestation.rs):
1. **Full Apple DeviceCheck App Attest (CBOR)**: Used when App Attest is available and entitled, and enforced when `apple_team_id` is configured on the channel.
2. **Secure Enclave Key Possession Fallback**: When `apple_team_id` is omitted (`None`), `auth_worker` accepts either a valid App Attest CBOR object or a hardware-backed Secure Enclave ECDSA P-256 signature over the challenge (`AttestationTier::SecureEnclave`).

### Step 5.1: Enable App Attest in Apple Developer Portal (When Using `apple_team_id`)
1. Sign in to the **Apple Developer Portal** (`https://developer.apple.com/account`).
2. Copy your 10-character **Team ID** (e.g., `ABCDE12345`) from the top-right account header and set it as `apple_team_id` in Consul KV.
3. Navigate to **Certificates, Identifiers & Profiles → Identifiers** and select your app's **App ID** (matching `app_id`, e.g., `com.company.app`).
4. Scroll down the **Capabilities** list, check **App Attest**, and click **Save**.
5. Regenerate your provisioning profiles if you manage profiles manually (Xcode Automatic Signing refreshes them automatically).

### Step 5.2: Add the App Attest Capability in Xcode
Because a Flutter plugin cannot add entitlements to the host app automatically, enable it on your iOS Runner target:
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
6. Optionally set `apple_app_attest_env` in Consul KV to match (`"development"` or `"production"`).

---

## 6. Activating Attestation in Consul KV

Channel attestation policies live in HashiCorp Consul KV under `gateway/config/channels/{channel_id}`. `auth_worker` watches this prefix via Consul HTTP blocking queries and hot-reloads changes in real time with **zero downtime**.

### Option A: Via Logger Admin UI (`:8082`)

1. Open `http://localhost:8082` (or your deployed `logger_admin` URL) and sign in with your `GATEWAY_ADMIN_TOKEN`.
2. Add or edit your target channel:
   - **Channel ID**: e.g., `mobile-app-prod`
   - **App ID**: Your application bundle/package identifier (e.g., `com.company.app`)
   - **Android Signing Cert SHA-256**: Optional comma-separated list of allowed APK signing certificate SHA-256 digests (leave empty to skip certificate hash pinning)
   - **Apple Team ID**: Optional 10-character Apple Team ID (set to require full CBOR App Attest; leave empty to allow Secure Enclave fallback)
   - **Apple App Attest Env**: Optional environment (`Any (unset)`, `Development`, or `Production`)
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
    "apple_team_id": "ABCDE12345",
    "android_signing_cert_sha256": [
      "ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67"
    ],
    "apple_app_attest_env": "production"
  }'
```

---

## 7. Mobile Client SDK Configuration

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

Hardware-backed attestation keys (`DCAppAttestService` / Android hardware Keystore) are unavailable in iOS Simulators, uncertified emulators, and CI test runners. For development channels (configured with `android_attestation_mode: "off"` and `ios_attestation_mode: "off"`), pass `const NoAttestationProvider()`:

```dart
final remoteLogger = await FlutterRemoteLogging.initRemote(
  remoteLoggingConfig: const RemoteLoggingConfig(
    gatewayUrl: 'http://localhost:8080',
    channelId: 'example-app-dev',
  ),
  attestationProvider: const NoAttestationProvider(),
);
```

---

## 8. Recommended Rollout Strategy (`off` → `monitor` → `enforce`)

1. **Stage 1 — Development (`off`)**:
   Keep `example-app-dev` set to `off` so local emulators, simulators, and CI tests work without hardware keys.
2. **Stage 2 — Shadow Validation (`monitor`)**:
   Set your production channel's `android_attestation_mode` and `ios_attestation_mode` to `monitor`.
   - Test on real physical devices using the interactive diagnostic app in [`mobile/example`](../mobile/example/README.md).
   - Query OpenSearch (`mobile-logs-*`) and inspect the `attestation` field across incoming logs (`"strongbox"`, `"tee"`, `"secure_enclave"`, vs `"none"`), and monitor `auth_worker` logs for any attestation warnings.
3. **Stage 3 — Strict Enforcement (`enforce`)**:
   Once legitimate physical devices consistently produce hardware-backed `attestation` tiers, switch the channel mode to `enforce` in Logger Admin UI. Any unverified or tampered client will immediately receive `403 Forbidden` during `/auth/enroll` and `/auth/exchange`.

---

## 9. Backend Verifier Architecture & Production Readiness Checklist

The offline cryptographic verifiers in [`auth_worker/src/auth/attestation.rs`](../auth_worker/src/auth/attestation.rs) perform the following checks during `POST /auth/enroll` and direct `POST /auth/exchange`:

### 9.1 Android Keystore Key Attestation Verifier (`AndroidKeyAttestationVerifier`)
1. **Parse Certificate Chain:** Base64-decodes `req.attestation` and parses the concatenated PEM X.509 certificate chain (`cert[0]` = leaf key certificate, `cert[n-1]` = root CA).
2. **Verify Signatures & Root CA:** Validates every certificate signature (`cert[i]` signed by `cert[i+1]`) and confirms `cert[n-1]` is self-signed and matches one of the pinned **Google Hardware Attestation Root CAs** (RSA-4096 and EC P-384 roots).
3. **Public Key Binding:** Confirms `cert[0]`'s SubjectPublicKeyInfo EC P-256 public key matches `req.public_key`.
4. **Parse ASN.1 Key Attestation Extension (`1.3.6.1.4.1.11129.2.1.17`):**
   - Confirms `attestationChallenge == req.challenge`.
   - Confirms `attestationSecurityLevel` is `TrustedEnvironment (1)` → `AttestationTier::Tee` (`"tee"`) or `StrongBox (2)` → `AttestationTier::StrongBox` (`"strongbox"`). Rejects `Software (0)`.
   - Extracts `AttestationApplicationId` (tag `[709]`) from `softwareEnforced` / `teeEnforced` `AuthorizationList` and verifies the attested package name equals `channel.app_id`.
   - When `channel.android_signing_cert_sha256` is non-empty, verifies that at least one attested APK signing certificate SHA-256 digest matches the configured list.

### 9.2 Apple App Attest & Secure Enclave Verifier (`AppleAppAttestVerifier`)
1. **Mode 1 — Full CBOR App Attest (`fmt == "apple-appattest"`):**
   - Validates `attStmt.x5c` certificate chain signatures up to the pinned **Apple App Attest Root CA**.
   - Computes `clientDataHash = SHA256(req.challenge)` and `expectedNonce = SHA256(authData || clientDataHash)`, and confirms leaf certificate (`x5c[0]`) extension `1.2.840.113635.100.8.2` equals `expectedNonce`.
   - When `channel.apple_team_id` is configured, verifies `authData[0..32]` (`rpIdHash`) equals `SHA256("{apple_team_id}.{app_id}")`.
   - When `channel.apple_app_attest_env` is configured, verifies `authData[37..53]` (`aaguid`) matches `"development"` (`appattestdevelop`) or `"production"` (`appattest\0\0\0\0\0\0\0`).
   - Returns `AttestationTier::SecureEnclave` (`"secure_enclave"`).
2. **Mode 2 — Secure Enclave ECDSA P-256 Signature Fallback:**
   - When `channel.apple_team_id` is `None` and the token is an ECDSA P-256 DER signature over `challenge`, verifies the signature against `req.public_key` and returns `AttestationTier::SecureEnclave` (`"secure_enclave"`).

### 9.3 Production Key Rotation Checklist
- [ ] Replace `docker/keys/private.pem` and `docker/keys/public.pem` with freshly generated production ECDSA P-256 keys (Section 3).

