# Tahap 1: Caching dependensi dengan cargo-chef
FROM rust:1.77-slim-bookworm AS chef
USER root
RUN cargo install cargo-chef
WORKDIR /app

# Tahap 2: Menghitung "resep" dependensi (hanya Cargo.toml & Cargo.lock)
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Tahap 3: Build (Cook) dependensi dan aplikasi utama
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependensi. Layer ini akan di-cache selama Cargo.toml/lock tidak berubah.
# Ini sangat menghemat waktu build di CI/CD.
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code bot trading
COPY . .
# Asumsi nama binary di Cargo.toml adalah `trading_bot`
# Silakan sesuaikan nama ini jika berbeda
RUN cargo build --release

# Tahap 4: Runtime Image (Sangat ringan dan aman)
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Install root sertifikat SSL yang dibutuhkan untuk koneksi tokio-tungstenite (Binance WS) dan Teloxide
RUN apt-get update -y && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Pindahkan binary bot dari tahap builder.
# Ganti `trading_bot` sesuai dengan nama binary di project Anda.
COPY --from=builder /app/target/release/trading_bot /usr/local/bin/crypto-bot

# Set environment variabel untuk optimasi Tokio dan logging
ENV RUST_LOG="info"
ENV TOKIO_WORKER_THREADS="2" 

# Jalankan bot
CMD ["crypto-bot"]
