use rust_decimal::Decimal;
use tracing::info;

use crate::broker::model::Kline;
use super::store::MarketStore;
use super::indicators::{calculate_atr, calculate_z_score_volume};

pub struct Scanner {
    pub store: MarketStore,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            // Kita menyimpan maksimal 50 candle terakhir di RAM,
            // lebih dari cukup untuk hitung ATR(14) dan Z-Score(20).
            store: MarketStore::new(50), 
        }
    }

    /// Dipanggil oleh WebSocket setiap kali ada 1 candle yang sudah selesai (Final).
    pub fn process_new_candle(&mut self, symbol: &str, kline: Kline) {
        let close_price = kline.close;
        
        // 1. Simpan candle ke Memory Store
        self.store.push(kline);

        // 2. Hitung indikator teknikal (ATR 14, Z-Score 20)
        let atr = calculate_atr(&self.store.candles, 14);
        let z_score = calculate_z_score_volume(&self.store.candles, 20);

        // 3. Jika data sudah cukup untuk dihitung (karena array sudah terisi > 20)
        if let (Some(a), Some(z)) = (atr, z_score) {
            info!(
                "🧠 [BRAIN] {}: Harga Close: Rp {} | ATR(14): Rp {:.2} | Vol Z-Score(20): {:.2}", 
                symbol.to_uppercase(), close_price, a, z
            );
            
            // Logika Deteksi Paus (Whale): Z-Score Volume lebih dari 2.0 (Lonjakan sangat abnormal)
            let threshold = rust_decimal::Decimal::from_f64_retain(2.0).unwrap_or(Decimal::ZERO);
            if z > threshold {
                info!("⚠️ 🐋 WHALE ALERT! Lonjakan Volume ekstrim terdeteksi! Z-Score: {:.2}!", z);
            }
        } else {
            // Jika data belum cukup
            info!(
                "⏳ [BRAIN] Mengumpulkan data historis pasar... ({}/{})", 
                self.store.candles.len(), 20
            );
        }
    }
}
