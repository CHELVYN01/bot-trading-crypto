use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, PartialEq)]
pub enum TradingMode {
    Paper,
    Live,
}

impl Default for TradingMode {
    fn default() -> Self {
        TradingMode::Paper
    }
}

/// Struktur yang menyimpan kondisi terkini dari bot.
/// Semua variabel di sini bisa dibaca oleh Telegram dan ditulis oleh Engine.
#[derive(Clone, Default)]
pub struct BotState {
    pub is_connected: bool,
    pub last_price: Option<Decimal>,
    pub current_atr: Option<Decimal>,
    pub current_z_score: Option<Decimal>,
    pub total_candles: usize,
    pub is_whale_alert: bool,
    pub trading_mode: TradingMode,
}

// Tipe data Papan Tulis (Shared Memory) yang aman untuk antar-thread (Thread Safe)
pub type SharedState = Arc<RwLock<BotState>>;
