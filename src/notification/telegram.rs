use std::env;
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::sync::mpsc;
use tracing::{info, warn};
use chrono::Utc;

use crate::engine::state::{SharedState, BotState, BtcTrend};
use crate::strategy::signal::{TradeReport, ReportType};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Perintah yang didukung bot:")]
enum Command {
    #[command(description = "Menampilkan bantuan ini.")]
    Help,
    #[command(description = "Memulai interaksi dengan bot.")]
    Start,
    #[command(description = "Menampilkan status bot dan market.")]
    Status,
    #[command(description = "Scan top 20 peluang altcoin saat ini (by Z-Score).")]
    Scan,
    #[command(description = "Tes koneksi server.")]
    Ping,
    #[command(description = "Mengubah mode ke PAPER TRADING (Simulasi).")]
    Paper,
    #[command(description = "Mengubah mode ke LIVE TRADING (Uang Asli!).")]
    Live,
}

/// The Messenger: menjalankan 3 sub-task secara konkuren.
/// 1. Push notifikasi trade dari Guardian
/// 2. Auto-scan report setiap 5 menit
/// 3. Command handler untuk admin Telegram
pub async fn run_telegram_bot(
    state: SharedState,
    mut rx_report: mpsc::Receiver<TradeReport>,
) -> anyhow::Result<()> {
    let token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN wajib diisi di .env");
    let admin_chat_id_str = env::var("TELEGRAM_ADMIN_CHAT_ID")
        .expect("TELEGRAM_ADMIN_CHAT_ID wajib diisi di .env");
    let admin_chat_id: i64 = admin_chat_id_str
        .parse()
        .expect("TELEGRAM_ADMIN_CHAT_ID harus berupa angka");

    let bot = Bot::new(token);
    info!("[MESSENGER] Telegram Bot aktif.");

    // Sub-task 1: Push notifikasi dari Guardian ke Telegram
    let bot_for_push = bot.clone();
    tokio::spawn(async move {
        while let Some(report) = rx_report.recv().await {
            let message = format_trade_report(&report);
            if let Err(e) = bot_for_push
                .send_message(ChatId(admin_chat_id), &message)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await
            {
                tracing::error!("[MESSENGER] Gagal kirim notif ke Telegram: {}", e);
            } else {
                info!("[MESSENGER] Notifikasi terkirim: {:?}", report.report_type);
            }
        }
        warn!("[MESSENGER] Channel laporan tertutup.");
    });

    // Sub-task 2: Auto-scan report setiap 5 menit
    let bot_for_scan = bot.clone();
    let state_for_scan = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5 * 60));
        interval.tick().await; // Skip tick pertama (langsung aktif)
        loop {
            interval.tick().await;
            let maybe_msg = {
                let s = state_for_scan.read().await;
                if s.is_connected && s.market_data.len() >= 3 {
                    Some(build_auto_scan_report(&s))
                } else {
                    None
                }
            };
            if let Some(msg) = maybe_msg {
                if let Err(e) = bot_for_scan
                    .send_message(ChatId(admin_chat_id), &msg)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await
                {
                    warn!("[MESSENGER] Gagal kirim auto-scan: {}", e);
                }
            }
        }
    });

    // Sub-task 3: Command handler (blocking)
    Command::repl(bot, move |bot: Bot, msg: Message, cmd: Command| {
        let admin_id_str = admin_chat_id_str.clone();
        let state_clone = state.clone();

        async move {
            // Keamanan: hanya Admin yang bisa kontrol bot
            if msg.chat.id.0.to_string() != admin_id_str {
                warn!("[MESSENGER] Akses ditolak dari chat ID: {}", msg.chat.id.0);
                return Ok(());
            }

            match cmd {
                Command::Help => {
                    bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
                }

                Command::Start => {
                    bot.send_message(
                        msg.chat.id,
                        "🤖 <b>HFT Bot Trading Aktif!</b>\n\n\
                        Gunakan /status untuk melihat kondisi pasar.\n\
                        Gunakan /scan untuk top peluang altcoin saat ini.\n\
                        Gunakan /paper atau /live untuk ganti mode.",
                    )
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;
                }

                Command::Status => {
                    let s = state_clone.read().await;

                    let conn_status = if s.is_connected { "🟢 Connected" } else { "🔴 Disconnected" };
                    let mode_text = match s.trading_mode {
                        crate::engine::state::TradingMode::Paper => "🛡️ PAPER (Simulasi)",
                        crate::engine::state::TradingMode::Live  => "⚔️ LIVE (Uang Asli!)",
                    };

                    let (btc_trend_label, btc_change_text) = build_btc_trend_text(&s);
                    let top_opps = build_top_opportunities(&s, 15);
                    let total = s.market_data.len();
                    let hot = s.market_data.values().filter(|d| d.is_whale_alert).count();

                    let status_msg = format!(
                        "📊 <b>STATUS MARKET SCANNER:</b>\n\n\
                        🔌 Exchange: <i>{conn_status}</i>\n\
                        🎮 Mode: <b>{mode_text}</b>\n\n\
                        ₿ <b>BTC Trend:</b> {btc_trend_label}{btc_change_text}\n\n\
                        🔥 <b>Top 15 Peluang (by Z-Score):</b>\n{top_opps}\n\
                        📡 Dipantau: <b>{total} koin</b> | 🚨 Surge: <b>{hot}</b>"
                    );

                    bot.send_message(msg.chat.id, status_msg)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .await?;
                }

                Command::Scan => {
                    let msg_text = {
                        let s = state_clone.read().await;
                        build_scan_report(&s)
                    };
                    bot.send_message(msg.chat.id, msg_text)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .await?;
                }

                Command::Ping => {
                    bot.send_message(msg.chat.id, "🏓 Pong! Server merespons normal.").await?;
                }

                Command::Paper => {
                    let mut s = state_clone.write().await;
                    s.trading_mode = crate::engine::state::TradingMode::Paper;
                    bot.send_message(
                        msg.chat.id,
                        "🛡️ <b>Mode: PAPER TRADING</b>\nBot berjalan dalam simulasi. Tidak ada uang asli yang digunakan.",
                    )
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;
                }

                Command::Live => {
                    let mut s = state_clone.write().await;
                    s.trading_mode = crate::engine::state::TradingMode::Live;
                    bot.send_message(
                        msg.chat.id,
                        "⚔️ <b>⚠️ WARNING: LIVE TRADING AKTIF!</b>\nBot sekarang menggunakan <b>UANG ASLI (Rp500.000)</b>.\nPastikan kondisi pasar kondusif!",
                    )
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;
                }
            }

            Ok(())
        }
    })
    .await;

    Ok(())
}

// ─── Helper: teks BTC trend + perubahan persen ────────────────────────────────

fn build_btc_trend_text(s: &BotState) -> (&'static str, String) {
    let label = match s.btc_trend {
        BtcTrend::Bullish => "🟢 BULLISH — Alt siap beli!",
        BtcTrend::Bearish => "🔴 BEARISH — Mode Defensif Aktif",
        BtcTrend::Neutral => "🟡 NEUTRAL — Selektif",
    };
    let change = s.btc_change_pct
        .map(|c| format!(" ({:+.2}%/5m)", c))
        .unwrap_or_default();
    (label, change)
}

// ─── Helper: Top N koin berdasarkan Z-Score tertinggi ─────────────────────────

fn build_top_opportunities(s: &BotState, limit: usize) -> String {
    if s.market_data.is_empty() {
        return "<i>Menunggu data market pertama...</i>".to_string();
    }

    let mut symbols: Vec<_> = s.market_data.iter().collect();
    // Urutkan Z-Score dari tertinggi ke terendah (peluang terbesar duluan)
    symbols.sort_by(|a, b| {
        let z_a = a.1.current_z_score.unwrap_or_default();
        let z_b = b.1.current_z_score.unwrap_or_default();
        z_b.cmp(&z_a)
    });

    let mut out = String::new();
    for (sym, data) in symbols.iter().take(limit) {
        let price = data.last_price
            .map_or("N/A".to_string(), |p| format!("Rp {:.2}", p));
        let z = data.current_z_score
            .map_or("N/A".to_string(), |z| format!("{:.2}", z));
        let obi = data.order_book_imbalance
            .map_or("N/A".to_string(), |o| format!("{:.2}", o));
        let surge = if data.is_whale_alert { " 🚨" } else { "" };
        out.push_str(&format!(
            "🔹 <b>{}</b>: {} | Z: {} | OBI: {}{}\n",
            sym, price, z, obi, surge
        ));
    }
    out
}

// ─── Laporan scan lengkap untuk /scan ─────────────────────────────────────────

fn build_scan_report(s: &BotState) -> String {
    let ts = Utc::now().format("%H:%M:%S UTC").to_string();
    let (btc_label, btc_change) = build_btc_trend_text(s);
    let top = build_top_opportunities(s, 20);
    let total = s.market_data.len();
    let hot = s.market_data.values().filter(|d| d.is_whale_alert).count();

    format!(
        "🔭 <b>MARKET SCAN — Top Peluang:</b>\n\n\
        ₿ <b>BTC Trend:</b> {btc_label}{btc_change}\n\n\
        🏆 <b>Top 20 Koin (by Z-Score):</b>\n{top}\n\
        🚨 Surge aktif: <b>{hot}/{total}</b> koin\n\
        🕐 {ts}"
    )
}

// ─── Laporan auto-scan singkat setiap 5 menit ─────────────────────────────────

fn build_auto_scan_report(s: &BotState) -> String {
    let ts = Utc::now().format("%H:%M:%S UTC").to_string();
    let (btc_label, btc_change) = build_btc_trend_text(s);
    let status_line = match s.btc_trend {
        BtcTrend::Bullish => "✅ Siap entry altcoin!",
        BtcTrend::Bearish => "🛡️ Mode Defensif — skip altcoin",
        BtcTrend::Neutral => "⚖️ Selektif — butuh konfirmasi kuat",
    };
    let top = build_top_opportunities(s, 5);
    let total = s.market_data.len();
    let hot = s.market_data.values().filter(|d| d.is_whale_alert).count();

    format!(
        "🔄 <b>AUTO-SCAN (5m) — {ts}</b>\n\n\
        ₿ BTC: {btc_label}{btc_change}\n\
        → {status_line}\n\n\
        🔥 <b>Top 5 Peluang:</b>\n{top}\n\
        📊 {total} koin dipantau | 🚨 {hot} surge"
    )
}

// ─── Format TradeReport → pesan Telegram ──────────────────────────────────────

fn format_trade_report(report: &TradeReport) -> String {
    let ts = report.timestamp.format("%H:%M:%S UTC").to_string();

    match report.report_type {
        ReportType::Entry => format!(
            "🚀 <b>ENTRY SIGNAL - BOT MEMBELI!</b>\n\n\
            - Pair: <b>{}</b>\n\
            - Harga Beli: <b>Rp {:.0}</b>\n\
            - Quantity: {:.8}\n\
            - Modal Aktif: Rp {:.0}\n\
            - ⏰ Waktu: {}",
            report.symbol,
            report.executed_price,
            report.quantity,
            report.equity_idr.unwrap_or_default(),
            ts
        ),

        ReportType::TakeProfit => format!(
            "🎉 <b>TAKE PROFIT! BOT UNTUNG!</b>\n\n\
            - Pair: <b>{}</b>\n\
            - Harga Jual: <b>Rp {:.0}</b>\n\
            - 💰 PnL: <b>+Rp {:.0}</b>\n\
            - Equity Baru: Rp {:.0}\n\
            - ⏰ Waktu: {}",
            report.symbol,
            report.executed_price,
            report.pnl_idr.unwrap_or_default(),
            report.equity_idr.unwrap_or_default(),
            ts
        ),

        ReportType::StopLoss | ReportType::TrailingStop => {
            let label = match report.report_type {
                ReportType::StopLoss     => "🔴 STOP LOSS",
                ReportType::TrailingStop => "🟡 TRAILING STOP",
                _                        => "EXIT",
            };
            let pnl = report.pnl_idr.unwrap_or_default();
            let pnl_str = if pnl >= rust_decimal::Decimal::ZERO {
                format!("+Rp {:.0}", pnl)
            } else {
                format!("-Rp {:.0}", pnl.abs())
            };
            format!(
                "{} - <b>BOT KELUAR POSISI</b>\n\n\
                - Pair: <b>{}</b>\n\
                - Harga Jual: <b>Rp {:.0}</b>\n\
                - 📉 PnL: <b>{}</b>\n\
                - Equity Sisa: Rp {:.0}\n\
                - ⏰ Waktu: {}",
                label,
                report.symbol,
                report.executed_price,
                pnl_str,
                report.equity_idr.unwrap_or_default(),
                ts
            )
        }
    }
}
