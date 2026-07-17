# syntax=docker/dockerfile:1
# Build stage
FROM rust:bookworm as builder

# Install cmake for rdkafka's cmake-build feature
RUN apt-get update && apt-get install -y cmake

WORKDIR /usr/src/logger_gateway

# Copy over manifests and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build for release with BuildKit cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=gateway_registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=gateway_git \
    --mount=type=cache,target=/usr/src/logger_gateway/target,id=gateway_target \
    cargo build --release && \
    cp target/release/logger_gateway /usr/local/bin/logger_gateway

# Production stage
FROM debian:bookworm-slim

# Install CA certificates to enable outgoing HTTPS requests
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

# Create a non-privileged group and user
RUN groupadd -r appgroup && useradd -r -g appgroup -s /sbin/nologin appuser

WORKDIR /app

# Copy the binary from the build context with user/group ownership
COPY --from=builder --chown=appuser:appgroup /usr/local/bin/logger_gateway /app/logger_gateway

# Expose the proxy listener port (8080) and Prometheus metrics port (9090)
EXPOSE 8080 9090

# Define environment variables with empty default values for documentation
ENV RUST_LOG="info"

# Switch to the non-privileged user
USER appuser

# Set entrypoint to run the gateway binary
ENTRYPOINT ["/app/logger_gateway"]
