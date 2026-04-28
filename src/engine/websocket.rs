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

pub async fn connect_and_listen(
    state: SharedState,
    db_pool: Option<Pool<Postgres>>,
    tx_signal: mpsc::Sender<TradeSignal>,
) -> anyhow::Result<()> {
    // 1. DYNAMIC DISCOVERY: Cari koin yang paling 'hot' (Top 10 Gainers)
    let dynamic_symbols = match crate::broker::api::fetch_top_gainers_bidr(10).await {
        Ok(syms) => syms,
        Err(e) => {
            warn!("⚠️ Gagal mencari koin dinamis: {}. Pakai fallback BTC & ETH.", e);
            vec!["btcbidr".to_string(), "ethbidr".to_string()]
        }
    };

    // Bangun URL Combined Stream
    let streams = dynamic_symbols
        .iter()
        .map(|s| format!("{}@kline_1m", s))
        .collect::<Vec<_>>()
        .join("/");
    
    let full_url = format!("{}{}", BINANCE_COMBINED_BASE, streams);
    
    info!("👂 [LISTENER] Menghubungkan Market Scanner ke {} koin Top Gainers...", dynamic_symbols.len());

    let (ws_stream, _) = connect_async(full_url).await?;

    {
        let mut s = state.write().await;
        s.is_connected = true;
    }
    info!("✅ [LISTENER] Market Scanner Aktif! Memantau sinyal di seluruh pasar.");

    let (_, mut read) = ws_stream.split();

    // 2. Inisialisasi Scanner dan COLD START untuk tiap koin
    let mut scanners: HashMap<String, Scanner> = HashMap::new();

    {
        // Bersihkan data market lama agar status Telegram cuma isi koin yang baru
        let mut s = state.write().await;
        s.market_data.clear();
    }

    info!("🚀 [ENGINE] Memulai Cold Start untuk {} koin Hunter...", dynamic_symbols.len());

    for symbol in &dynamic_symbols {
        let sym_upper = symbol.to_uppercase();
        let mut scanner = Scanner::new();

        // Ambil 50 data sejarah dari REST API
        match crate::broker::api::fetch_historical_klines(symbol, 50).await {
            Ok(klines) => {
                let last_price = klines.last().map(|k| k.close);
                
                // Masukkan semua data sejarah ke scanner (tanpa kirim sinyal)
                for kline in klines {
                    scanner.store.push(kline);
                }

                // Hitung indikator awal
                let atr = crate::strategy::indicators::calculate_atr(&scanner.store.candles, 14);
                let z_score = crate::strategy::indicators::calculate_z_score_volume(&scanner.store.candles, 20);

                // Update SharedState agar Telegram langsung valid
                {
                    let mut s = state.write().await;
                    let entry = s.market_data.entry(sym_upper.clone()).or_insert_with(SymbolState::default);
                    entry.last_price = last_price;
                    entry.current_atr = atr;
                    entry.current_z_score = z_score;
                    entry.total_candles = scanner.store.candles.len();
                }

                if let (Some(a), Some(z)) = (atr, z_score) {
                    info!("📊 [ANALYSIS] {}: ATR=Rp {:.0} | Z-Score={:.2} (Ready)", sym_upper, a, z);
                }

                scanners.insert(sym_upper.clone(), scanner);
                debug!("[ENGINE] Cold Start selesai untuk {}", sym_upper);
            }
            Err(e) => {
                error!("[ENGINE] Gagal Cold Start untuk {}: {}", sym_upper, e);
                scanners.insert(sym_upper.clone(), scanner);
            }
        }
    }

    info!("✅ [ENGINE] Cold Start Selesai. Seluruh data indikator telah terisi.");

    let mut msg_count = 0;

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                msg_count += 1;
                if msg_count % 100 == 0 {
                    info!("💓 [LISTENER] Heartbeat: {} data market diterima...", msg_count);
                }

                let json: serde_json::Value = serde_json::from_str(&text)?;
                let data = &json["data"];
                
                if let Ok(event) = serde_json::from_value::<TokocryptoKlineEvent>(data.clone()) {
                    let symbol = event.symbol.clone();
                    let current_price = event.kline.close;

                    // Update harga real-time di SharedState per symbol
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
