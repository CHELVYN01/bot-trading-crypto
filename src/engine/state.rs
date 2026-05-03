use rust_decimal::Decimal;
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};
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

/// Status trend Bitcoin dalam 5 candle terakhir.
/// Digunakan Guardian untuk memutuskan apakah alt coin boleh dibeli.
#[derive(Clone, Debug, PartialEq)]
pub enum BtcTrend {
    /// BTC naik > +0.3% → alt season siap, entry diperbolehkan
    Bullish,
    /// BTC turun < -0.3% → mode defensif, alt coin diblokir
    Bearish,
    /// Sideways antara -0.3% dan +0.3% → selektif, hanya sinyal kuat
    Neutral,
}

impl Default for BtcTrend {
    fn default() -> Self {
        BtcTrend::Neutral
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
#[derive(Clone)]
pub struct BotState {
    pub is_connected: bool,
    pub trading_mode: TradingMode,
    /// Map: Nama Koin → Statusnya (Contoh: "BTCBIDR" → SymbolState)
    pub market_data: HashMap<String, SymbolState>,
    /// Trend BTC saat ini — diperbarui setiap candle 1m BTCBIDR ditutup
    pub btc_trend: BtcTrend,
    /// Persentase perubahan BTC dalam 5 candle terakhir
    pub btc_change_pct: Option<f64>,
    /// Ring buffer 5 close price BTCBIDR untuk menghitung trend
    pub btc_price_history: VecDeque<Decimal>,
}

impl Default for BotState {
    fn default() -> Self {
        Self {
            is_connected: false,
            trading_mode: TradingMode::default(),
            market_data: HashMap::new(),
            btc_trend: BtcTrend::Neutral,
            btc_change_pct: None,
            btc_price_history: VecDeque::new(),
        }
    }
}

// Tipe data Shared Memory (Thread Safe)
pub type SharedState = Arc<RwLock<BotState>>;
