use rust_decimal::Decimal;
use std::sync::Arc;
use std::collections::HashMap;
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

/// Status untuk satu koin tertentu (BTC, ETH, dkk)
#[derive(Clone, Default, Debug)]
pub struct SymbolState {
    pub last_price: Option<Decimal>,
    pub current_atr: Option<Decimal>,
    pub current_z_score: Option<Decimal>,
    pub total_candles: usize,
    pub is_whale_alert: bool,
    pub order_book_imbalance: Option<Decimal>, // 0.0 - 1.0
    pub bid_wall: Option<(Decimal, Decimal)>,  // (Price, Volume)
    pub ask_wall: Option<(Decimal, Decimal)>,  // (Price, Volume)
}

/// Struktur utama yang menyimpan kondisi terkini dari bot.
#[derive(Clone, Default)]
pub struct BotState {
    pub is_connected: bool,
    pub trading_mode: TradingMode,
    // Map: Nama Koin -> Statusnya (Contoh: "BTCBIDR" -> SymbolState)
    pub market_data: HashMap<String, SymbolState>,
}

// Tipe data Shared Memory (Thread Safe)
pub type SharedState = Arc<RwLock<BotState>>;
