use std::env;
use teloxide::{prelude::*, utils::command::BotCommands};
use tracing::{info, warn};

use crate::engine::state::SharedState;

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
}

pub async fn run_telegram_bot(state: SharedState) -> anyhow::Result<()> {
    // Membaca konfigurasi dari .env
    let token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN wajib diisi di .env");
    let admin_chat_id = env::var("TELEGRAM_ADMIN_CHAT_ID").expect("TELEGRAM_ADMIN_CHAT_ID wajib diisi di .env");

    let bot = Bot::new(token);

    info!("Memulai Telegram Bot Handler...");

    // Gunakan Command::repl untuk meng-handle perintah secara asinkron
    Command::repl(bot, move |bot: Bot, msg: Message, cmd: Command| {
        let admin_chat_id = admin_chat_id.clone();
        let state_clone = state.clone();
        
        async move {
            // KEAMANAN TINGKAT TINGGI:
            // Cek apakah pengirim pesan adalah Admin yang sah (berdasarkan Chat ID di .env)
            // Jika bukan, abaikan pesannya sepenuhnya tanpa peringatan ke pengirim (stealth).
            if msg.chat.id.0.to_string() != admin_chat_id {
                warn!("Akses ditolak! Pesan masuk dari chat ID tidak dikenal: {}", msg.chat.id.0);
                return Ok(());
            }

            // Memproses perintah
            match cmd {
                Command::Help => {
                    bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
                }
                Command::Start => {
                    bot.send_message(
                        msg.chat.id, 
                        "🤖 <b>HFT Bot Trading Aktif!</b>\n\nSistem terkunci pada mode aman.\nGunakan /status untuk melihat kondisi pasar.",
                    ).parse_mode(teloxide::types::ParseMode::Html).await?;
                }
                Command::Status => {
                    let s = state_clone.read().await;
                    
                    let conn_status = if s.is_connected { "🟢 Connected" } else { "🔴 Pending" };
                    let price_text = match s.last_price {
                        Some(p) => format!("Rp {}", p),
                        None => "Menunggu data...".to_string(),
                    };
                    let atr_text = match s.current_atr {
                        Some(a) => format!("Rp {:.2}", a),
                        None => "Menghitung...".to_string(),
                    };
                    let z_text = match s.current_z_score {
                        Some(z) => format!("{:.2}", z),
                        None => "Menghitung...".to_string(),
                    };
                    let whale_alert = if s.is_whale_alert { "⚠️ AKTIF (CUKONG MASUK)" } else { "Aman" };

                    let status_msg = format!(
                        "📊 <b>Status Sistem (BTCBIDR):</b>\n\n\
                        - Koneksi Exchange: <i>{}</i>\n\
                        - Data Tersimpan: {} / 20 candle\n\
                        - Harga Terakhir: <b>{}</b>\n\
                        - 🛡️ ATR(14): {}\n\
                        - 🐋 Z-Score(20): {}\n\
                        - Whale Alert: {}\n\n\
                        - 💰 Modal Aktif: Rp 500.000\n\
                        - 🎯 Posisi: Flat (Tidak ada)",
                        conn_status, s.total_candles, price_text, atr_text, z_text, whale_alert
                    );

                    bot.send_message(msg.chat.id, status_msg).parse_mode(teloxide::types::ParseMode::Html).await?;
                }
                Command::Ping => {
                    bot.send_message(msg.chat.id, "🏓 Pong! Latensi sistem dalam batas toleransi.").await?;
                }
            };
            
            Ok(())
        }
    })
    .await;

    Ok(())
}
