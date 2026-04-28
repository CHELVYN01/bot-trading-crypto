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

    // Loop utama: terima sinyal dari channel
    while let Some(signal) = rx_signal.recv().await {
        // Baca mode trading (PAPER/LIVE) dari shared state
        let trading_mode = {
            let s = state.read().await;
            s.trading_mode.clone()
        };

        // Baca harga terkini untuk cek posisi yang sedang aktif
        let current_price = signal.entry_price;

        // === BAGIAN 1: CEK POSISI AKTIF (EXIT LOGIC) ===
        if let Some(ref pos) = position.clone() {
            let should_exit = check_exit_condition(pos, current_price);

            if let Some(exit_reason) = should_exit {
                info!(
                    "🚪 [GUARDIAN] Kondisi EXIT terdeteksi: {:?} | {} @ Rp {}",
                    exit_reason, pos.symbol, current_price
                );

                // Eksekusi SELL
                let sell_result = match trading_mode {
                    TradingMode::Live => {
                        order::place_sell_order(&api_key, &secret_key, &pos.symbol, pos.quantity)
                            .await
                    }
                    TradingMode::Paper => {
                        // Simulasi: anggap SELL berhasil di harga saat ini
                        info!("📋 [PAPER] Simulasi SELL {} @ Rp {}", pos.symbol, current_price);
                        Ok(order::OrderResult {
                            order_id: 0,
                            symbol: pos.symbol.clone(),
                            executed_price: current_price,
                            executed_qty: pos.quantity,
                        })
                    }
                };

                match sell_result {
                    Ok(result) => {
                        let sell_value = result.executed_price * result.executed_qty;
                        let buy_value = pos.entry_price * pos.quantity;
                        let pnl = sell_value - buy_value;
                        equity += pnl;

                        info!(
                            "💰 [GUARDIAN] SELL selesai | PnL: Rp {:.0} | Equity baru: Rp {:.0}",
                            pnl, equity
                        );

                        // Kirim laporan ke Messenger
                        let report = TradeReport {
                            symbol: pos.symbol.clone(),
                            report_type: exit_reason,
                            executed_price: result.executed_price,
                            quantity: result.executed_qty,
                            pnl_idr: Some(pnl),
                            equity_idr: Some(equity),
                            timestamp: Utc::now(),
                        };

                        if let Err(e) = tx_report.send(report).await {
                            error!("[GUARDIAN] Gagal kirim laporan ke Messenger: {}", e);
                        }

                        // Reset posisi menjadi Flat
                        position = None;
                    }
                    Err(e) => {
                        error!("❌ [GUARDIAN] SELL ORDER GAGAL: {:?}", e);
                    }
                }

                continue; // Skip bagian ENTRY, tunggu sinyal berikutnya
            }

            // Update Trailing Stop jika harga naik melebihi highest
            if let Some(ref mut pos) = position {
                if current_price > pos.highest_price {
                    pos.highest_price = current_price;
                    // Trailing stop mengikuti harga naik (selalu 1.5 ATR di bawah high)
                    let new_trailing = current_price - (signal.atr * atr_multiplier);
                    if new_trailing > pos.trailing_stop {
                        pos.trailing_stop = new_trailing;
                        info!(
                            "📈 [GUARDIAN] Trailing Stop naik → Rp {:.0} (High baru: Rp {})",
                            pos.trailing_stop, pos.highest_price
                        );
                    }
                }
            }

            continue; // Sudah ada posisi aktif, skip bagian ENTRY
        }

        // === BAGIAN 2: EVALUASI SINYAL BARU (ENTRY LOGIC) ===

        // Validasi sinyal: Z-Score harus > 2.5 untuk entry
        let z_threshold = dec!(2.5);
        if signal.z_score < z_threshold {
            info!(
                "⏩ [GUARDIAN] Sinyal {} ditolak. Z-Score {:.2} < threshold {:.2}",
                signal.symbol, signal.z_score, z_threshold
            );
            continue;
        }

        // Validasi saldo: pastikan equity mencukupi
        if equity < dec!(10000) {
            warn!("⚠️ [GUARDIAN] Equity tidak mencukupi untuk entry. Saldo: Rp {:.0}", equity);
            continue;
        }

        info!(
            "🎯 [GUARDIAN] Sinyal VALID! {} | Z-Score: {:.2} | Entry: Rp {} | SL: Rp {}",
            signal.symbol, signal.z_score, signal.entry_price, signal.stop_loss
        );

        // Eksekusi BUY dengan full equity yang tersedia
        let buy_result = match trading_mode {
            TradingMode::Live => {
                order::place_buy_order(&api_key, &secret_key, &signal.symbol, equity).await
            }
            TradingMode::Paper => {
                // Simulasi: hitung quantity berdasarkan harga entry
                let simulated_qty = equity / signal.entry_price;
                info!(
                    "📋 [PAPER] Simulasi BUY {} | Qty: {:.8} @ Rp {}",
                    signal.symbol, simulated_qty, signal.entry_price
                );
                Ok(order::OrderResult {
                    order_id: 0,
                    symbol: signal.symbol.clone(),
                    executed_price: signal.entry_price,
                    executed_qty: simulated_qty,
                })
            }
        };

        match buy_result {
            Ok(result) => {
                let trailing_initial = result.executed_price - (signal.atr * atr_multiplier);

                // Simpan posisi baru
                position = Some(ActivePosition {
                    symbol: result.symbol.clone(),
                    entry_price: result.executed_price,
                    quantity: result.executed_qty,
                    stop_loss: signal.stop_loss,
                    take_profit: signal.take_profit,
                    trailing_stop: trailing_initial,
                    highest_price: result.executed_price,
                });

                info!(
                    "✅ [GUARDIAN] Posisi AKTIF: {} | Entry: Rp {} | SL: Rp {} | TP: Rp {}",
                    result.symbol, result.executed_price, signal.stop_loss, signal.take_profit
                );

                // Kirim laporan ENTRY ke Messenger
                let report = TradeReport {
                    symbol: result.symbol,
                    report_type: ReportType::Entry,
                    executed_price: result.executed_price,
                    quantity: result.executed_qty,
                    pnl_idr: None,
                    equity_idr: Some(equity),
                    timestamp: Utc::now(),
                };

                if let Err(e) = tx_report.send(report).await {
                    error!("[GUARDIAN] Gagal kirim laporan ENTRY ke Messenger: {}", e);
                }
            }
            Err(e) => {
                error!("❌ [GUARDIAN] BUY ORDER GAGAL: {:?}", e);
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
