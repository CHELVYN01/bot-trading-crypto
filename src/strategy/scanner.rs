use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::info;
use chrono::Utc;
use tokio::sync::mpsc;

use crate::broker::model::Kline;
use crate::strategy::signal::{SignalDirection, TradeSignal};
use super::store::MarketStore;
use super::indicators::{calculate_atr, calculate_z_score_volume};

/// Threshold Z-Score untuk sinyal entry (lonjakan volume abnormal)
const Z_SCORE_THRESHOLD: Decimal = dec!(2.5);

/// ATR Multiplier untuk Stop Loss (1.5x ATR)
const SL_MULTIPLIER: Decimal = dec!(1.5);

/// ATR Multiplier untuk Take Profit (3.0x ATR = Risk:Reward 1:2)
const TP_MULTIPLIER: Decimal = dec!(3.0);

/// Threshold Order Book Imbalance (Min 60% tekanan beli)
const OBI_THRESHOLD: Decimal = dec!(0.6);

pub struct Scanner {
    pub store: MarketStore,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            // Simpan 50 candle terakhir di RAM — cukup untuk ATR(14) dan Z-Score(20)
            store: MarketStore::new(50),
        }
    }

    /// Dipanggil oleh The Listener setiap kali ada final candle baru.
    pub async fn process_new_candle(
        &mut self,
        symbol: &str,
        kline: Kline,
        tx_signal: &mpsc::Sender<TradeSignal>,
        imbalance: Option<Decimal>,
    ) -> (Option<Decimal>, Option<Decimal>) {
        let close_price = kline.close;

        // 1. Simpan candle ke ring buffer
        self.store.push(kline);

        // 2. Hitung indikator teknikal
        let atr = calculate_atr(&self.store.candles, 14);
        let z_score = calculate_z_score_volume(&self.store.candles, 20);

        // 3. Evaluasi sinyal jika data sudah cukup
        if let (Some(atr_val), Some(z_val)) = (atr, z_score) {
            let obi_val = imbalance.unwrap_or(dec!(0.5));
            
            info!(
                "🧠 [STRATEGIST] {}: Price=Rp {} | Z-Score={:.2} | OBI={:.2}",
                symbol.to_uppercase(),
                close_price,
                z_val,
                obi_val
            );

            // === SINYAL BUY: Volume Surge + Order Book Imbalance ===
            if z_val > Z_SCORE_THRESHOLD && obi_val > OBI_THRESHOLD {
                let stop_loss = close_price - (atr_val * SL_MULTIPLIER);
                let take_profit = close_price + (atr_val * TP_MULTIPLIER);

                let signal = TradeSignal {
                    symbol: symbol.to_uppercase(),
                    direction: SignalDirection::Buy,
                    entry_price: close_price,
                    atr: atr_val,
                    stop_loss,
                    take_profit,
                    z_score: z_val,
                    timestamp: Utc::now(),
                };

                info!(
                    "🚀 [STRATEGIST] SINYAL BUY TERKONFIRMASI! {} | Z={:.2} | OBI={:.2}",
                    symbol, z_val, obi_val
                );

                // Kirim sinyal ke Guardian via channel (non-blocking)
                if let Err(e) = tx_signal.try_send(signal) {
                    tracing::warn!("[STRATEGIST] Channel penuh, sinyal diabaikan: {}", e);
                }
            } else if z_val > Z_SCORE_THRESHOLD {
                info!(
                    "⚠️ [STRATEGIST] {} : Volume Surge terdeteksi (Z={:.2}), tapi Order Book tidak mendukung (OBI={:.2}). Trade dibatalkan.",
                    symbol, z_val, obi_val
                );
            }
        } else {
            info!(
                "⏳ [STRATEGIST] Mengumpulkan data: ({}/20 candle)",
                self.store.candles.len()
            );
        }

        (atr, z_score)
    }
}
