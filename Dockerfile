# Build Stage
FROM rust:latest as builder

WORKDIR /app

# Copy manifests first for caching
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm src/main.rs

# Copy source code
COPY . .

# Build actual application
RUN touch src/main.rs
RUN cargo build --release

# Runtime Stage
FROM debian:bookworm-slim

WORKDIR /app

# Install SSL certificates (needed for HTTPS requests)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/htmlwordpress-api /usr/local/bin/htmlwordpress-api

# Environment variables
ENV RUST_LOG=info
ENV PORT=3000

# Expose port (Railway overrides PORT env var)
EXPOSE 3000

# Run the binary
CMD ["htmlwordpress-api"]
