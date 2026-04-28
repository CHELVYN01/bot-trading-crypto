use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info};

type HmacSha256 = Hmac<Sha256>;

/// Base URL REST API Tokocrypto untuk order yang memerlukan autentikasi
const TOKO_REST_BASE: &str = "https://www.tokocrypto.com";

/// Hasil eksekusi order dari exchange.
/// Berisi detail konfirmasi yang dikembalikan oleh Tokocrypto API.
#[derive(Debug)]
pub struct OrderResult {
    /// Order ID dari exchange (untuk audit log)
    pub order_id: u64,

    /// Simbol yang diperdagangkan
    pub symbol: String,

    /// Harga rata-rata eksekusi aktual
    pub executed_price: Decimal,

    /// Quantity yang berhasil terisi (filled)
    pub executed_qty: Decimal,
}

/// Mengambil timestamp Unix dalam milidetik.
/// Wajib disertakan di setiap signed request ke Tokocrypto.
fn get_timestamp_ms() -> Result<u64> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("Gagal mendapatkan waktu sistem")?
        .as_millis() as u64;
    Ok(ts)
}

/// Membuat tanda tangan HMAC-SHA256 dari query string.
/// Ini adalah mekanisme keamanan wajib Tokocrypto untuk semua endpoint trading.
fn sign_query(secret_key: &str, query: &str) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
        .context("Gagal inisialisasi HMAC dengan secret key")?;
    mac.update(query.as_bytes());
    let result = mac.finalize();
    Ok(hex::encode(result.into_bytes()))
}

/// Mengeksekusi order BUY Market ke Tokocrypto.
/// Budget dalam IDR akan dikonversi ke quantity aset secara otomatis.
///
/// # Arguments
/// * `api_key` - API Key dari .env
/// * `secret_key` - Secret Key dari .env
/// * `symbol` - Contoh: "BTCBIDR"
/// * `budget_idr` - Jumlah IDR yang akan dibelanjakan (contoh: 500000)
///
/// # Returns
/// `OrderResult` berisi konfirmasi dari exchange, atau error jika gagal.
pub async fn place_buy_order(
    api_key: &str,
    secret_key: &str,
    symbol: &str,
    budget_idr: Decimal,
) -> Result<OrderResult> {
    info!(
        "🛒 Mengeksekusi BUY ORDER: {} | Budget: Rp {}",
        symbol, budget_idr
    );

    let client = Client::new();
    let timestamp = get_timestamp_ms()?;

    // Tokocrypto (Binance-compatible): quoteOrderQty untuk market buy berdasarkan IDR
    let budget_str = budget_idr
        .to_f64()
        .context("Gagal konversi budget ke f64")?;

    let query = format!(
        "symbol={}&side=BUY&type=MARKET&quoteOrderQty={:.0}&timestamp={}",
        symbol.to_uppercase(),
        budget_str,
        timestamp
    );

    let signature = sign_query(secret_key, &query)?;
    let full_query = format!("{}&signature={}", query, signature);

    let url = format!("{}/open/v1/orders?{}", TOKO_REST_BASE, full_query);

    let response = client
        .post(&url)
        .header("X-MBX-APIKEY", api_key)
        .send()
        .await
        .context("Gagal mengirim request BUY ke Tokocrypto")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("Gagal membaca response body")?;

    if !status.is_success() {
        error!(
            "❌ BUY ORDER GAGAL! Status: {} | Body: {}",
            status, body
        );
        return Err(anyhow::anyhow!(
            "Tokocrypto menolak BUY order. Status: {}, Body: {}",
            status,
            body
        ));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).context("Gagal parse response JSON dari BUY order")?;

    let order_id = json["orderId"]
        .as_u64()
        .context("Field 'orderId' tidak ditemukan di response")?;
    let executed_qty_str = json["executedQty"]
        .as_str()
        .context("Field 'executedQty' tidak ditemukan")?;
    let avg_price_str = json["fills"][0]["price"]
        .as_str()
        .unwrap_or("0");

    let executed_qty = executed_qty_str
        .parse::<Decimal>()
        .context("Gagal parse executedQty")?;
    let executed_price = avg_price_str
        .parse::<Decimal>()
        .unwrap_or(Decimal::ZERO);

    info!(
        "✅ BUY ORDER SUKSES! OrderID: {} | Qty: {} {} | Harga: Rp {}",
        order_id, executed_qty, symbol, executed_price
    );

    Ok(OrderResult {
        order_id,
        symbol: symbol.to_uppercase(),
        executed_price,
        executed_qty,
    })
}

/// Mengeksekusi order SELL Market ke Tokocrypto.
/// Menjual seluruh quantity yang dipegang.
///
/// # Arguments
/// * `api_key` - API Key dari .env
/// * `secret_key` - Secret Key dari .env
/// * `symbol` - Contoh: "BTCBIDR"
/// * `quantity` - Quantity aset yang akan dijual (hasil dari BUY sebelumnya)
///
/// # Returns
/// `OrderResult` berisi konfirmasi dari exchange.
pub async fn place_sell_order(
    api_key: &str,
    secret_key: &str,
    symbol: &str,
    quantity: Decimal,
) -> Result<OrderResult> {
    info!(
        "💰 Mengeksekusi SELL ORDER: {} | Qty: {}",
        symbol, quantity
    );

    let client = Client::new();
    let timestamp = get_timestamp_ms()?;

    // Format quantity: hapus trailing zero agar tidak ditolak exchange
    let qty_str = format!("{}", quantity.normalize());

    let query = format!(
        "symbol={}&side=SELL&type=MARKET&quantity={}&timestamp={}",
        symbol.to_uppercase(),
        qty_str,
        timestamp
    );

    let signature = sign_query(secret_key, &query)?;
    let full_query = format!("{}&signature={}", query, signature);

    let url = format!("{}/open/v1/orders?{}", TOKO_REST_BASE, full_query);

    let response = client
        .post(&url)
        .header("X-MBX-APIKEY", api_key)
        .send()
        .await
        .context("Gagal mengirim request SELL ke Tokocrypto")?;

    let status = response.status();
    let body = response
        .text()
        .await
        .context("Gagal membaca response body")?;

    if !status.is_success() {
        error!(
            "❌ SELL ORDER GAGAL! Status: {} | Body: {}",
            status, body
        );
        return Err(anyhow::anyhow!(
            "Tokocrypto menolak SELL order. Status: {}, Body: {}",
            status,
            body
        ));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body).context("Gagal parse response JSON dari SELL order")?;

    let order_id = json["orderId"]
        .as_u64()
        .context("Field 'orderId' tidak ditemukan di response")?;
    let executed_qty_str = json["executedQty"]
        .as_str()
        .context("Field 'executedQty' tidak ditemukan")?;
    let avg_price_str = json["fills"][0]["price"]
        .as_str()
        .unwrap_or("0");

    let executed_qty = executed_qty_str
        .parse::<Decimal>()
        .context("Gagal parse executedQty")?;
    let executed_price = avg_price_str
        .parse::<Decimal>()
        .unwrap_or(Decimal::ZERO);

    info!(
        "✅ SELL ORDER SUKSES! OrderID: {} | Qty: {} | Harga: Rp {}",
        order_id, executed_qty, executed_price
    );

    Ok(OrderResult {
        order_id,
        symbol: symbol.to_uppercase(),
        executed_price,
        executed_qty,
    })
}

/// Mengambil saldo IDR terkini dari akun Tokocrypto.
/// Digunakan Guardian untuk verifikasi saldo sebelum entry.
pub async fn get_idr_balance(api_key: &str, secret_key: &str) -> Result<Decimal> {
    let client = Client::new();
    let timestamp = get_timestamp_ms()?;

    let query = format!("timestamp={}", timestamp);
    let signature = sign_query(secret_key, &query)?;
    let full_query = format!("{}&signature={}", query, signature);

    let url = format!("{}/open/v1/account/spot?{}", TOKO_REST_BASE, full_query);

    let response = client
        .get(&url)
        .header("X-MBX-APIKEY", api_key)
        .send()
        .await
        .context("Gagal request saldo ke Tokocrypto")?;

    let body = response
        .text()
        .await
        .context("Gagal membaca response saldo")?;

    let json: serde_json::Value =
        serde_json::from_str(&body).context("Gagal parse response JSON saldo")?;

    // Cari asset IDR di dalam array balances
    if let Some(balances) = json["balances"].as_array() {
        for balance in balances {
            if balance["asset"].as_str() == Some("BIDR") || balance["asset"].as_str() == Some("IDR") {
                let free_str = balance["free"].as_str().unwrap_or("0");
                let free = free_str.parse::<Decimal>().unwrap_or(Decimal::ZERO);
                info!("💳 Saldo IDR tersedia: Rp {}", free);
                return Ok(free);
            }
        }
    }

    Err(anyhow::anyhow!("Saldo IDR/BIDR tidak ditemukan di akun"))
}
