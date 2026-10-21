# Remote Logging Example & Diagnostic App (`mobile/example`)

An interactive Flutter diagnostic application for testing **Remote Logger** ingestion, offline SQLite queuing, and end-to-end hardware attestation (**Android Keystore Key Attestation** & **Apple DeviceCheck App Attest / Secure Enclave**).

---

## Features & Tabs

1. **Logger Tab**:
   - Configure the target `gatewayUrl` and `channelId` (`example-app-dev`, `example-app-prod`, or custom).
   - Toggle between `PlatformAttestationProvider` (real hardware attestation) and `NoAttestationProvider` (direct/unattested mode for simulators & emulators).
   - Send single logs, structured error/stacktrace logs, and burst batches, and inspect SQLite offline queue behavior.
2. **Attestation Tab**:
   - Step-by-step interactive hardware attestation inspector:
     - Check hardware support (`isSupported()`).
     - Fetch a stateless challenge (`GET /auth/challenge`).
     - Generate & attest a hardware P-256 key (`enroll(challenge)` $\rightarrow$ `POST /auth/enroll`).
     - Sign a session challenge (`prove(challenge, sessionId, deviceId)` $\rightarrow$ `POST /auth/exchange`).
     - Reset local hardware keys and cached enrollment tokens (`reset()`).
3. **Diagnostics Tab**:
   - Automated end-to-end diagnostic suite verifying gateway connectivity, challenge issuance, hardware key enrollment, session token exchange, and log ingestion.

---

## Running the App

1. Ensure the backend stack is running (see the root [`README.md`](../../README.md)).
2. Launch on a physical Android/iOS device (recommended for hardware attestation) or simulator/emulator:

```bash
cd mobile/example
flutter run
```

> **Note:** When testing on a physical device against a local backend, set the Gateway URL in the app to your host machine's LAN IP (e.g., `http://192.168.1.x:8080`) and ensure the channel's `app_id` in Logger Admin UI (`http://localhost:8082`) matches `dev.femrek.remote_logging_example`.

