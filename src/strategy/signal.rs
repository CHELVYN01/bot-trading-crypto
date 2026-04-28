use rust_decimal::Decimal;
use chrono::{DateTime, Utc};

/// Arah sinyal trading yang dihasilkan oleh The Strategist.
#[derive(Debug, Clone, PartialEq)]
pub enum SignalDirection {
    Buy,
    Sell,
}

/// Sinyal trading yang dikirim dari The Strategist → The Guardian via mpsc channel.
/// Berisi semua informasi yang dibutuhkan Guardian untuk eksekusi dan manajemen risiko.
#[derive(Debug, Clone)]
pub struct TradeSignal {
    /// Simbol pair trading, contoh: "BTCBIDR"
    pub symbol: String,

    /// Arah sinyal: Buy atau Sell
    pub direction: SignalDirection,

    /// Harga saat sinyal dihasilkan (harga close candle terakhir)
    pub entry_price: Decimal,

    /// Nilai ATR(14) saat sinyal, digunakan untuk hitung Stop Loss
    pub atr: Decimal,

    /// Stop Loss = entry_price - (ATR × 1.5) untuk Buy
    /// Dihitung oleh Strategist, dieksekusi oleh Guardian
    pub stop_loss: Decimal,

    /// Target profit = entry_price + (ATR × 3.0) → Risk:Reward 1:2
    pub take_profit: Decimal,

    /// Z-Score volume saat sinyal (untuk audit log)
    pub z_score: Decimal,

    /// Timestamp sinyal dibuat
    pub timestamp: DateTime<Utc>,
}

/// Laporan hasil trade yang dikirim dari The Guardian → The Messenger via mpsc channel.
#[derive(Debug, Clone)]
pub struct TradeReport {
    /// Simbol yang diperdagangkan
    pub symbol: String,

    /// Jenis laporan: entry beli, exit untung, exit rugi
    pub report_type: ReportType,

    /// Harga eksekusi aktual dari exchange
    pub executed_price: Decimal,

    /// Jumlah quantity yang dibeli/dijual
    pub quantity: Decimal,

    /// Profit/Loss dalam IDR (jika trade sudah selesai)
    pub pnl_idr: Option<Decimal>,

    /// Saldo equity setelah trade selesai
    pub equity_idr: Option<Decimal>,

    /// Timestamp kejadian
    pub timestamp: DateTime<Utc>,
}

/// Jenis laporan trade untuk Telegram
#[derive(Debug, Clone)]
pub enum ReportType {
    /// Bot membeli aset — Entry signal
    Entry,
    /// Bot menjual aset karena Take Profit tercapai
    TakeProfit,
    /// Bot menjual aset karena Stop Loss tersentuh
    StopLoss,
    /// Bot menjual aset karena Trailing Stop aktif
    TrailingStop,
}
