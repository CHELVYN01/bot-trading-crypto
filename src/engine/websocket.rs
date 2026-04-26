use futures_util::StreamExt;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::broker::model::TokocryptoKlineEvent;
use crate::strategy::scanner::Scanner;
use crate::engine::state::SharedState;

// Target: BTC-BIDR (Bitcoin to Rupiah) di timeframe 1 menit
// Menggunakan server bypass resmi Tokocrypto untuk menghindari blokir internet lokal
const TOKOCRYPTO_WS_URL: &str = "wss://stream-toko.2meta.app/ws/btcbidr@kline_1m";

pub async fn connect_and_listen(state: SharedState) -> anyhow::Result<()> {
    info!("Menghubungkan 'Mata Bot' ke Tokocrypto WebSocket: {}", TOKOCRYPTO_WS_URL);

    let (ws_stream, _) = connect_async(TOKOCRYPTO_WS_URL).await?;
    
    // Set status menjadi Connected
    {
        let mut s = state.write().await;
        s.is_connected = true;
    }
    info!("✅ [Phase 2] Berhasil terhubung ke Tokocrypto WebSocket!");

    let (_, mut read) = ws_stream.split();
    
    // Inisialisasi Otak Bot (Scanner)
    let mut scanner = Scanner::new();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parsing JSON ke Struct menggunakan serde_json dan rust_decimal
                match serde_json::from_str::<TokocryptoKlineEvent>(&text) {
                    Ok(event) => {
                        // Kita hanya akan memproses jika candle (1 menit) sudah Final
                        // agar perhitungan matematis tidak meleset akibat harga yang masih bergerak.
                        if event.kline.is_final {
                            let close_price = event.kline.close;
                            
                            // Masukkan data ke Scanner untuk dihitung (Phase 3)
                            let (atr, z_score) = scanner.process_new_candle(&event.symbol, event.kline);
                            
                            // Update Shared State agar bisa dibaca oleh Telegram (Phase 3.5)
                            {
                                let mut s = state.write().await;
                                s.last_price = Some(close_price);
                                s.current_atr = atr;
                                s.current_z_score = z_score;
                                s.total_candles = scanner.store.candles.len();
                                
                                // Jika ada whale alert (Z-Score > 2.0)
                                let threshold = rust_decimal::Decimal::from_f64_retain(2.0).unwrap_or(rust_decimal::Decimal::ZERO);
                                if let Some(z) = z_score {
                                    if z > threshold {
                                        s.is_whale_alert = true;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Gagal parsing JSON dari Tokocrypto: {}. Data raw: {}", e, text);
                    }
                }
            }
            Ok(Message::Ping(_)) => {
                // Ping-Pong dikendalikan otomatis oleh tungstenite
            }
            Ok(msg) => {
                warn!("Menerima tipe pesan tidak terduga dari WebSocket: {:?}", msg);
            }
            Err(e) => {
                error!("Error pada koneksi WebSocket Tokocrypto: {:?}", e);
                break; // Keluar dari loop agar di-restart oleh Auto-Reconnect
            }
        }
    }

    error!("Koneksi WebSocket terputus.");
    Ok(())
}
