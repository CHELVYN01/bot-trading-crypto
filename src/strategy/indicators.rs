use rust_decimal::{Decimal, MathematicalOps};
use rust_decimal::prelude::FromPrimitive;
use crate::broker::model::Kline;

/// Menghitung Average True Range (ATR) untuk menentukan Stop Loss yang dinamis.
/// Rumus: Simple Moving Average dari True Range (TR) selama `period` terakhir.
pub fn calculate_atr(candles: &std::collections::VecDeque<Kline>, period: usize) -> Option<Decimal> {
    if candles.len() < period {
        return None; // Data belum cukup untuk perhitungan
    }
    
    let mut sum_tr = Decimal::ZERO;
    let start_idx = candles.len() - period;
    
    for i in start_idx..candles.len() {
        let current = &candles[i];
        let tr = if i > 0 {
            let prev = &candles[i-1];
            let hl = current.high - current.low;
            let hc = (current.high - prev.close).abs();
            let lc = (current.low - prev.close).abs();
            hl.max(hc).max(lc)
        } else {
            current.high - current.low
        };
        sum_tr += tr;
    }
    
    let n = Decimal::from_usize(period)?;
    Some(sum_tr / n)
}

/// Menghitung Z-Score Volume untuk mendeteksi anomali/lonjakan (Sinyal Whale).
/// Rumus: Z = (Volume Saat Ini - Rata2 Volume) / Deviasi Standar Volume
pub fn calculate_z_score_volume(candles: &std::collections::VecDeque<Kline>, period: usize) -> Option<Decimal> {
    if candles.len() < period {
        return None; // Data belum cukup
    }

    let start_idx = candles.len() - period;
    let mut sum_vol = Decimal::ZERO;
    
    // 1. Hitung Rata-rata (Mean)
    for i in start_idx..candles.len() {
        sum_vol += candles[i].volume;
    }
    
    let n = Decimal::from_usize(period)?;
    let mean = sum_vol / n;
    
    // 2. Hitung Varians (Variance)
    let mut sum_sq_diff = Decimal::ZERO;
    for i in start_idx..candles.len() {
        let diff = candles[i].volume - mean;
        sum_sq_diff += diff * diff;
    }
    
    let variance = sum_sq_diff / n;
    
    // 3. Hitung Deviasi Standar (Standar Deviation)
    // Fungsi .sqrt() memerlukan feature = ["maths"] pada crate rust_decimal
    let std_dev = variance.sqrt()?;
    
    if std_dev.is_zero() {
        return Some(Decimal::ZERO); // Hindari pembagian dengan nol
    }
    
    // 4. Hitung Z-Score dari volume candle terakhir
    let current_vol = candles.back().unwrap().volume;
    Some((current_vol - mean) / std_dev)
}
