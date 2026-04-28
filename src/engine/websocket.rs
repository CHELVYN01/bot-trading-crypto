use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn, debug};
use sqlx::{Pool, Postgres};

use crate::broker::model::TokocryptoKlineEvent;
use crate::strategy::scanner::Scanner;
use crate::strategy::signal::TradeSignal;
use crate::engine::state::SharedState;

/// Official Tokocrypto WebSocket URL
const TOKOCRYPTO_WS_URL: &str = "wss://stream.tokocrypto.com/ws/btcbidr@kline_1m";

pub async fn connect_and_listen(
    state: SharedState,
    mut scanner: Scanner,
    db_pool: Option<Pool<Postgres>>,
    tx_signal: mpsc::Sender<TradeSignal>,
) -> anyhow::Result<()> {
    info!("👂 [LISTENER] Mencoba konek ke Official Tokocrypto WS: {}", TOKOCRYPTO_WS_URL);

    let (ws_stream, _) = connect_async(TOKOCRYPTO_WS_URL).await?;

    {
        let mut s = state.write().await;
        s.is_connected = true;
    }
    info!("✅ [LISTENER] Berhasil Terhubung! Menunggu detak jantung market...");

    let (_, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // DEBUG: Log raw data singkat biar kita tau ada data masuk
                debug!("[WS RAW] Data diterima: {}...", &text[..50]);

                match serde_json::from_str::<TokocryptoKlineEvent>(&text) {
                    Ok(event) => {
                        let current_price = event.kline.close;

                        // UPDATE TIAP DETIK: Biar user liat harga gerak di /status
                        {
                            let mut s = state.write().await;
                            s.last_price = Some(current_price);
                        }

                        // HANYA HITUNG STRATEGI JIKA CANDLE SUDAH FINAL (1 MENIT)
                        if event.kline.is_final {
                            info!("📦 [LISTENER] Candle 1m Terbentuk! Harga: Rp {}", current_price);
                            
                            let symbol = event.symbol.clone();
                            let open_price  = event.kline.open;
                            let high_price  = event.kline.high;
                            let low_price   = event.kline.low;
                            let volume      = event.kline.volume;

                            // 1. Simpan ke Database
                            if let Some(ref pool) = db_pool {
                                let pool_clone = pool.clone();
                                let sym_clone  = symbol.clone();
                                tokio::spawn(async move {
                                    crate::broker::db::save_kline(
                                        &pool_clone, &sym_clone,
                                        open_price, high_price, low_price, current_price, volume,
                                    ).await;
                                });
                            }

                            // 2. Kirim ke Strategist
                            let (atr, z_score) = scanner
                                .process_new_candle(&symbol, event.kline, &tx_signal)
                                .await;

                            // 3. Update SharedState untuk indikator
                            {
                                let mut s = state.write().await;
                                s.current_atr      = atr;
                                s.current_z_score  = z_score;
                                s.total_candles    = scanner.store.candles.len();
                                
                                let threshold = rust_decimal::Decimal::from_f64_retain(2.0).unwrap_or_default();
                                s.is_whale_alert = z_score.map_or(false, |z| z > threshold);
                            }
                        }
                    }
                    Err(e) => {
                        error!("[LISTENER] Gagal parse JSON: {} | Raw: {}", e, text);
                    }
                }
            }
            Ok(Message::Ping(_)) => {}
            Err(e) => {
                error!("[LISTENER] WebSocket Error: {:?}", e);
                break;
            }
            _ => {}
        }
    }

    error!("[LISTENER] Terputus dari WebSocket.");
    Ok(())
}
