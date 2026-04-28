# Architecture & Strategy Blueprint - Crypto Trading Bot (Rust)

## 1. Core Philosophy
The bot acts as a **Deterministic AI** (Expert System) that executes trades based on strict
mathematical rules without emotional bias. It prioritizes **Capital Preservation** and
**Data-Driven Execution**. Budget aktif: **Rp 500.000**.

---

## 2. Infrastructure Stack

| Komponen      | Teknologi                              |
|---------------|----------------------------------------|
| Language      | Rust (Edition 2021) — zero-cost, safe  |
| Runtime       | Tokio (Async, multi-task)              |
| Communication | `tokio::sync::mpsc` channels           |
| Database      | PostgreSQL (via `sqlx`)                |
| Interface     | Telegram (via `teloxide`)              |
| Exchange      | Tokocrypto (Binance Cloud Based)       |
| Deployment    | Linux systemd / Docker                 |

---

## 3. Arsitektur: 4-Task Pipeline

Semua task berjalan **konkuren** dalam 1 binary menggunakan `tokio::spawn`.
Komunikasi antar-task menggunakan **Tokio MPSC Channels** (bukan shared memory).

```
[Tokocrypto WebSocket]
        │  Raw Kline Data
        ▼
┌───────────────────┐
│  Task 1           │  Beban: Tinggi (I/O Bound)
│  THE LISTENER     │  File: src/engine/websocket.rs
│  (WebSocket)      │  ─────────────────────────────
│                   │  • Konek ke WS Tokocrypto
│                   │  • Parse JSON → Kline struct
│                   │  • Kirim ke Strategist via mpsc
└────────┬──────────┘
         │  tx_kline: Sender<Kline>
         ▼
┌───────────────────┐
│  Task 2           │  Beban: Sedang (CPU Bound)
│  THE STRATEGIST   │  File: src/strategy/scanner.rs
│  (Brain)          │  ─────────────────────────────
│                   │  • Hitung Z-Score Volume (>2.5)
│                   │  • Hitung ATR(14) Stop Loss
│                   │  • Hitung RSI(14)
│                   │  • Jika sinyal valid → kirim Signal
└────────┬──────────┘
         │  tx_signal: Sender<TradeSignal>
         ▼
┌───────────────────┐
│  Task 3           │  Beban: Ringan tapi KRITIS
│  THE GUARDIAN     │  File: src/engine/guardian.rs
│  (Risk Manager)   │  ─────────────────────────────
│                   │  • Terima signal ENTRY dari Strategist
│                   │  • Eksekusi order BUY via REST API
│                   │  • Pantau harga vs Trailing Stop Loss
│                   │  • Eksekusi order SELL jika SL tersentuh
│                   │  • Kirim laporan ke Messenger
└────────┬──────────┘
         │  tx_report: Sender<TradeReport>
         ▼
┌───────────────────┐
│  Task 4           │  Beban: Sangat Ringan
│  THE MESSENGER    │  File: src/notification/telegram.rs
│  (Telegram)       │  ─────────────────────────────
│                   │  • Terima laporan dari Guardian
│                   │  • Kirim notifikasi ke Telegram
│                   │  • Terima command dari user (/status, /live)
└───────────────────┘
```

---

## 4. Trading Strategy: "Leader & Follower + Outlier Hunter"

### Phase A: Market Filter (BTC sebagai Leader)
- Monitor `BTCBIDR` sebagai indikator market global.
- Jika BTC dump berat (Z-Score negatif / break EMA bawah) → **Defensive Mode** (stop beli altcoin).

### Phase B: Signal Detection (Altcoin sebagai Follower)
- Scan Top Altcoin by Volume.
- **Entry Trigger:**
  - Volume Surge: Z-Score > 2.5 (normal) atau > 4.0 (jika BTC bearish)
  - Price Momentum: Break 20-period High
  - Relative Strength: Naik saat BTC turun

### Phase C: Risk Management (Guardian)
- **Entry:** Market Order saat semua sinyal aligned.
- **Stop Loss:** Dynamic `ATR × 1.5` (dihitung ulang tiap candle).
- **Take Profit:** Risk:Reward 1:2 atau Trailing Stop setelah gain 3%.

---

## 5. Data Flow & Channel Types

```rust
// Channel definitions (src/main.rs)
let (tx_kline, rx_kline)   = mpsc::channel::<Kline>(100);        // Listener → Strategist
let (tx_signal, rx_signal) = mpsc::channel::<TradeSignal>(10);   // Strategist → Guardian
let (tx_report, rx_report) = mpsc::channel::<TradeReport>(10);   // Guardian → Messenger
```

---

## 6. Status Implementasi

| Task             | File                          | Status         |
|------------------|-------------------------------|----------------|
| The Listener     | `src/engine/websocket.rs`     | ✅ Done         |
| The Strategist   | `src/strategy/scanner.rs`     | ⚠️ Partial (Z-Score + ATR ada, RSI belum) |
| The Guardian     | `src/engine/guardian.rs`      | ❌ Belum ada    |
| The Messenger    | `src/notification/telegram.rs`| ⚠️ Partial (command ada, push notif belum)|
| Order Execution  | `src/broker/order.rs`         | ❌ Belum ada    |

**Next Step: Implementasi `broker/order.rs` + `engine/guardian.rs`**

---

## 7. Deployment Specs

- **VPS:** 2 vCPU / 2GB RAM (Singapura atau Jakarta — dekat server Tokocrypto)
- **OS:** Linux Ubuntu 22.04 LTS
- **Process Manager:** systemd atau Docker Compose
- **Budget Modal:** Rp 500.000 (dikelola oleh Guardian, tidak hardcoded)
