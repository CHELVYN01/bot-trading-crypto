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
    // utils::logger::init(); // (Akan diimplementasikan nanti)
    tracing_subscriber::fmt::init();

    tracing::info!("Mulai inisialisasi HFT Crypto Trading Bot...");

    // TODO: Inisialisasi koneksi Binance WSS dan sistem Telegram
    // TODO: Jalankan Event Loop Utama (engine::runner)

    tracing::info!("Bot siap beroperasi.");
    
    // Menahan main thread agar tidak exit
    tokio::signal::ctrl_c().await?;
    tracing::info!("Menerima sinyal terminasi. Mematikan bot dengan aman...");
    
    Ok(())
}
