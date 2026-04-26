use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use tracing::{info, error};

pub async fn init_db() -> anyhow::Result<Pool<Sqlite>> {
    info!("Menyiapkan Pustakawan Database SQLite...");
    
    // Create DB file if it doesn't exist
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite:history.db?mode=rwc").await?;
        
    // Create table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS klines (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            symbol TEXT NOT NULL,
            close REAL NOT NULL,
            volume REAL NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#
    )
    .execute(&pool)
    .await?;

    info!("Database SQLite siap menerima arsip!");
    Ok(pool)
}

pub async fn save_kline(pool: &Pool<Sqlite>, symbol: &str, close: f64, volume: f64) {
    let result = sqlx::query(
        "INSERT INTO klines (symbol, close, volume) VALUES (?, ?, ?)"
    )
    .bind(symbol)
    .bind(close)
    .bind(volume)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Gagal mengarsipkan data ke SQLite: {}", e);
    }
}

pub async fn cleanup_old_data(pool: &Pool<Sqlite>) {
    // Menghapus data yang usianya lebih dari 30 hari
    let result = sqlx::query(
        "DELETE FROM klines WHERE created_at < datetime('now', '-30 days')"
    )
    .execute(pool)
    .await;

    match result {
        Ok(res) => {
            let deleted = res.rows_affected();
            if deleted > 0 {
                info!("🧹 Membakar {} baris data usang dari SQLite (Archive Cleanup)!", deleted);
            }
        }
        Err(e) => {
            error!("Gagal melakukan cleanup SQLite: {}", e);
        }
    }
}
