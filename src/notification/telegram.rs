use std::env;
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::engine::state::SharedState;
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
    #[command(description = "Tes koneksi server.")]
    Ping,
    #[command(description = "Mengubah mode ke PAPER TRADING (Simulasi).")]
    Paper,
    #[command(description = "Mengubah mode ke LIVE TRADING (Uang Asli!).")]
    Live,
}

/// The Messenger: menjalankan 2 sub-task secara konkuren.
/// 1. Command handler — menerima perintah dari admin Telegram
/// 2. Report listener — menerima TradeReport dari Guardian dan push notifikasi
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

    // Sub-task 1: Push notifikasi dari Guardian (TradeReport) ke Telegram
    let bot_for_push = bot.clone();
    tokio::spawn(async move {
        while let Some(report) = rx_report.recv().await {
            let chat_id = ChatId(admin_chat_id);
            let message = format_trade_report(&report);

            if let Err(e) = bot_for_push
                .send_message(chat_id, &message)
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

    // Sub-task 2: Command handler (blocking — ini yang menahan task ini berjalan)
    Command::repl(bot, move |bot: Bot, msg: Message, cmd: Command| {
        let admin_id_str = admin_chat_id_str.clone();
        let state_clone = state.clone();

        async move {
            // KEAMANAN: hanya Admin yang bisa kontrol bot
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
                        "🤖 <b>HFT Bot Trading Aktif!</b>\n\nGunakan /status untuk melihat kondisi pasar.\nGunakan /paper atau /live untuk ganti mode.",
                    )
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;
                }

                Command::Status => {
                    let s = state_clone.read().await;

                    let conn_status  = if s.is_connected { "🟢 Connected" } else { "🔴 Disconnected" };
                    let price_text   = s.last_price.map_or("Menunggu...".into(), |p| format!("Rp {}", p));
                    let atr_text     = s.current_atr.map_or("Menghitung...".into(), |a| format!("Rp {:.0}", a));
                    let z_text       = s.current_z_score.map_or("Menghitung...".into(), |z| format!("{:.2}", z));
                    let whale_alert  = if s.is_whale_alert { "⚠️ AKTIF (Volume Surge!)" } else { "✅ Normal" };
                    let mode_text    = match s.trading_mode {
                        crate::engine::state::TradingMode::Paper => "🛡️ PAPER (Simulasi)",
                        crate::engine::state::TradingMode::Live  => "⚔️ LIVE (Uang Asli!)",
                    };

                    let status_msg = format!(
                        "📊 <b>Status Sistem (BTCBIDR):</b>\n\n\
                        - Koneksi Exchange: <i>{}</i>\n\
                        - Data Candle: {} / 50\n\
                        - Harga Terakhir: <b>{}</b>\n\
                        - 🛡️ ATR(14): {}\n\
                        - 🐋 Z-Score(20): {}\n\
                        - Whale Alert: {}\n\n\
                        - 🕹️ Mode: <b>{}</b>\n\
                        - 💰 Modal: Rp 500.000\n\
                        - 🎯 Posisi: Flat",
                        conn_status, s.total_candles, price_text, atr_text, z_text, whale_alert, mode_text
                    );

                    bot.send_message(msg.chat.id, status_msg)
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

/// Format TradeReport menjadi pesan Telegram yang informatif dan mudah dibaca.
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
                ReportType::StopLoss    => "🔴 STOP LOSS",
                ReportType::TrailingStop => "🟡 TRAILING STOP",
                _ => "EXIT",
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
