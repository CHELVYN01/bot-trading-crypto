use tracing::{error, info, warn};
use super::websocket;
use super::state::SharedState;
use crate::broker::{api, db};
use crate::strategy::scanner::Scanner;

pub async fn start_engine(state: SharedState) -> anyhow::Result<()> {
    // 1. Inisialisasi SQLite Database
    let db_pool = match db::init_db().await {
        Ok(pool) => pool,
        Err(e) => {
            error!("Fatal Error: Database SQLite gagal disiapkan! {}", e);
            return Err(e);
        }
    };

    // 2. Jalankan Petugas Bersih-bersih (Archive Cleanup) di background setiap 24 jam
    let cleanup_pool = db_pool.clone();
    tokio::spawn(async move {
        loop {
            // Tidur selama 24 jam
            tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
            db::cleanup_old_data(&cleanup_pool).await;
        }
    });

    info!("Memulai Trading Engine (Auto-Reconnect Active)...");
    
    // Logika Auto-Reconnect: Jika internet VPS kedip, bot tidak akan mati, 
    // melainkan mencoba konek ulang setelah 5 detik.
    loop {
        // 3. Cold Start: Menyedot riwayat dari REST API agar bot langsung pintar (HFT Style)
        let mut scanner = Scanner::new();
        match api::fetch_historical_klines("BTCBIDR", 50).await {
            Ok(history) => {
                for kline in history {
                    // Masukkan riwayat ke otak bot tanpa perlu menunggu 50 menit
                    scanner.process_new_candle("BTCBIDR", kline);
                }
                
                // Beri tahu Telegram bahwa data memori sudah penuh
                {
                    let mut s = state.write().await;
                    s.total_candles = scanner.store.candles.len();
                }
            }
            Err(_) => {
                warn!("Bot gagal menyedot REST API, akan mengambil data manual dari nol (1 menit/candle).");
            }
        }

        // Pass clone of the state, pre-filled scanner, and db_pool into websocket stream
        if let Err(e) = websocket::connect_and_listen(state.clone(), scanner, db_pool.clone()).await {
            error!("WebSocket Engine terputus: {:?}. Mencoba reconnect dalam 5 detik...", e);
            // Ubah status koneksi di shared state menjadi False
            {
                let mut s = state.write().await;
                s.is_connected = false;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        } else {
            // Jika loop keluar secara wajar (jarang terjadi di WSS), kita juga tunggu 5 detik
            warn!("WebSocket Engine berhenti. Mencoba reconnect dalam 5 detik...");
            {
                let mut s = state.write().await;
                s.is_connected = false;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
}
