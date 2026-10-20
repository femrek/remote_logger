# Flutter Remote Logging

A Flutter extension of the [`remote_logging`](../remote_logging) package designed to seamlessly send structured mobile
logs to a self-hosted Remote Logger backend.

It provides out-of-the-box device data collection, offline persistence using SQLite, persistent device/session ID
tracking, and attestation token handling.

---

## Features

- 📱 **Automated Device Metadata**: Automatically collects device metadata (OS, OS version, model, brand, manufacturer,
  package identifier, physical device check) across **Android, iOS, macOS, Windows, Linux, and Web**.
- 💾 **SQLite Offline Persistence**: Uses `sqflite` to locally queue unsent log batches (`SqfliteLocalLogRepository`).
  Logs survive network outages and app restarts.
- 🆔 **Device & Session Tracking**: Automatically manages persistent device UUIDs (persisted in `shared_preferences`) and
  unique per-session UUIDs.
- 🔒 **Hardware Attestation Built-In**: Defaults to `PlatformAttestationProvider` (Android Keystore Key Attestation & Apple DeviceCheck App Attest / Secure Enclave) for 90-day key enrollment and per-session challenge signing, with `FakeAttestationProvider` for simulators and tests.
- 📦 **Single-Import Convenience**: Re-exports all core classes from `package:remote_logging`, eliminating duplicate
  package imports.

---

## Installation

Add `flutter_remote_logging` to your `pubspec.yaml`:

```yaml
dependencies:
  flutter_remote_logging: ^0.0.2
```

---

## Quick Start

### 1. Initialize & Start Remote Logging

Initialize `FlutterRemoteLogging` early in your application startup (e.g., inside `main()`) and start the background log sender with an `INetworkInvoker`:

```dart
import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  const gatewayUrl = 'https://logger.example.com';

  // Initialize Remote Logging for Flutter (uses PlatformAttestationProvider by default)
  final remoteLogger = await FlutterRemoteLogging.initRemote(
    remoteLoggingConfig: const RemoteLoggingConfig(
      gatewayUrl: gatewayUrl,
      channelId: 'mobile-app-prod',
    ),
    onLog: (log) {
      debugPrint('[LOG] ${log.level}: ${log.message}');
    },
    onError: (error, stackTrace) {
      debugPrint('[LOGGING ERROR] $error');
    },
  );

  // Start the background transmission loop & initial attestation exchange
  final invoker = DioNetworkInvoker.fromBaseUrl(gatewayUrl);
  remoteLogger.start(invoker);

  runApp(const MyApp());
}
```

### 2. Log Events

Once initialized, use standard `logging` package APIs or standard log calls:

```dart
import 'package:flutter_remote_logging/flutter_remote_logging.dart';

final logger = Logger('UserService');

void loginUser(String userId) {
  logger.info('User initiated login sequence', {'userId': userId});

  try {
    // Perform login...
  } catch (e, stackTrace) {
    logger.severe('Login failed', e, stackTrace);
  }
}
```

---

## Architecture & How It Works

```
 ┌─────────────────────────────────────────────────────────────┐
 │                       Flutter App                           │
 └──────────────────────────────┬──────────────────────────────┘
                                │ log.info(...)
                                ▼
 ┌─────────────────────────────────────────────────────────────┐
 │                  FlutterRemoteLogging                       │
 │  • FlutterDeviceDataGenerator  (OS, Model, DeviceID, etc.)  │
 │  • PlatformAttestationProvider (KeyStore / App Attest)      │
 └──────────────┬───────────────────────────────┬──────────────┘
                │ Success                       │ Offline / Retries
                ▼                               ▼
 ┌──────────────────────────────┐ ┌───────────────────────────┐
 │ Remote Logger Gateway Backend│ │ SqfliteLocalLogRepository │
 └──────────────────────────────┘ └───────────────────────────┘
```

When calling `FlutterRemoteLogging.initRemote(...)`, the library automatically:
1. Instantiates a `FlutterDeviceDataGenerator` which fetches system information via `device_info_plus` & `package_info_plus`, and retrieves or generates a persistent device ID via `shared_preferences`.
2. Configures `PlatformAttestationProvider` (unless a custom `attestationProvider` is passed) to perform stateless `/auth/challenge` → `/auth/enroll` → `/auth/exchange` hardware attestation.
3. Initializes `SqfliteLocalLogRepository` to back up unsent logs to a local SQLite database (`remote_logs.db`).
4. Bridges log entries directly into the `remote_logging` transmission engine.

---

## API Reference

### `FlutterRemoteLogging.initRemote`

```dart
static Future<RemoteLogging> initRemote({
  required RemoteLoggingConfig remoteLoggingConfig,
  DeviceDataGenerator? deviceDataGenerator,
  AttestationProvider? attestationProvider,
  GetAttestationTokenCallback? getAttestationTokenCallback,
  LocalLogRepository? localLogRepository,
  bool enableDefaultLocalFallbackStorage = true,
  OnLog? onLog,
  OnError? onError,
})
```

| Parameter                           | Type                           | Description                                                                                                    |
|-------------------------------------|--------------------------------|----------------------------------------------------------------------------------------------------------------|
| `remoteLoggingConfig`               | `RemoteLoggingConfig`          | Gateway configuration (URL, channel ID, log levels, truncation).                                               |
| `deviceDataGenerator`               | `DeviceDataGenerator?`         | Optional custom generator. Defaults to `FlutterDeviceDataGenerator()`.                                         |
| `attestationProvider`               | `AttestationProvider?`         | Hardware attestation strategy. Defaults to `const PlatformAttestationProvider()`. Use `FakeAttestationProvider()` in simulators/tests. |
| `getAttestationTokenCallback`       | `GetAttestationTokenCallback?` | Optional legacy fallback callback used only when `attestationProvider.isSupported()` returns `false`.          |
| `localLogRepository`                | `LocalLogRepository?`          | Optional custom storage repository. Defaults to `SqfliteLocalLogRepository()`.                                 |
| `enableDefaultLocalFallbackStorage` | `bool`                         | Set to `false` to disable SQLite local fallback storage when `localLogRepository` is null. Defaults to `true`. |
| `onLog`                             | `OnLog?`                       | Callback invoked when a log record is processed.                                                               |
| `onError`                           | `OnError?`                     | Callback invoked on internal logging failures.                                                                 |

---

### Local-Only Logging Mode

For testing or offline environments where remote transmission is not desired:

```dart
final localLogger = await FlutterRemoteLogging.initLocalOnly(
  localLoggingConfig: const LocalLoggingConfig(),
  onLog: (record) => print('Local log: ${record.message}'),
);
```

---

## Custom Implementation Hooks

### Custom Device Data Generator

You can provide your own `DeviceDataGenerator` implementation if you need custom device metadata or custom device ID resolution:

```dart
class CustomDeviceDataGenerator implements DeviceDataGenerator {
  @override
  Future<void> init() async { /* Custom init */ }

  @override
  String get deviceId => 'custom-device-id';

  @override
  String get sessionId => 'custom-session-id';

  @override
  DeviceMetadata get deviceMetadata => const DeviceMetadata(
    timestamp: '...',
    deviceId: 'custom-device-id',
    sessionId: 'custom-session-id',
    os: 'CustomOS',
    osVersion: '1.0',
    model: 'ModelX',
    brand: 'BrandY',
    device: 'DeviceZ',
    identifier: 'com.example.app',
    manufacturer: 'Vendor',
    isPhysicalDevice: 'true',
  );
}

// Pass to initRemote:
await FlutterRemoteLogging.initRemote(
  remoteLoggingConfig: config,
  deviceDataGenerator: CustomDeviceDataGenerator(),
);
```

---

## Hardware Attestation & Host App Setup Requirements

`flutter_remote_logging` provides stateless hardware attestation via `PlatformAttestationProvider` (default) for Android Keystore Key Attestation and Apple DeviceCheck App Attest.

### iOS Setup Requirements
1. **Deployment Target:** iOS 14.0 or higher.
2. **App Attest Capability:**
   - In Xcode, go to **Signing & Capabilities** -> click `+ Capability` -> add **App Attest**.
3. **App Attest Environment Entitlement:**
   - In `ios/Runner/Runner.entitlements`:
     ```xml
     <key>com.apple.developer.devicecheck.appattest-environment</key>
     <string>development</string>
     ```
   - For App Store / TestFlight release builds, set this value to `production`.

### Android Setup Requirements
1. **Minimum SDK:** Android 7.0 (API level 24) or higher. StrongBox hardware backing is supported on API level 28+.
2. **Signing Certificate Fingerprints:**
   - Register your release keystore and Google Play App Signing SHA-256 certificate fingerprints in your channel configuration (`android_signing_cert_sha256`).

### Running in Tests, CI, or Emulators
When running unit tests, CI pipelines, or simulators where hardware-backed keys are unavailable:
```dart
await FlutterRemoteLogging.initRemote(
  remoteLoggingConfig: config,
  attestationProvider: FakeAttestationProvider(),
);
```

---

## Testing

Run package tests using standard Flutter test runner:

```bash
flutter test
```
