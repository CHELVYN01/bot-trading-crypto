use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use crate::broker::model::DepthEvent;

/// Menghitung Order Book Imbalance (OBI)
/// OBI > 0.5 berarti tekanan beli lebih kuat (Bid lebih tebal)
/// OBI < 0.5 berarti tekanan jual lebih kuat (Ask lebih tebal)
pub fn calculate_imbalance(depth: &DepthEvent) -> Decimal {
    let total_bid_vol: Decimal = depth.bids.iter().map(|pair| pair[1]).sum();
    let total_ask_vol: Decimal = depth.asks.iter().map(|pair| pair[1]).sum();

    let total_vol = total_bid_vol + total_ask_vol;
    
    if total_vol.is_zero() {
        return dec!(0.5);
    }

    // Presisi: Total Bid dibagi Total Volume (Bid + Ask)
    total_bid_vol / total_vol
}

/// Mencari "Tembok" (Wall) terbesar di sisi Bid atau Ask
/// Mengembalikan (Price, Volume)
pub fn find_walls(depth: &DepthEvent) -> (Option<(Decimal, Decimal)>, Option<(Decimal, Decimal)>) {
    let max_bid = depth.bids.iter().max_by(|a, b| a[1].cmp(&b[1]));
    let max_ask = depth.asks.iter().max_by(|a, b| a[1].cmp(&b[1]));

    let bid_wall = max_bid.map(|p| (p[0], p[1]));
    let ask_wall = max_ask.map(|p| (p[0], p[1]));

    (bid_wall, ask_wall)
}
