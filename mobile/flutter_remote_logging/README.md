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
- 🔒 **App Attestation Ready**: Supports dynamic attestation token callbacks (e.g. Firebase App Check, Play Integrity,
  App Attest) for secure backend endpoint verification.
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

### 1. Initialize Remote Logging

Initialize `FlutterRemoteLogging` early in your application startup (e.g., inside `main()`):

```dart
import 'package:flutter/material.dart';
import 'package:flutter_remote_logging/flutter_remote_logging.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Initialize Remote Logging for Flutter
  final remoteLogger = await FlutterRemoteLogging.initRemote(
    remoteLoggingConfig: const RemoteLoggingConfig(
      gatewayUrl: 'https://logger.example.com',
      channelId: 'mobile-app-prod',
    ),
    getAttestationTokenCallback: () async {
      // Return dynamic attestation token if your backend requires it
      return 'your-attestation-jwt-or-token';
    },
    onLog: (log) {
      debugPrint('[LOG] ${log.level}: ${log.message}');
    },
    onError: (error, stackTrace) {
      debugPrint('[LOGGING ERROR] $error');
    },
  );

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
 └──────────────┬───────────────────────────────┬──────────────┘
                │ Success                       │ Offline / Retries
                ▼                               ▼
 ┌──────────────────────────────┐ ┌───────────────────────────┐
 │ Remote Logger Gateway Backend│ │ SqfliteLocalLogRepository │
 └──────────────────────────────┘ └───────────────────────────┘
```

When calling `FlutterRemoteLogging.initRemote(...)`, the library automatically:
1. Instantiates a `FlutterDeviceDataGenerator` which fetches system information via `device_info_plus` & `package_info_plus`, and retrieves or generates a persistent device ID via `shared_preferences`.
2. Initializes `SqfliteLocalLogRepository` to back up unsent logs to a local SQLite database (`remote_logs.db`).
3. Bridges log entries directly into the `remote_logging` transmission engine.

---

## API Reference

### `FlutterRemoteLogging.initRemote`

```dart
static Future<RemoteLogging> initRemote({
  required RemoteLoggingConfig remoteLoggingConfig,
  DeviceDataGenerator? deviceDataGenerator,
  GetAttestationTokenCallback? getAttestationTokenCallback,
  LocalLogRepository? localLogRepository,
  bool enableDefaultLocalFallbackStorage = true,
  OnLog? onLog,
  OnError? onError,
})
```

| Parameter                           | Type                           | Description                                                                                                    |
|-------------------------------------|--------------------------------|----------------------------------------------------------------------------------------------------------------|
| `remoteLoggingConfig`               | `RemoteLoggingConfig`          | Gateway configuration (URL, channel ID, batch size, flush interval).                                           |
| `deviceDataGenerator`               | `DeviceDataGenerator?`         | Optional custom generator. Defaults to `FlutterDeviceDataGenerator()`.                                         |
| `getAttestationTokenCallback`       | `GetAttestationTokenCallback?` | Optional async callback returning an attestation token for gateway authentication.                             |
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

## Testing

Run package tests using standard Flutter test runner:

```bash
flutter test
```
