# syntax=docker/dockerfile:1
# Build stage
FROM rust:bookworm as builder

# Install cmake for rdkafka's cmake-build feature
RUN apt-get update && apt-get install -y cmake

WORKDIR /usr/src/validation_worker

# Copy over manifests and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build for release with BuildKit cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=worker_registry \
    --mount=type=cache,target=/usr/local/cargo/git,id=worker_git \
    --mount=type=cache,target=/usr/src/validation_worker/target,id=worker_target \
    cargo build --release && \
    cp target/release/validation_worker /usr/local/bin/validation_worker

# Production stage
FROM debian:bookworm-slim

# Install ca-certificates and other required minimal runtime libraries
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Create a non-privileged group and user
RUN groupadd -r appgroup && useradd -r -g appgroup -s /sbin/nologin appuser

# Copy the compiled binary from the builder environment with user/group ownership
COPY --from=builder --chown=appuser:appgroup /usr/local/bin/validation_worker /usr/local/bin/validation_worker

# Expose the application port
EXPOSE 3000

# Switch to the non-privileged user
USER appuser

# Start the worker
CMD ["validation_worker"]
