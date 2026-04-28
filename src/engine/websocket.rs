use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn, debug};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;

use crate::broker::model::TokocryptoKlineEvent;
use crate::strategy::scanner::Scanner;
use crate::strategy::signal::TradeSignal;
use crate::engine::state::{SharedState, SymbolState};

/// Base URL untuk Combined Streams Binance
const BINANCE_COMBINED_BASE: &str = "wss://stream.binance.com:9443/stream?streams=";

/// List koin BIDR yang akan dipantau (Market Scanner)
pub const SCAN_SYMBOLS: &[&str] = &[
    "btcbidr", "ethbidr", "solbidr", "bnbbidr", "adabidr", 
    "dotbidr", "xrpbidr", "dogebidr", "maticbidr", "nearbidr"
];

pub async fn connect_and_listen(
    state: SharedState,
    db_pool: Option<Pool<Postgres>>,
    tx_signal: mpsc::Sender<TradeSignal>,
) -> anyhow::Result<()> {
    // 1. Bangun URL Combined Stream (contoh: .../stream?streams=btcbidr@kline_1m/ethbidr@kline_1m/...)
    let streams = SCAN_SYMBOLS
        .iter()
        .map(|s| format!("{}@kline_1m", s))
        .collect::<Vec<_>>()
        .join("/");
    
    let full_url = format!("{}{}", BINANCE_COMBINED_BASE, streams);
    
    info!("👂 [LISTENER] Menghubungkan Market Scanner ke {} koin...", SCAN_SYMBOLS.len());

    let (ws_stream, _) = connect_async(full_url).await?;

    {
        let mut s = state.write().await;
        s.is_connected = true;
    }
    info!("✅ [LISTENER] Market Scanner Aktif! Memantau sinyal di seluruh pasar.");

    let (_, mut read) = ws_stream.split();

    // 2. Inisialisasi Scanner terpisah untuk tiap koin
    let mut scanners: HashMap<String, Scanner> = SCAN_SYMBOLS
        .iter()
        .map(|s| (s.to_uppercase(), Scanner::new()))
        .collect();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Combined stream membungkus data asli di dalam field "data"
                let json: serde_json::Value = serde_json::from_str(&text)?;
                let data = &json["data"];
                
                if let Ok(event) = serde_json::from_value::<TokocryptoKlineEvent>(data.clone()) {
                    let symbol = event.symbol.clone();
                    let current_price = event.kline.close;

                    // Update harga real-time di SharedState per symbol (tiap detik)
                    {
                        let mut s = state.write().await;
                        let entry = s.market_data.entry(symbol.clone()).or_insert_with(SymbolState::default);
                        entry.last_price = Some(current_price);
                    }

                    if event.kline.is_final {
                        info!("📦 [LISTENER] Candle 1m Final: {} | Rp {}", symbol, current_price);
                        
                        if let Some(scanner) = scanners.get_mut(&symbol) {
                            let open_price  = event.kline.open;
                            let high_price  = event.kline.high;
                            let low_price   = event.kline.low;
                            let volume      = event.kline.volume;

                            // Simpan ke Database
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

                            // Proses Strategi
                            let (atr, z_score) = scanner
                                .process_new_candle(&symbol, event.kline, &tx_signal)
                                .await;

                            // Update State untuk indikator per-koin
                            {
                                let mut s = state.write().await;
                                if let Some(entry) = s.market_data.get_mut(&symbol) {
                                    entry.current_atr = atr;
                                    entry.current_z_score = z_score;
                                    entry.total_candles = scanner.store.candles.len();
                                    
                                    let threshold = rust_decimal::Decimal::from_f64_retain(2.5).unwrap_or_default();
                                    entry.is_whale_alert = z_score.map_or(false, |z| z > threshold);
                                }
                            }
                        }
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

    error!("[LISTENER] Market Scanner terputus.");
    Ok(())
}
