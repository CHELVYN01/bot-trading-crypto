use tracing::{error, info, warn};
use tokio::sync::mpsc;
use sqlx::{Pool, Postgres};

use super::websocket;
use super::state::SharedState;
use crate::broker::{api, db};
use crate::strategy::scanner::Scanner;
use crate::strategy::signal::TradeSignal;

pub async fn start_engine(
    state: SharedState,
    tx_signal: mpsc::Sender<TradeSignal>,
) -> anyhow::Result<()> {
    // 1. Coba koneksi ke PostgreSQL — BUKAN FATAL jika gagal
    //    Bot tetap jalan tanpa database (hanya audit log yang dimatikan)
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/trading_bot".to_string());

    let db_pool: Option<Pool<Postgres>> = match db::init_db(&database_url).await {
        Ok(pool) => {
            info!("[ENGINE] ✅ Database PostgreSQL terhubung. Audit log aktif.");
            Some(pool)
        }
        Err(e) => {
            warn!(
                "[ENGINE] ⚠️ Database tidak tersedia: {}",
                e
            );
            warn!("[ENGINE] ⚠️ Bot tetap berjalan TANPA audit log. Trading tidak terpengaruh.");
            None
        }
    };

    // 2. Cleanup data lama setiap 24 jam (hanya jika DB tersedia)
    if let Some(ref pool) = db_pool {
        let cleanup_pool = pool.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
                db::cleanup_old_data(&cleanup_pool).await;
            }
        });
    }

    info!("[ENGINE] Trading Engine aktif (Auto-Reconnect enabled)...");

    // Loop Auto-Reconnect: jika WebSocket putus, reconnect dalam 5 detik
    loop {
        // 3. Cold Start: sedot riwayat REST API agar Strategist langsung siap
        let mut scanner = Scanner::new();
        match api::fetch_historical_klines("BTCBIDR", 50).await {
            Ok(history) => {
                let count = history.len();
                for kline in history {
                    // Hanya push ke memory store, tidak emit sinyal dari data historis
                    scanner.store.push(kline);
                }
                {
                    let mut s = state.write().await;
                    s.total_candles = scanner.store.candles.len();
                }
                info!("[ENGINE] Cold Start selesai: {} candle historis dimuat.", count);
            }
            Err(e) => {
                warn!("[ENGINE] REST API gagal ({}). Bot akan belajar dari nol via WebSocket.", e);
            }
        }

        // 4. Jalankan The Listener dengan channel ke Guardian
        let result = websocket::connect_and_listen(
            state.clone(),
            scanner,
            db_pool.clone(), // Teruskan Option<Pool> — bisa None jika DB mati
            tx_signal.clone(),
        )
        .await;

        if let Err(e) = result {
            error!(
                "[ENGINE] WebSocket terputus: {:?}. Reconnect dalam 5 detik...",
                e
            );
        } else {
            warn!("[ENGINE] WebSocket berhenti normal. Reconnect dalam 5 detik...");
        }

        {
            let mut s = state.write().await;
            s.is_connected = false;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
