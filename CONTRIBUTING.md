# Contributing to Remote Logger

Thank you for your interest in contributing to **Remote Logger**! We welcome contributions, bug reports, documentation enhancements, and feature suggestions.

---

## Development Workflow

### Prerequisites

- **Rust** 1.82+ (`rustup update stable`)
- **Docker & Docker Compose v2**
- **CMake** (required when compiling `rdkafka` from source)
- **Dart 3.10+ & Flutter 3.x** (for mobile SDKs)

### Repository Layout

- `logger_gateway/`: Pingora-based reverse proxy (Rust binary crate)
- `validation_worker/`: Axum schema validator and Kafka producer (Rust library & binary)
- `logger_admin/`: Admin dashboard and Consul KV configuration management (Rust crate)
- `mobile/remote_logging/`: Platform-agnostic Dart logging client
- `mobile/flutter_remote_logging/`: Flutter-specific logging extension with SQLite cache
- `load_tests/`: k6 and bash load testing suites
- `e2e/`: End-to-end integration test crate

---

## Running Tests

Before submitting a Pull Request, verify that all test suites pass.

### Rust Workspace Tests

```bash
# Run unit tests across all Rust crates (fast, in-process)
cargo test --workspace --lib

# Run validation_worker integration tests (requires Docker)
cd validation_worker
cargo test --test integration_test

# Run e2e tests (requires Docker)
cd ../e2e
cargo test --test e2e_test
```

### Mobile SDK Tests

```bash
# Dart package
cd mobile/remote_logging
dart test

# Flutter package
cd ../flutter_remote_logging
flutter test
```

### Code Formatting & Linting

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Dart & Flutter
cd mobile/remote_logging
dart format --output=none --set-exit-if-changed .
dart analyze --fatal-infos

cd ../flutter_remote_logging
dart format --output=none --set-exit-if-changed .
flutter analyze --fatal-infos
```

---

## Pull Request Guidelines

1. **Branch Naming**: Use descriptive prefixes like `feat/`, `fix/`, `docs/`, `chore/`.
2. **Commit Messages**: Follow [Conventional Commits](https://www.conventionalcommits.org/) (e.g. `feat(gateway): add health check probe`, `fix(mobile): update flush interval`).
3. **Documentation**: If your change modifies configuration options, routes, or models, update the corresponding `README.md` or OpenAPI documentation.
4. **Clean Git History**: Rebase against the latest `main` branch before opening your PR.

---

## Reporting Issues

- Use the GitHub Issue Tracker to report bugs or request features.
- For security vulnerabilities, **do not open a public issue** — report them privately to `faruk.eksioglu@hotmail.com`.
- Provide step-by-step reproduction instructions, logs (`RUST_LOG=debug`), and environment details (OS, Docker version).
