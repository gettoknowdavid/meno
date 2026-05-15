# ==================== BUILD STAGE ====================
FROM rust:1.88-slim-bookworm AS builder

# Install system dependencies for SQLx and OpenSSL
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency manifests first to leverage Docker cache
COPY Cargo.toml Cargo.lock ./
COPY apps/api/Cargo.toml ./apps/api/Cargo.toml

# Create a dummy main.rs to compile dependencies first (speeds up rebuilds)
RUN mkdir -p apps/api/src && \
    echo "fn main() { println!(\"dummy\"); }" > apps/api/src/main.rs

# Initial build to cache dependencies. The backslash ensures the command continues.
RUN cargo build --release --package meno-api || \
    true

# Remove dummy file and copy the actual source code
RUN rm -f apps/api/src/main.rs
COPY apps/api/src ./apps/api/src

# Copy your existing .sqlx folder for offline macro verification
COPY .sqlx ./.sqlx

# Enable SQLx offline mode to avoid needing a live DB during build
ENV SQLX_OFFLINE=true

# Final compilation of the actual binary
RUN cargo build --release --package meno-api

# ==================== RUNTIME STAGE ====================
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/meno-api /app/meno-api

# Expose the API port
EXPOSE 8080

# Run the application
CMD ["./meno-api"]