# Build stage
FROM rust:1.76-bookworm as builder

# Install cmake for rdkafka's cmake-build feature
RUN apt-get update && apt-get install -y cmake

WORKDIR /usr/src/validation_worker

# Copy over manifests and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build for release
RUN cargo build --release

# Production stage
FROM debian:bookworm-slim

# Install ca-certificates and other required minimal runtime libraries
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy the compiled binary from the builder environment
COPY --from=builder /usr/src/validation_worker/target/release/validation_worker /usr/local/bin/validation_worker

# Expose the application port
EXPOSE 3000

# Start the worker
CMD ["validation_worker"]
