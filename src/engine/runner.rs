use tracing::{error, info, warn};
use super::websocket;

pub async fn start_engine() -> anyhow::Result<()> {
    info!("Memulai Trading Engine (Auto-Reconnect Active)...");
    
    // Logika Auto-Reconnect: Jika internet VPS kedip, bot tidak akan mati, 
    // melainkan mencoba konek ulang setelah 5 detik.
    loop {
        if let Err(e) = websocket::connect_and_listen().await {
            error!("WebSocket Engine terputus: {:?}. Mencoba reconnect dalam 5 detik...", e);
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        } else {
            // Jika loop keluar secara wajar (jarang terjadi di WSS), kita juga tunggu 5 detik
            warn!("WebSocket Engine berhenti. Mencoba reconnect dalam 5 detik...");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
}
