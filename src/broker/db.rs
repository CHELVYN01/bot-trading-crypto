use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tracing::{info, error};
use rust_decimal::Decimal;
use chrono::{DateTime, Utc};

pub async fn init_db(database_url: &str) -> anyhow::Result<Pool<Postgres>> {
    info!("🚀 Menyiapkan Pustakawan Database PostgreSQL...");
    
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url).await?;
        
    // Create table with Decimal precision
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS klines (
            id SERIAL PRIMARY KEY,
            symbol TEXT NOT NULL,
            open_price NUMERIC NOT NULL,
            high_price NUMERIC NOT NULL,
            low_price NUMERIC NOT NULL,
            close_price NUMERIC NOT NULL,
            volume NUMERIC NOT NULL,
            created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
        )
        "#
    )
    .execute(&pool)
    .await?;

    info!("✅ Database PostgreSQL siap! Ingatan bot kini jauh lebih kuat.");
    Ok(pool)
}

pub async fn save_kline(
    pool: &Pool<Postgres>, 
    symbol: &str, 
    open: Decimal,
    high: Decimal,
    low: Decimal,
    close: Decimal, 
    volume: Decimal
) {
    let result = sqlx::query(
        "INSERT INTO klines (symbol, open_price, high_price, low_price, close_price, volume) VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(symbol)
    .bind(open)
    .bind(high)
    .bind(low)
    .bind(close)
    .bind(volume)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("❌ Gagal mengarsipkan data ke PostgreSQL: {}", e);
    }
}

pub async fn cleanup_old_data(pool: &Pool<Postgres>) {
    // Menghapus data yang usianya lebih dari 7 hari (agar DB tidak bengkak di VPS murah)
    let result = sqlx::query(
        "DELETE FROM klines WHERE created_at < NOW() - INTERVAL '7 days'"
    )
    .execute(pool)
    .await;

    match result {
        Ok(res) => {
            let deleted = res.rows_affected();
            if deleted > 0 {
                info!("🧹 Cleanup: {} baris data usang telah dihapus dari PostgreSQL.", deleted);
            }
        }
        Err(e) => {
            error!("❌ Gagal melakukan cleanup PostgreSQL: {}", e);
        }
    }
}
