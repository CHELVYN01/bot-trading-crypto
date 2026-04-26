use dotenvy::dotenv;
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod notification;
pub mod broker;
pub mod engine;
pub mod strategy;
pub mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Muat variabel lingkungan dari file .env
    dotenv().ok();

    // 2. Inisialisasi sistem Logging ke terminal
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Mulai inisialisasi HFT Crypto Trading Bot...");
    
    // 3. Setup Papan Tulis Memori (Shared State) - Menghubungkan Telegram & Engine
    let shared_state = Arc::new(RwLock::new(engine::state::BotState::default()));

    // 4. Jalankan Telegram Bot di Background (Thread B)
    let state_for_telegram = Arc::clone(&shared_state);
    tokio::spawn(async move {
        if let Err(e) = notification::telegram::run_telegram_bot(state_for_telegram).await {
            tracing::error!("Telegram Bot terhenti secara fatal: {:?}", e);
        }
    });

    tracing::info!("Bot Telegram Service siap beroperasi. Silakan kirim /start di Telegram.");
    
    // 5. Jalankan Trading Engine di Background (Koneksi ke Tokocrypto WebSocket) (Thread A)
    let state_for_engine = Arc::clone(&shared_state);
    tokio::spawn(async move {
        if let Err(e) = engine::runner::start_engine(state_for_engine).await {
            tracing::error!("Trading Engine terhenti secara fatal: {:?}", e);
        }
    });
    
    // Menahan main thread agar aplikasi tidak langsung mati setelah inisialisasi
    tokio::signal::ctrl_c().await?;
    tracing::info!("Menerima sinyal terminasi (Ctrl+C). Mematikan bot dengan aman...");
    
    Ok(())
}
