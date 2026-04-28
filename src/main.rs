use dotenvy::dotenv;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

pub mod notification;
pub mod broker;
pub mod engine;
pub mod strategy;
pub mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Muat variabel lingkungan dari file .env
    dotenv().ok();

    // 2. Inisialisasi sistem Logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("╔══════════════════════════════════════╗");
    tracing::info!("║   HFT Crypto Trading Bot — Rust      ║");
    tracing::info!("║   Exchange: Tokocrypto (BTCBIDR)     ║");
    tracing::info!("║   Modal: Rp 500.000                  ║");
    tracing::info!("╚══════════════════════════════════════╝");

    // 3. Papan Tulis Memori Bersama (Shared State)
    //    Digunakan Telegram untuk baca data pasar real-time
    let shared_state = Arc::new(RwLock::new(engine::state::BotState::default()));

    // ============================================================
    // PIPELINE CHANNEL SETUP
    // Mendefinisikan "pipa komunikasi" antar 4 task
    // ============================================================

    // Channel A: The Listener → The Guardian (sinyal entry/exit)
    // Buffer 10: jika Guardian sibuk, sinyal lama akan dibuang (bukan di-queue lama)
    let (tx_signal, rx_signal) = mpsc::channel::<strategy::signal::TradeSignal>(10);

    // Channel B: The Guardian → The Messenger (laporan trade)
    // Buffer 20: laporan bisa sedikit tertunda sebelum dikirim ke Telegram
    let (tx_report, rx_report) = mpsc::channel::<strategy::signal::TradeReport>(20);

    tracing::info!("📡 Pipeline MPSC channel siap.");

    // ============================================================
    // SPAWN 4 TASK KONKUREN
    // ============================================================

    // Task 4: The Messenger (Telegram)
    // Dijalankan pertama agar siap menerima laporan dari Task 3
    let state_for_telegram = Arc::clone(&shared_state);
    tokio::spawn(async move {
        if let Err(e) = notification::telegram::run_telegram_bot(state_for_telegram, rx_report).await {
            tracing::error!("❌ [MESSENGER] Terhenti secara fatal: {:?}", e);
        }
    });
    tracing::info!("📨 Task 4 (The Messenger) aktif.");

    // Task 3: The Guardian (Risk Manager)
    let state_for_guardian = Arc::clone(&shared_state);
    tokio::spawn(async move {
        if let Err(e) = engine::guardian::run_guardian(rx_signal, tx_report, state_for_guardian).await {
            tracing::error!("❌ [GUARDIAN] Terhenti secara fatal: {:?}", e);
        }
    });
    tracing::info!("🛡️ Task 3 (The Guardian) aktif.");

    // Task 1 + 2: The Listener (WebSocket) + The Strategist (Scanner)
    // Runner mengelola keduanya secara bersamaan dengan auto-reconnect
    let state_for_engine = Arc::clone(&shared_state);
    tokio::spawn(async move {
        if let Err(e) = engine::runner::start_engine(state_for_engine, tx_signal).await {
            tracing::error!("❌ [ENGINE] Terhenti secara fatal: {:?}", e);
        }
    });
    tracing::info!("👂 Task 1+2 (The Listener + The Strategist) aktif.");

    tracing::info!("✅ Semua task berjalan. Bot siap beroperasi!");
    tracing::info!("   Kirim /start di Telegram untuk memulai.\n");

    // Menahan main thread — bot berjalan sampai ada sinyal terminasi
    tokio::signal::ctrl_c().await?;
    tracing::info!("🛑 Menerima sinyal terminasi. Mematikan bot dengan aman...");

    Ok(())
}
