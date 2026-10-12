---
name: Bug Report
about: Create a report to help us improve Remote Logger
title: "[BUG] "
labels: bug
assignees: ''
---

**Describe the bug**
A clear and concise description of what the bug is.

**Component Affected**
Which component is affected?
- [ ] `logger_gateway` (Pingora proxy)
- [ ] `validation_worker` (Axum ingest / auth)
- [ ] `logger_admin` (Admin dashboard & API)
- [ ] `mobile/remote_logging` (Dart package)
- [ ] `mobile/flutter_remote_logging` (Flutter package)
- [ ] Deployment / Infrastructure (Docker / Kubernetes / Consul)

**To Reproduce**
Steps to reproduce the behavior:
1. Start stack with `...`
2. Send request to endpoint `...` with payload `...`
3. See error response `...`

**Expected behavior**
A clear and concise description of what you expected to happen.

**Logs & Traces**
If applicable, add terminal output or logs with `RUST_LOG=debug`:
```text
paste logs here
```

**Environment (please complete the following information):**
 - OS: [e.g. macOS 14, Ubuntu 22.04]
 - Rust version: [e.g. 1.82.0]
 - Docker / Compose version: [e.g. 24.0.5 / v2.20]
 - Flutter / Dart version (if mobile issue): [e.g. Flutter 3.22, Dart 3.4]

**Additional context**
Add any other context about the problem here.
