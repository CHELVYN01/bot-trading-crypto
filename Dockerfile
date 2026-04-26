# Tahap 1: Rust 1.88.0 - Wajib untuk dependensi terbaru
FROM lukemathwalker/cargo-chef:latest-rust-1.88.0-slim-bookworm AS chef
WORKDIR /app

# Tahap 2: Planner
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Tahap 3: Builder
FROM chef AS builder

# Install build dependencies: pkg-config & libssl-dev untuk crate openssl-sys
RUN apt-get update -y && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json

# Build dependensi (Layer ini ter-cache jika Cargo.toml tidak berubah)
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code dan build aplikasi utama
COPY . .
RUN cargo build --release

# Tahap 4: Runtime
FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update -y && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Copy binary dari builder
COPY --from=builder /app/target/release/trading_bot /usr/local/bin/crypto-bot

ENV RUST_LOG="info"
ENV TOKIO_WORKER_THREADS="2"

CMD ["crypto-bot"]
