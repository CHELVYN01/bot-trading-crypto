use tracing::{error, info, warn};
use super::websocket;
use super::state::SharedState;

pub async fn start_engine(state: SharedState) -> anyhow::Result<()> {
    info!("Memulai Trading Engine (Auto-Reconnect Active)...");
    
    // Logika Auto-Reconnect: Jika internet VPS kedip, bot tidak akan mati, 
    // melainkan mencoba konek ulang setelah 5 detik.
    loop {
        // Pass a clone of the state into websocket stream
        if let Err(e) = websocket::connect_and_listen(state.clone()).await {
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
