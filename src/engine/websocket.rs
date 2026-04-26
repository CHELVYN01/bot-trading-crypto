use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};

use crate::broker::model::TokocryptoKlineEvent;

// Target: BTC-BIDR (Bitcoin to Rupiah) di timeframe 1 menit
// Menggunakan server bypass resmi Tokocrypto untuk menghindari blokir internet lokal
const TOKOCRYPTO_WS_URL: &str = "wss://stream-toko.2meta.app/ws/btcbidr@kline_1m";

pub async fn connect_and_listen() -> anyhow::Result<()> {
    info!("Menghubungkan 'Mata Bot' ke Tokocrypto WebSocket: {}", TOKOCRYPTO_WS_URL);

    let (ws_stream, _) = connect_async(TOKOCRYPTO_WS_URL).await?;
    info!("✅ [Phase 2] Berhasil terhubung ke Tokocrypto WebSocket!");

    let (_, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parsing JSON ke Struct menggunakan serde_json dan rust_decimal
                match serde_json::from_str::<TokocryptoKlineEvent>(&text) {
                    Ok(event) => {
                        // Kita hanya akan memproses jika candle (1 menit) sudah Final
                        // agar perhitungan matematis tidak meleset akibat harga yang masih bergerak.
                        if event.kline.is_final {
                            info!(
                                "📈 [K-LINE FINAL] Pair: {}, Close: Rp {}, Volume: {}",
                                event.symbol.to_uppercase(),
                                event.kline.close,
                                event.kline.volume
                            );
                            // TODO (Phase 3): Teruskan data OHLCV ini ke Memory Store / Indicator Engine
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
