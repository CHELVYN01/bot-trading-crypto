use crate::broker::model::Kline;
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromStr;
use tracing::{info, error};

const REST_API_URL: &str = "https://api.binance.com/api/v3/klines";

pub async fn fetch_historical_klines(symbol: &str, limit: usize) -> anyhow::Result<Vec<Kline>> {
    info!("Menyusup ke server Tokocrypto untuk mengambil {} data sejarah terakhir...", limit);
    let client = Client::new();
    let url = format!("{}?symbol={}&interval=1m&limit={}", REST_API_URL, symbol.to_uppercase(), limit);
    
    let res = client.get(&url).send().await?;
    
    if !res.status().is_success() {
        error!("Gagal mengambil data dari REST API: {}", res.status());
        return Err(anyhow::anyhow!("REST API Error"));
    }
    
    let text = res.text().await?;
    let data: Vec<Vec<serde_json::Value>> = serde_json::from_str(&text)?;
    
    let mut klines = Vec::new();
    for row in data {
        // Format Binance/Tokocrypto API:
        // [
        //   0: Open time,
        //   1: Open,
        //   2: High,
        //   3: Low,
        //   4: Close,
        //   5: Volume,
        //   6: Close time,
        //   ...
        // ]
        if row.len() >= 7 {
            let start_time = row[0].as_u64().unwrap_or(0);
            let open_str = row[1].as_str().unwrap_or("0");
            let high_str = row[2].as_str().unwrap_or("0");
            let low_str = row[3].as_str().unwrap_or("0");
            let close_str = row[4].as_str().unwrap_or("0");
            let vol_str = row[5].as_str().unwrap_or("0");
            let end_time = row[6].as_u64().unwrap_or(0);
            
            let kline = Kline {
                start_time,
                end_time,
                interval: "1m".to_string(),
                open: Decimal::from_str(open_str).unwrap_or(Decimal::ZERO),
                close: Decimal::from_str(close_str).unwrap_or(Decimal::ZERO),
                high: Decimal::from_str(high_str).unwrap_or(Decimal::ZERO),
                low: Decimal::from_str(low_str).unwrap_or(Decimal::ZERO),
                volume: Decimal::from_str(vol_str).unwrap_or(Decimal::ZERO),
                is_final: true, // Data historis pasti sudah final
            };
            klines.push(kline);
        }
    }
    
    info!("Berhasil menyedot {} data sejarah dalam hitungan milidetik!", klines.len());
    Ok(klines)
}
