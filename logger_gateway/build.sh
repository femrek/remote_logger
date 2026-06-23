#!/usr/bin/env bash
# build.sh - Build logger_gateway locally and create a Docker image.

set -euo pipefail

# Configurations
IMAGE_NAME="logger-gateway"
IMAGE_TAG="latest"
DEFAULT_TARGET="aarch64-unknown-linux-gnu" # Match Mac's ARM64 architecture for local Docker execution

# Determine target architecture
TARGET_ARCH="${1:-$DEFAULT_TARGET}"

echo "=== Step 1: Compiling locally ==="
OS="$(uname -s)"

if [ "$OS" = "Darwin" ]; then
    echo "Detected macOS. To build a Linux binary locally, we will use 'cargo zigbuild'."
    
    # Check for zig
    if ! command -v zig >/dev/null 2>&1; then
        echo "Error: 'zig' is not installed. Please install it using: brew install zig"
        exit 1
    fi
    
    # Check for cargo-zigbuild
    if ! cargo zigbuild --version >/dev/null 2>&1; then
        echo "Installing cargo-zigbuild..."
        cargo install cargo-zigbuild
    fi
    
    # Add target if not present
    echo "Ensuring rustup target '$TARGET_ARCH' is installed..."
    rustup target add "$TARGET_ARCH"
    
    echo "Running: cargo zigbuild --release --target $TARGET_ARCH"
    cargo zigbuild --release --target "$TARGET_ARCH"
    BINARY_PATH="target/$TARGET_ARCH/release/logger_gateway"
else
    echo "Detected native Linux (or other OS). Compiling natively..."
    cargo build --release
    BINARY_PATH="target/release/logger_gateway"
fi

echo "=== Step 2: Preparing Docker Staging Context ==="
STAGING_DIR=".docker-build"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/config"

cp "$BINARY_PATH" "$STAGING_DIR/logger_gateway"
cp -r config/* "$STAGING_DIR/config/"
cp docker/Dockerfile "$STAGING_DIR/Dockerfile"

echo "=== Step 3: Building Docker Image ==="
docker build -t "${IMAGE_NAME}:${IMAGE_TAG}" -f "$STAGING_DIR/Dockerfile" "$STAGING_DIR"

echo "=== Success ==="
echo "Docker image '${IMAGE_NAME}:${IMAGE_TAG}' built successfully."
echo "You can run it with docker-compose or:"
echo "  docker run -p 8080:8080 -p 9090:9090 -e GATEWAY_JWT_SECRET=my-secret ${IMAGE_NAME}:${IMAGE_TAG}"
