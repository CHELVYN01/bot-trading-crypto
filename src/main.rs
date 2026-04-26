pub mod engine;
pub mod strategy;
pub mod broker;
pub mod notification;
pub mod utils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Memuat konfigurasi dari .env
    dotenvy::dotenv().ok();

    // 2. Setup Tracing / Logger
    tracing_subscriber::fmt::init();

    tracing::info!("Mulai inisialisasi HFT Crypto Trading Bot...");

    // 3. Jalankan Telegram Bot di Background (Spawn Thread Async)
    // Kita jalankan di thread terpisah agar tidak memblokir Event Loop WebSocket nanti
    tokio::spawn(async move {
        if let Err(e) = notification::telegram::run_telegram_bot().await {
            tracing::error!("Telegram Bot Service berhenti dengan error fatal: {:?}", e);
        }
    });

    tracing::info!("Bot Telegram Service siap beroperasi. Silakan kirim /start di Telegram.");
    
    // 4. Jalankan Trading Engine di Background (Koneksi ke Tokocrypto WebSocket)
    tokio::spawn(async move {
        if let Err(e) = engine::runner::start_engine().await {
            tracing::error!("Trading Engine terhenti secara fatal: {:?}", e);
        }
    });
    
    // Menahan main thread agar aplikasi tidak langsung mati setelah inisialisasi
    tokio::signal::ctrl_c().await?;
    tracing::info!("Menerima sinyal terminasi (Ctrl+C). Mematikan bot dengan aman...");
    
    Ok(())
}
