use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};
use sqlx::{Pool, Postgres};

use crate::broker::model::TokocryptoKlineEvent;
use crate::strategy::scanner::Scanner;
use crate::strategy::signal::TradeSignal;
use crate::engine::state::SharedState;

/// Target: BTC-BIDR (Bitcoin to Rupiah) di timeframe 1 menit
const TOKOCRYPTO_WS_URL: &str = "wss://stream-toko.2meta.app/ws/btcbidr@kline_1m";

/// The Listener: Task I/O Bound yang menangkap data mentah dari Tokocrypto WebSocket.
///
/// Pipeline:
/// [Tokocrypto WS] → parse JSON → Scanner (Strategist) → emit signal → Guardian
pub async fn connect_and_listen(
    state: SharedState,
    mut scanner: Scanner,
    db_pool: Option<Pool<Postgres>>,
    tx_signal: mpsc::Sender<TradeSignal>,
) -> anyhow::Result<()> {
    info!("👂 [LISTENER] Menghubungkan ke Tokocrypto WebSocket: {}", TOKOCRYPTO_WS_URL);

    let (ws_stream, _) = connect_async(TOKOCRYPTO_WS_URL).await?;

    // Update status koneksi di SharedState
    {
        let mut s = state.write().await;
        s.is_connected = true;
    }
    info!("✅ [LISTENER] Terhubung ke Tokocrypto WebSocket!");

    let (_, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<TokocryptoKlineEvent>(&text) {
                    Ok(event) => {
                        // Hanya proses candle yang sudah FINAL (1 menit selesai)
                        // Data mid-candle tidak akurat untuk kalkulasi ATR/Z-Score
                        if event.kline.is_final {
                            let symbol = event.symbol.clone();

                            // Ekstrak data candle untuk database
                            let open_price  = event.kline.open;
                            let high_price  = event.kline.high;
                            let low_price   = event.kline.low;
                            let close_price = event.kline.close;
                            let volume      = event.kline.volume;

                            // 1. Simpan ke PostgreSQL di background (Fire & Forget)
                            //    Hanya jika DB tersedia — tidak fatal jika None
                            if let Some(ref pool) = db_pool {
                                let pool_clone = pool.clone();
                                let sym_clone  = symbol.clone();
                                tokio::spawn(async move {
                                    crate::broker::db::save_kline(
                                        &pool_clone, &sym_clone,
                                        open_price, high_price, low_price, close_price, volume,
                                    ).await;
                                });
                            }

                            // 2. Kirim ke The Strategist (Scanner) untuk dihitung
                            // Scanner akan emit TradeSignal ke Guardian jika sinyal valid
                            let (atr, z_score) = scanner
                                .process_new_candle(&symbol, event.kline, &tx_signal)
                                .await;

                            // 3. Update SharedState (papan tulis untuk Telegram /status)
                            {
                                let mut s = state.write().await;
                                s.last_price       = Some(close_price);
                                s.current_atr      = atr;
                                s.current_z_score  = z_score;
                                s.total_candles    = scanner.store.candles.len();

                                // Update whale alert flag
                                let threshold = rust_decimal::Decimal::from_f64_retain(2.0)
                                    .unwrap_or(rust_decimal::Decimal::ZERO);
                                s.is_whale_alert = z_score.map_or(false, |z| z > threshold);
                            }
                        }
                    }
                    Err(e) => {
                        error!("[LISTENER] Gagal parse JSON: {} | Raw: {}", e, text);
                    }
                }
            }
            Ok(Message::Ping(_)) => {
                // Ping-Pong dikelola otomatis oleh tungstenite
            }
            Ok(msg) => {
                warn!("[LISTENER] Tipe pesan tidak dikenal: {:?}", msg);
            }
            Err(e) => {
                error!("[LISTENER] Koneksi WebSocket error: {:?}", e);
                break; // Keluar agar Auto-Reconnect di runner.rs aktif
            }
        }
    }

    error!("[LISTENER] Koneksi WebSocket terputus.");
    Ok(())
}
