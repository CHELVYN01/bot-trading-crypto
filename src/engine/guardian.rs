use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use chrono::Utc;

use crate::broker::order;
use crate::engine::state::{SharedState, TradingMode};
use crate::strategy::signal::{ReportType, TradeReport, TradeSignal};

/// Representasi posisi yang sedang aktif (bot sedang memegang aset).
#[derive(Debug, Clone)]
struct ActivePosition {
    /// Simbol yang sedang dipegang
    symbol: String,

    /// Harga beli aktual dari exchange
    entry_price: Decimal,

    /// Quantity aset yang dipegang
    quantity: Decimal,

    /// Stop Loss awal (ATR × 1.5)
    stop_loss: Decimal,

    /// Target take profit (Risk:Reward 1:2)
    take_profit: Decimal,

    /// Trailing Stop Loss (diperbarui setiap candle baru)
    trailing_stop: Decimal,

    /// Harga tertinggi yang pernah dicapai sejak entry (untuk trailing)
    highest_price: Decimal,
}

/// The Guardian: Task paling kritis di seluruh sistem.
///
/// Tanggung jawab:
/// 1. Menerima TradeSignal dari The Strategist
/// 2. Verifikasi saldo & validasi sinyal sebelum eksekusi
/// 3. Eksekusi BUY order ke Tokocrypto (LIVE) atau simulasi (PAPER)
/// 4. Memantau harga aktif vs Stop Loss / Take Profit setiap tick
/// 5. Eksekusi SELL jika kondisi exit terpenuhi
/// 6. Mengirim TradeReport ke The Messenger
pub async fn run_guardian(
    mut rx_signal: mpsc::Receiver<TradeSignal>,
    tx_report: mpsc::Sender<TradeReport>,
    state: SharedState,
) -> Result<()> {
    info!("🛡️ [GUARDIAN] Task Guardian aktif. Memantau sinyal masuk...");

    // Budget awal dari .env
    let initial_budget: Decimal = std::env::var("INITIAL_BUDGET_IDR")
        .unwrap_or_else(|_| "500000".to_string())
        .parse()
        .unwrap_or(dec!(500000));

    let api_key = std::env::var("TOKOCRYPTO_API_KEY")
        .unwrap_or_default();
    let secret_key = std::env::var("TOKOCRYPTO_SECRET_KEY")
        .unwrap_or_default();

    let atr_multiplier = std::env::var("ATR_STOP_LOSS_MULTIPLIER")
        .unwrap_or_else(|_| "1.5".to_string())
        .parse::<Decimal>()
        .unwrap_or(dec!(1.5));

    // Equity yang dikelola Guardian (tidak hardcoded, bisa update)
    let mut equity = initial_budget;

    // Posisi aktif saat ini. None = Flat (tidak memegang aset).
    let mut position: Option<ActivePosition> = None;

    info!(
        "🛡️ [GUARDIAN] Modal aktif: Rp {} | ATR Multiplier: {}x",
        equity, atr_multiplier
    );

    // Loop utama: menggunakan interval untuk pengecekan real-time jika ada posisi
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));

    loop {
        tokio::select! {
            // 1. Terima sinyal baru untuk Entry
            Some(signal) = rx_signal.recv() => {
                if position.is_none() {
                    // Logika Entry (Sama seperti sebelumnya)
                    if signal.z_score >= dec!(2.5) && equity >= dec!(10000) {
                        info!("🎯 [GUARDIAN] Sinyal VALID! {} | Entry: Rp {}", signal.symbol, signal.entry_price);
                        
                        let buy_result = match trading_mode {
                            TradingMode::Live => order::place_buy_order(&api_key, &secret_key, &signal.symbol, equity).await,
                            TradingMode::Paper => {
                                let simulated_qty = equity / signal.entry_price;
                                Ok(order::OrderResult {
                                    order_id: 0,
                                    symbol: signal.symbol.clone(),
                                    executed_price: signal.entry_price,
                                    executed_qty: simulated_qty,
                                })
                            }
                        };

                        if let Ok(result) = buy_result {
                            let trailing_initial = result.executed_price - (signal.atr * atr_multiplier);
                            position = Some(ActivePosition {
                                symbol: result.symbol.clone(),
                                entry_price: result.executed_price,
                                quantity: result.executed_qty,
                                stop_loss: signal.stop_loss,
                                take_profit: signal.take_profit,
                                trailing_stop: trailing_initial,
                                highest_price: result.executed_price,
                            });
                            
                            info!("✅ [GUARDIAN] Posisi AKTIF: {} | SL: Rp {} | TP: Rp {}", result.symbol, signal.stop_loss, signal.take_profit);
                            
                            let _ = tx_report.send(TradeReport {
                                symbol: result.symbol,
                                report_type: ReportType::Entry,
                                executed_price: result.executed_price,
                                quantity: result.executed_qty,
                                pnl_idr: None,
                                equity_idr: Some(equity),
                                timestamp: Utc::now(),
                            }).await;
                        }
                    }
                }
            }

            // 2. Cek posisi aktif secara Real-time setiap 500ms
            _ = interval.tick() => {
                if let Some(mut pos) = position.clone() {
                    // Ambil harga terbaru dari SharedState
                    let current_price = {
                        let s = state.read().await;
                        s.market_data.get(&pos.symbol).and_then(|d| d.last_price)
                    };

                    if let Some(price) = current_price {
                        // Cek Kondisi Exit (TP/SL/Trailing)
                        if let Some(exit_reason) = check_exit_condition(&pos, price) {
                            info!("🚪 [GUARDIAN] EXIT Real-time: {:?} | {} @ Rp {}", exit_reason, pos.symbol, price);
                            
                            let sell_result = match trading_mode {
                                TradingMode::Live => order::place_sell_order(&api_key, &secret_key, &pos.symbol, pos.quantity).await,
                                TradingMode::Paper => Ok(order::OrderResult {
                                    order_id: 0,
                                    symbol: pos.symbol.clone(),
                                    executed_price: price,
                                    executed_qty: pos.quantity,
                                }),
                            };

                            if let Ok(result) = sell_result {
                                let pnl = (result.executed_price * result.executed_qty) - (pos.entry_price * pos.quantity);
                                equity += pnl;
                                
                                let _ = tx_report.send(TradeReport {
                                    symbol: pos.symbol.clone(),
                                    report_type: exit_reason,
                                    executed_price: result.executed_price,
                                    quantity: result.executed_qty,
                                    pnl_idr: Some(pnl),
                                    equity_idr: Some(equity),
                                    timestamp: Utc::now(),
                                }).await;
                                
                                position = None;
                                info!("💰 [GUARDIAN] Exit Selesai. PnL: Rp {:.0} | Equity: Rp {:.0}", pnl, equity);
                            }
                        } else {
                            // Update Trailing Stop jika harga naik
                            if price > pos.highest_price {
                                pos.highest_price = price;
                                // Kita asumsikan ATR tetap (atau bisa diupdate dari state jika perlu)
                                // Untuk simplifikasi, kita pakai ATR saat entry
                                let new_trailing = price - (pos.entry_price - pos.stop_loss); // Jarak SL awal
                                if new_trailing > pos.trailing_stop {
                                    pos.trailing_stop = new_trailing;
                                }
                                position = Some(pos);
                            }
                        }
                    }
                }
            }
        }
    }

    warn!("[GUARDIAN] Channel sinyal tertutup. Guardian berhenti.");
    Ok(())
}

/// Evaluasi apakah posisi aktif harus di-exit.
/// Mengembalikan Some(ReportType) jika exit, None jika masih hold.
fn check_exit_condition(pos: &ActivePosition, current_price: Decimal) -> Option<ReportType> {
    // 1. Take Profit tercapai
    if current_price >= pos.take_profit {
        info!(
            "🎉 [GUARDIAN] TAKE PROFIT! {} | Harga: Rp {} | TP: Rp {}",
            pos.symbol, current_price, pos.take_profit
        );
        return Some(ReportType::TakeProfit);
    }

    // 2. Trailing Stop Loss tersentuh (lebih prioritas dari SL statis)
    if current_price <= pos.trailing_stop {
        info!(
            "🔴 [GUARDIAN] TRAILING STOP! {} | Harga: Rp {} | TSL: Rp {}",
            pos.symbol, current_price, pos.trailing_stop
        );
        return Some(ReportType::TrailingStop);
    }

    // 3. Stop Loss statis tersentuh (safety net)
    if current_price <= pos.stop_loss {
        info!(
            "🔴 [GUARDIAN] STOP LOSS! {} | Harga: Rp {} | SL: Rp {}",
            pos.symbol, current_price, pos.stop_loss
        );
        return Some(ReportType::StopLoss);
    }

    None
}
