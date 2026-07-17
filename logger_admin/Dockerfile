# syntax=docker/dockerfile:1
# Build stage
FROM rust:bookworm AS builder

WORKDIR /usr/src/logger_admin

# Copy over manifests and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src
# Embed static assets into the binary at build time
COPY static ./static

# Build for release with BuildKit cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=admin_registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=admin_git \
    --mount=type=cache,target=/usr/src/logger_admin/target,id=admin_target \
    cargo build --release && \
    cp target/release/logger_admin /usr/local/bin/logger_admin

# Production stage
FROM debian:bookworm-slim

# Install CA certificates to enable outgoing HTTPS requests to Consul
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

# Create a non-privileged group and user
RUN groupadd -r appgroup && useradd -r -g appgroup -s /sbin/nologin appuser

WORKDIR /app

# Copy the binary — static assets are embedded inside, no volume needed
COPY --from=builder --chown=appuser:appgroup /usr/local/bin/logger_admin /app/logger_admin

# Expose the admin UI port
EXPOSE 8082

ENV RUST_LOG="info"

USER appuser

ENTRYPOINT ["/app/logger_admin"]
