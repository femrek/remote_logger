FROM debian:bookworm-slim

# Install CA certificates to enable outgoing HTTPS requests
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary and configuration files from the build context
COPY logger_gateway /app/logger_gateway
COPY config/ /app/config/

# Expose the proxy listener port (8080) and Prometheus metrics port (9090)
EXPOSE 8080 9090

# Define environment variables with empty default values for documentation
ENV RUST_LOG="info"

# Set entrypoint to run the gateway binary
ENTRYPOINT ["/app/logger_gateway"]
CMD ["--config", "/app/config/gateway.yaml"]
