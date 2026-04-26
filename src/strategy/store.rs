use std::collections::VecDeque;
use crate::broker::model::Kline;

/// Menyimpan riwayat grafik OHLCV dalam memori (RAM) tanpa database eksternal.
/// Menggunakan struktur Ring Buffer (VecDeque) untuk performa Zero-Cost Allocation.
pub struct MarketStore {
    pub max_size: usize,
    pub candles: VecDeque<Kline>,
}

impl MarketStore {
    /// Inisialisasi Market Store dengan batas kapasitas maksimal.
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            candles: VecDeque::with_capacity(max_size),
        }
    }

    /// Memasukkan data candle baru. Jika kapasitas penuh, data paling lama akan dibuang (Shift).
    pub fn push(&mut self, kline: Kline) {
        if self.candles.len() == self.max_size {
            self.candles.pop_front();
        }
        self.candles.push_back(kline);
    }
}
