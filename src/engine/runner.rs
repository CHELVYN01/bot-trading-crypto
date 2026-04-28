use tracing::{error, info, warn};
use tokio::sync::mpsc;
use sqlx::{Pool, Postgres};

use super::websocket;
use super::state::SharedState;
use crate::broker::db;
use crate::strategy::signal::TradeSignal;

pub async fn start_engine(
    state: SharedState,
    tx_signal: mpsc::Sender<TradeSignal>,
) -> anyhow::Result<()> {
    // 1. Koneksi Database (Optional)
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/trading_bot".to_string());

    let db_pool: Option<Pool<Postgres>> = match db::init_db(&database_url).await {
        Ok(pool) => {
            info!("[ENGINE] ✅ Database PostgreSQL terhubung. Audit log aktif.");
            Some(pool)
        }
        Err(e) => {
            warn!("[ENGINE] ⚠️ Database tidak tersedia: {}. Berjalan tanpa audit log.", e);
            None
        }
    };

    // 2. Background Task: Cleanup Data Lama (24 Jam sekali)
    if let Some(ref pool) = db_pool {
        let cleanup_pool = pool.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
                db::cleanup_old_data(&cleanup_pool).await;
            }
        });
    }

    info!("[ENGINE] Market Scanner Engine aktif (Multi-Coin support)...");

    // 3. Loop Auto-Reconnect: Market Scanner (The Listener)
    loop {
        let result = websocket::connect_and_listen(
            state.clone(),
            db_pool.clone(),
            tx_signal.clone(),
        )
        .await;

        if let Err(e) = result {
            error!(
                "[ENGINE] Market Scanner terputus: {:?}. Reconnect dalam 5 detik...",
                e
            );
        } else {
            warn!("[ENGINE] Market Scanner berhenti normal. Reconnect dalam 5 detik...");
        }

        {
            let mut s = state.write().await;
            s.is_connected = false;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
