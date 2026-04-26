# Architecture & Strategy Blueprint - Crypto Trading Bot (Rust)

## 1. Core Philosophy
The bot acts as a **Deterministic AI** (Expert System) that executes trades based on strict mathematical rules without emotional bias. It prioritizes **Capital Preservation** and **Data-Driven Execution**.

## 2. Infrastructure Stack
- **Language:** Rust (Edition 2021) for low-latency and memory safety.
- **Runtime:** Tokio (Asynchronous execution).
- **Database:** PostgreSQL (Replacing SQLite for better scaling and decimal precision).
- **Interface:** Telegram (via Teloxide) for real-time reporting and command & control.
- **Exchange:** Tokocrypto (Indonesian Exchange, Binance Cloud based).

## 3. Trading Strategy: "Leader & Follower + Outlier Hunter"
### Phase A: Market Filter (The Leader)
- **Monitoring:** BTC/BIDR or BTC/USDT on Tokocrypto.
- **Logic:** If BTC is in a heavy dump (negative Z-Score or breaking major EMA), the bot enters "Defensive Mode" (stops buying Altcoins).

### Phase B: Signal Detection (The Follower)
- **Scanning:** Top 50-100 Altcoins by Volume.
- **Primary Trigger:** 
    - **Volume Surge:** Z-Score of Volume > 2.5 (Normal) or > 4.0 (If BTC is bearish).
    - **Price Momentum:** Breaking 20-period High.
- **Relative Strength:** Identifying coins that move up or stay flat while BTC moves down.

### Phase C: Risk Management
- **Entry:** Market execution when signals align.
- **Stop Loss:** Dynamic **ATR 1.5x**.
- **Take Profit:** Risk/Reward Ratio 1:2 or Trailing Stop after 3% gain.
- **Precision:** Use `rust_decimal` for all financial calculations.

## 4. Database Functionality (The Memory)
- **Contextual Analysis:** Calculating moving averages and Z-Scores based on historical data.
- **Audit Log:** Every trade is recorded for weekly human review.
- **Self-Optimization:** Identifying high-performing vs low-performing coins over time.

## 5. Deployment Specs (Budget: Rp500,000)
- **Recommended:** 2 vCPU / 2GB RAM VPS (Location: Singapore or Jakarta for low latency to Tokocrypto).
- **OS:** Linux (managed via systemd).
