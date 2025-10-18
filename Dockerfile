# Multi-stage build for efficient image size
FROM rust:1.89-bookworm AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    cmake \
    g++ \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a new empty shell project
WORKDIR /usr/src

# Copy workspace configuration files first
COPY Cargo.toml Cargo.lock ./
COPY rust-toolchain.toml ./

# Copy all crate directories with their Cargo.toml files
COPY crates/ ./crates/

# Build dependencies first (this layer will be cached)
RUN cargo build --release --workspace || true

# Copy the rest of the source code
COPY . .

# Build the actual application
RUN cargo build --release --bin forge

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libsqlite3-0 \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 forge && \
    mkdir -p /home/forge/.config/forge /home/forge/.cache/forge /home/forge/workspace

# Copy the binary from builder
COPY --from=builder /usr/src/target/release/forge /usr/local/bin/forge

# Copy default configuration files
COPY --from=builder /usr/src/forge.default.yaml /home/forge/.config/forge/
COPY --from=builder /usr/src/templates/ /home/forge/.config/forge/templates/
# Fix the permissions file path
COPY --from=builder /usr/src/crates/forge_services/src/permissions.default.yaml /home/forge/.config/forge/

# Set ownership
RUN chown -R forge:forge /home/forge

# Switch to non-root user
USER forge
WORKDIR /home/forge/workspace

# Set environment variables
ENV FORGE_CONFIG_DIR=/home/forge/.config/forge
ENV FORGE_CACHE_DIR=/home/forge/.cache/forge
ENV FORGE_HISTORY_FILE=/home/forge/.cache/forge/history

# Expose any necessary ports (adjust if needed)
# EXPOSE 8080

# Set the entrypoint
ENTRYPOINT ["forge"]
CMD ["--help"]