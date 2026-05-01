use rust_decimal::Decimal;
use serde::Deserialize;

/// Representasi Data dari Stream Tokocrypto
#[derive(Debug, Deserialize)]
pub struct TokocryptoKlineEvent {
    #[serde(rename = "e")]
    pub event_type: String, // "kline"
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String, // "BTCBIDR"
    #[serde(rename = "k")]
    pub kline: Kline,
}

/// Struktur Candlestick (K-Line) dengan tingkat presisi finansial absolut
#[derive(Debug, Deserialize)]
pub struct Kline {
    #[serde(rename = "t")]
    pub start_time: u64,
    #[serde(rename = "T")]
    pub end_time: u64,
    #[serde(rename = "i")]
    pub interval: String,
    
    // rust_decimal::Decimal secara otomatis mengkonversi String JSON ke Decimal
    // berkat fitur `serde-with-str` di Cargo.toml
    #[serde(rename = "o")]
    pub open: Decimal,
    #[serde(rename = "c")]
    pub close: Decimal,
    #[serde(rename = "h")]
    pub high: Decimal,
    #[serde(rename = "l")]
    pub low: Decimal,
    #[serde(rename = "v")]
    pub volume: Decimal,
    
    // Properti krusial untuk HFT: Apakah candle ini sudah ditutup?
    #[serde(rename = "x")]
    pub is_final: bool,
}

/// Representasi Order Book (Depth) Limited Levels (misal: 5, 10, 20)
#[derive(Debug, Deserialize, Clone)]
pub struct DepthEvent {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: u64,
    pub bids: Vec<[Decimal; 2]>, // [Price, Quantity]
    pub asks: Vec<[Decimal; 2]>, // [Price, Quantity]
}
