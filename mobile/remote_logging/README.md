# Remote Logging

A pure Dart logging library designed to transmit structured log events to a self-hosted Remote Logger backend pipeline.

Built on top of the standard Dart [`package:logging`](https://pub.dev/packages/logging), it intercepts log records, validates and truncates payloads safely at the source, and batches them asynchronously over HTTP with automatic retry handling.

> For Flutter applications, use [`flutter_remote_logging`](../flutter_remote_logging) which includes automatic hardware metadata collection and SQLite offline log caching out-of-the-box.

---

## Features

- 🚀 **Standard Logging API**: Re-exports and integrates with `package:logging`. Use familiar `Logger` instances and log levels.
- 📦 **Batch Ingestion**: Queues records and flushes them in batches over HTTP to minimize mobile network radio wakeups.
- 🛡️ **Defensive Truncation**: Configurable bounds on message lengths, error descriptions, and stack trace frames (`LogTruncationConfig`) to prevent oversized payloads.
- 🔌 **Pluggable Strategies**: Customize how device metadata is generated (`DeviceDataGenerator`), how hardware attestation is performed (`AttestationProvider`), and how logs are cached locally during outages (`LocalLogRepository`).
- 🔒 **Two-Tier Attestation & Session Proof**: Supports hardware key enrollment (`/auth/challenge` → `/auth/enroll` → `/auth/exchange`) via `AttestationProvider`, as well as direct token exchange via `getAttestationTokenCallback`.
- 📴 **Local-Only Mode**: Toggle between remote network transmission and local console formatting without rewriting logging calls.

---

## Installation

Add `remote_logging` to your `pubspec.yaml`:

```yaml
dependencies:
  remote_logging: ^0.0.5
```

Or install with `dart pub`:

```bash
dart pub add remote_logging
```

---

## Quick Start

### 1. Initialize & Start Remote Logging

Call `RemoteLoggingInit.initRemote` and start the sender loop with an `INetworkInvoker`:

```dart
import 'package:dart_network_layer_dio/dart_network_layer_dio.dart';
import 'package:remote_logging/remote_logging.dart';

void main() async {
  const gatewayUrl = 'https://logger.example.com';

  final remoteLogger = await RemoteLoggingInit.initRemote(
    config: const RemoteLoggingConfig(
      gatewayUrl: gatewayUrl,
      channelId: 'example-app-dev',
      logSendLevel: Level.INFO,
      logPrintLevel: Level.ALL,
    ),
    deviceDataGenerator: AnonymousDeviceDataGenerator(),
    attestationProvider: const NoAttestationProvider(),
    onLog: (record) {
      // Optional callback for custom monitoring or debugging
    },
    onError: (error, stackTrace) {
      // Invoked on internal logging pipeline errors
    },
  );

  // Start the background transmission loop & initial auth exchange
  final invoker = DioNetworkInvoker.fromBaseUrl(gatewayUrl);
  remoteLogger.start(invoker);

  // Obtain a logger and log records normally
  final logger = Logger('DatabaseService');
  logger.info('Database connection established.');

  // Flush & stop pipeline gracefully when shutting down
  await remoteLogger.stop();
}
```

### 2. Local-Only Mode

For offline testing or CLI tools where remote transmission is not required:

```dart
import 'package:remote_logging/remote_logging.dart';

void main() {
  final localLogger = RemoteLoggingInit.initLocal(
    config: const LocalLoggingConfig(
      logPrintLevel: Level.INFO,
    ),
    onLog: (record) {
      print('[LOCAL] ${record.level.name}: ${record.message}');
    },
  );

  final logger = Logger('App');
  logger.info('Running in local mode');
}
```

---

## Configuration Reference

### `RemoteLoggingConfig`

| Property | Type | Default | Description |
|---|---|---|---|
| `gatewayUrl` | `String` | **Required** | Base HTTP URL of the gateway (e.g. `https://logger.example.com`). Do not append `/v1/logs`. |
| `channelId` | `String` | **Required** | Target routing channel ID registered on the backend. |
| `logSendLevel` | `Level` | `Level.INFO` | Minimum severity level sent to the remote gateway. |
| `logPrintLevel` | `Level` | `Level.FINEST` | Minimum severity level printed to the local console. |
| `enableRemoteLogging` | `bool` | `true` | When `false`, remote HTTP transmission is skipped. |
| `truncation` | `LogTruncationConfig` | `LogTruncationConfig()` | Maximum length limits for message text, error strings, and stack traces. |
| `maxPendingLogCount` | `int` | `500` | Maximum failed-retry logs retained in memory before FIFO eviction. |

---

## Custom Strategies

### Custom `DeviceDataGenerator`

Implement `DeviceDataGenerator` to supply client-specific device identifiers and hardware specs:

```dart
class MyDeviceDataGenerator implements DeviceDataGenerator {
  @override
  Future<void> init() async {}

  @override
  String get deviceId => 'custom-device-id';

  @override
  String get sessionId => 'custom-session-uuid';

  @override
  DeviceMetadata get deviceMetadata => DeviceMetadata(
    timestamp: DateTime.now().toUtc().toIso8601String(),
    deviceId: deviceId,
    sessionId: sessionId,
    os: 'Linux',
    osVersion: '6.5',
    model: 'ServerNode',
    brand: 'Custom',
    device: 'desktop',
    identifier: 'com.example.app',
    manufacturer: 'Custom',
    isPhysicalDevice: 'true',
  );
}
```

### Custom `LocalLogRepository`

Implement `LocalLogRepository` to persist unsent logs to disk, database, or key-value storage:

```dart
class MyDiskLogRepository implements LocalLogRepository {
  @override
  Future<void> save(List<ServerLog> logs) async {
    // Write unsent logs to disk
  }

  @override
  Future<List<ServerLog>> getPendingLogs() async {
    // Read pending logs from disk
    return [];
  }

  @override
  Future<void> delete(List<ServerLog> logs) async {
    // Remove successfully sent logs
  }
}
```

---

## License

Distributed under the MIT License. See [LICENSE](../../LICENSE) for details.
