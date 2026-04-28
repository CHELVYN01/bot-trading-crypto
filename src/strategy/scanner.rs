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
    ///
    /// Returns tuple:
    /// - `(Option<Decimal>, Option<Decimal>)` → (ATR, Z-Score) untuk update SharedState
    /// - Jika sinyal valid, emit TradeSignal melalui `tx_signal` channel
    pub async fn process_new_candle(
        &mut self,
        symbol: &str,
        kline: Kline,
        tx_signal: &mpsc::Sender<TradeSignal>,
    ) -> (Option<Decimal>, Option<Decimal>) {
        let close_price = kline.close;

        // 1. Simpan candle ke ring buffer
        self.store.push(kline);

        // 2. Hitung indikator teknikal
        let atr = calculate_atr(&self.store.candles, 14);
        let z_score = calculate_z_score_volume(&self.store.candles, 20);

        // 3. Evaluasi sinyal jika data sudah cukup
        if let (Some(atr_val), Some(z_val)) = (atr, z_score) {
            info!(
                "🧠 [STRATEGIST] {}: Close=Rp {} | ATR(14)=Rp {:.2} | Z-Score(20)={:.2}",
                symbol.to_uppercase(),
                close_price,
                atr_val,
                z_val
            );

            // === SINYAL BUY: Volume Surge terdeteksi ===
            if z_val > Z_SCORE_THRESHOLD {
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
                    "🚀 [STRATEGIST] SINYAL BUY! {} | Entry=Rp {} | SL=Rp {:.0} | TP=Rp {:.0} | Z={:.2}",
                    symbol, close_price, stop_loss, take_profit, z_val
                );

                // Kirim sinyal ke Guardian via channel (non-blocking)
                if let Err(e) = tx_signal.try_send(signal) {
                    tracing::warn!("[STRATEGIST] Channel penuh, sinyal diabaikan: {}", e);
                }
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
