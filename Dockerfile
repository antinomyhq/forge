# Use the official Rust image as the base image
FROM rust:1.75-slim as builder

# Set the working directory
WORKDIR /app

# Copy the entire project
COPY . .

# Build the application in release mode
RUN cargo build --release --bin forge

# Use a minimal runtime image
FROM debian:bookworm-slim

# Install necessary runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 forge

# Set the working directory
WORKDIR /app

# Copy the built binary from the builder stage
COPY --from=builder /app/target/release/forge /usr/local/bin/forge

# Create necessary directories and set permissions
RUN mkdir -p /app/config /app/workspace && \
    chown -R forge:forge /app

# Switch to the non-root user
USER forge

# Set environment variables
ENV FORGE_CONFIG_DIR=/app/config
ENV FORGE_WORKSPACE_DIR=/app/workspace

# Expose any necessary ports (if needed for future features)
# EXPOSE 8080

# Set the entrypoint to the forge binary
ENTRYPOINT ["forge"]

# Default command (can be overridden)
CMD ["--help"]
