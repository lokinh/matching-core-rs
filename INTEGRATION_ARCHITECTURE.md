# Kiến trúc Tích hợp Matching Engine

Tài liệu này mô tả cách các service khác tích hợp với matching engine.

## Tổng quan

Matching engine đã xử lý **TẤT CẢ** logic nghiệp vụ:
- ✅ Khớp lệnh (Matching)
- ✅ Quản lý rủi ro (Risk Management)
- ✅ Quản lý số dư (Balance Management)
- ✅ **Settlement (Thanh toán)** - Tự động thanh toán cho Taker và Maker
- ✅ **Statistics (Thống kê)** - Tính toán market data và thống kê giao dịch
- ✅ Xử lý nhiều loại lệnh (GTC, IOC, FOK, Iceberg, Stop, GTD)
- ✅ Hỗ trợ nhiều pair và multi-currency
- ✅ Journaling và Snapshot (độ tin cậy)

**Các service khác chỉ cần:**
1. **Lưu vào Database** - Lưu trữ lệnh, giao dịch, số dư, settlement, statistics
2. **Gửi qua WebSocket** - Thông báo real-time cho clients
3. **Tính toán Statistics** - Tổng hợp thống kê từ dữ liệu giao dịch

## Kiến trúc Tích hợp

```
┌─────────────────────────────────────────────────────────┐
│                    Client Applications                    │
│              (Web, Mobile, API Clients)                  │
└──────────────────────┬────────────────────────────────────┘
                       │ WebSocket / HTTP
                       │
┌──────────────────────▼────────────────────────────────────┐
│              API Gateway / WebSocket Server                │
│  - Nhận lệnh từ clients                                    │
│  - Gửi kết quả real-time qua WebSocket                    │
└──────────────────────┬────────────────────────────────────┘
                       │
        ┌──────────────┴──────────────┐
        │                             │
┌───────▼────────┐          ┌─────────▼─────────┐
│  Matching      │          │   Database        │
│  Engine        │          │   Service         │
│  (Core)        │          │                   │
│                │          │  - Orders         │
│  - Matching    │          │  - Trades         │
│  - Risk        │          │  - Balances       │
│  - Balance     │          │  - Positions      │
└───────┬────────┘          └───────────────────┘
        │
        │ Result Consumer (Callback)
        │
┌───────▼──────────────────────────────────────┐
│         Result Handler Service               │
│  - Lưu orders/trades vào DB                 │
│  - Gửi thông báo qua WebSocket              │
│  - Cập nhật cache (Redis)                   │
└──────────────────────────────────────────────┘
```

## Luồng xử lý

### 1. Nhận lệnh từ Client

```rust
// API Gateway / WebSocket Server nhận lệnh
let order_command = OrderCommand {
    command: OrderCommandType::PlaceOrder,
    uid: user_id,
    order_id: generate_order_id(),
    symbol: symbol_id,
    price: price,
    size: size,
    action: OrderAction::Bid,
    order_type: OrderType::Gtc,
    reserve_price: price,
    timestamp: current_timestamp(),
    ..Default::default()
};

// Gửi vào matching engine
let result = exchange.submit_command(order_command);
```

### 2. Xử lý kết quả (Result Consumer)

```rust
use matching_core::core::exchange::{ExchangeCore, ResultConsumer};
use std::sync::Arc;

// Tạo result consumer để xử lý kết quả
let result_consumer: ResultConsumer = Arc::new(|cmd: &OrderCommand| {
    // 1. Lưu vào Database
    save_to_database(cmd);
    
    // 2. Gửi qua WebSocket
    send_websocket_notification(cmd);
    
    // 3. Cập nhật cache (nếu cần)
    update_cache(cmd);
});

// Đăng ký consumer
exchange.set_result_consumer(result_consumer);
```

### 3. Lưu vào Database

```rust
async fn save_to_database(cmd: &OrderCommand) {
    let db = get_database_connection().await;
    
    match cmd.command {
        OrderCommandType::PlaceOrder => {
            // Lưu lệnh
            db.save_order(OrderRecord {
                order_id: cmd.order_id,
                uid: cmd.uid,
                symbol: cmd.symbol,
                price: cmd.price,
                size: cmd.size,
                action: cmd.action,
                order_type: cmd.order_type,
                status: match cmd.result_code {
                    CommandResultCode::Success => "Filled",
                    CommandResultCode::ValidForMatchingEngine => "Pending",
                    _ => "Rejected",
                },
                created_at: cmd.timestamp,
            }).await;
            
            // Lưu các giao dịch (nếu có)
            for event in &cmd.matcher_events {
                if event.event_type == MatcherEventType::Trade {
                    db.save_trade(TradeRecord {
                        trade_id: generate_trade_id(),
                        order_id: cmd.order_id,
                        matched_order_id: event.matched_order_id,
                        symbol: cmd.symbol,
                        price: event.price,
                        size: event.size,
                        taker_uid: cmd.uid,
                        maker_uid: event.matched_order_uid,
                        timestamp: event.timestamp,
                    }).await;
                }
            }
        }
        
        OrderCommandType::CancelOrder => {
            // Cập nhật trạng thái lệnh
            db.update_order_status(cmd.order_id, "Cancelled").await;
        }
        
        OrderCommandType::BalanceAdjustment => {
            // Lưu lịch sử nạp/rút tiền
            db.save_balance_transaction(BalanceTransaction {
                uid: cmd.uid,
                currency: cmd.symbol, // currency ID
                amount: cmd.price,     // amount
                transaction_id: cmd.order_id,
                timestamp: cmd.timestamp,
            }).await;
        }
        
        _ => {}
    }
}
```

### 4. Gửi qua WebSocket

```rust
use tokio::sync::broadcast;

struct WebSocketService {
    sender: broadcast::Sender<WebSocketMessage>,
}

async fn send_websocket_notification(cmd: &OrderCommand) {
    let ws_service = get_websocket_service();
    
    match cmd.command {
        OrderCommandType::PlaceOrder => {
            // Gửi thông báo lệnh mới
            ws_service.broadcast(WebSocketMessage {
                channel: format!("user:{}:orders", cmd.uid),
                event: "order_placed",
                data: json!({
                    "order_id": cmd.order_id,
                    "symbol": cmd.symbol,
                    "price": cmd.price,
                    "size": cmd.size,
                    "action": cmd.action,
                    "status": cmd.result_code,
                }),
            }).await;
            
            // Gửi thông báo giao dịch (nếu có)
            for event in &cmd.matcher_events {
                if event.event_type == MatcherEventType::Trade {
                    ws_service.broadcast(WebSocketMessage {
                        channel: format!("market:{}:trades", cmd.symbol),
                        event: "trade",
                        data: json!({
                            "trade_id": generate_trade_id(),
                            "price": event.price,
                            "size": event.size,
                            "timestamp": event.timestamp,
                        }),
                    }).await;
                    
                    // Gửi cho cả taker và maker
                    ws_service.broadcast(WebSocketMessage {
                        channel: format!("user:{}:trades", cmd.uid),
                        event: "trade_executed",
                        data: json!({
                            "order_id": cmd.order_id,
                            "price": event.price,
                            "size": event.size,
                            "side": "taker",
                        }),
                    }).await;
                    
                    ws_service.broadcast(WebSocketMessage {
                        channel: format!("user:{}:trades", event.matched_order_uid),
                        event: "trade_executed",
                        data: json!({
                            "order_id": event.matched_order_id,
                            "price": event.price,
                            "size": event.size,
                            "side": "maker",
                        }),
                    }).await;
                    
                    // Gửi thông báo settlement cho Taker
                    let taker_settlement = calculate_settlement(cmd, event, "taker");
                    ws_service.broadcast(WebSocketMessage {
                        channel: format!("user:{}:settlement", cmd.uid),
                        event: "settlement",
                        data: json!({
                            "trade_id": generate_trade_id(),
                            "order_id": cmd.order_id,
                            "symbol": cmd.symbol,
                            "role": "taker",
                            "quote_amount": taker_settlement.quote_amount,
                            "base_amount": taker_settlement.base_amount,
                            "fee": taker_settlement.fee,
                            "timestamp": cmd.timestamp,
                        }),
                    }).await;
                    
                    // Gửi thông báo settlement cho Maker
                    let maker_settlement = calculate_settlement(cmd, event, "maker");
                    ws_service.broadcast(WebSocketMessage {
                        channel: format!("user:{}:settlement", event.matched_order_uid),
                        event: "settlement",
                        data: json!({
                            "trade_id": generate_trade_id(),
                            "order_id": event.matched_order_id,
                            "symbol": cmd.symbol,
                            "role": "maker",
                            "quote_amount": maker_settlement.quote_amount,
                            "base_amount": maker_settlement.base_amount,
                            "fee": maker_settlement.fee,
                            "timestamp": cmd.timestamp,
                        }),
                    }).await;
                }
            }
        }
        
        OrderCommandType::CancelOrder => {
            ws_service.broadcast(WebSocketMessage {
                channel: format!("user:{}:orders", cmd.uid),
                event: "order_cancelled",
                data: json!({
                    "order_id": cmd.order_id,
                    "symbol": cmd.symbol,
                }),
            }).await;
        }
        
        _ => {}
    }
}
```

## Settlement (Thanh toán)

Matching engine **tự động xử lý settlement** trong Risk Engine R2 (post-process). Các service khác chỉ cần **lưu lại thông tin settlement** vào database.

### Cách Settlement hoạt động

#### 1. Settlement cho Taker (người đặt lệnh mới)

**Lệnh mua (Bid):**
```rust
// Sau khi khớp lệnh mua:
// - Hoàn lại tiền thừa: (reserve_price - match_price) * size * quote_scale_k
// - Nhận base currency: size * base_scale_k
// - Trừ taker_fee: size * taker_fee

let price_diff = event.bidder_hold_price - event.price;
let refund = event.size * price_diff * spec.quote_scale_k;
taker.balance[quote_currency] += refund;  // Hoàn lại tiền thừa
taker.balance[base_currency] += event.size * spec.base_scale_k;  // Nhận base currency
// Taker fee đã được trừ trong pre-process
```

**Lệnh bán (Ask):**
```rust
// Sau khi khớp lệnh bán:
// - Nhận quote currency: match_price * size * quote_scale_k
// - Trừ taker_fee: size * taker_fee

let amount = event.size * event.price * spec.quote_scale_k - event.size * spec.taker_fee;
taker.balance[quote_currency] += amount;
```

#### 2. Settlement cho Maker (người có lệnh treo)

**Maker mua (khi Taker bán):**
```rust
// Maker mua từ Taker bán:
// - Hoàn lại tiền thừa: (reserve_price - match_price) * size * quote_scale_k
// - Nhận base currency: size * base_scale_k

let price_diff = event.bidder_hold_price - event.price;
let refund = event.size * price_diff * spec.quote_scale_k;
maker.balance[quote_currency] += refund;
maker.balance[base_currency] += event.size * spec.base_scale_k;
```

**Maker bán (khi Taker mua):**
```rust
// Maker bán cho Taker mua:
// - Nhận quote currency: match_price * size * quote_scale_k
// - Trừ maker_fee: size * maker_fee

let amount = event.size * event.price * spec.quote_scale_k - event.size * spec.maker_fee;
maker.balance[quote_currency] += amount;
```

### Lưu Settlement vào Database

```rust
async fn save_settlement_to_database(cmd: &OrderCommand) {
    let db = get_database_connection().await;
    
    for event in &cmd.matcher_events {
        if event.event_type == MatcherEventType::Trade {
            let spec = get_symbol_spec(cmd.symbol);
            
            // Tính toán settlement cho Taker
            let taker_settlement = match cmd.action {
                OrderAction::Bid => {
                    // Lệnh mua
                    let price_diff = event.bidder_hold_price - event.price;
                    let refund = event.size * price_diff * spec.quote_scale_k;
                    let base_received = event.size * spec.base_scale_k;
                    let taker_fee = event.size * spec.taker_fee;
                    
                    SettlementRecord {
                        trade_id: generate_trade_id(),
                        uid: cmd.uid,
                        role: "taker",
                        quote_amount: refund,  // Hoàn lại
                        base_amount: base_received,  // Nhận
                        fee: taker_fee,
                        fee_currency: spec.quote_currency,
                    }
                }
                OrderAction::Ask => {
                    // Lệnh bán
                    let quote_received = event.size * event.price * spec.quote_scale_k;
                    let taker_fee = event.size * spec.taker_fee;
                    
                    SettlementRecord {
                        trade_id: generate_trade_id(),
                        uid: cmd.uid,
                        role: "taker",
                        quote_amount: quote_received - taker_fee,  // Nhận sau khi trừ fee
                        base_amount: -event.size * spec.base_scale_k,  // Trả base
                        fee: taker_fee,
                        fee_currency: spec.quote_currency,
                    }
                }
            };
            
            // Tính toán settlement cho Maker
            let maker_settlement = if cmd.action == OrderAction::Ask {
                // Taker bán => Maker mua
                let price_diff = event.bidder_hold_price - event.price;
                let refund = event.size * price_diff * spec.quote_scale_k;
                let base_received = event.size * spec.base_scale_k;
                
                SettlementRecord {
                    trade_id: generate_trade_id(),
                    uid: event.matched_order_uid,
                    role: "maker",
                    quote_amount: refund,  // Hoàn lại
                    base_amount: base_received,  // Nhận
                    fee: 0,  // Maker không trả fee trong ví dụ này
                    fee_currency: spec.quote_currency,
                }
            } else {
                // Taker mua => Maker bán
                let quote_received = event.size * event.price * spec.quote_scale_k;
                let maker_fee = event.size * spec.maker_fee;
                
                SettlementRecord {
                    trade_id: generate_trade_id(),
                    uid: event.matched_order_uid,
                    role: "maker",
                    quote_amount: quote_received - maker_fee,  // Nhận sau khi trừ fee
                    base_amount: -event.size * spec.base_scale_k,  // Trả base
                    fee: maker_fee,
                    fee_currency: spec.quote_currency,
                }
            };
            
            // Lưu settlement records
            db.save_settlement(taker_settlement).await;
            db.save_settlement(maker_settlement).await;
        }
    }
}
```

## Statistics (Thống kê)

Matching engine cung cấp **market data** qua OrderBook API. Các service khác cần **tính toán và lưu trữ statistics** từ dữ liệu giao dịch.

### 1. Market Data từ Matching Engine

```rust
use matching_core::api::market_data::L2MarketData;
use matching_core::core::orderbook::OrderBook;

// Lấy L2 market data từ orderbook
fn get_market_data(exchange: &ExchangeCore, symbol: SymbolId, depth: usize) -> L2MarketData {
    // Lấy orderbook cho symbol
    let orderbook = exchange.get_orderbook(symbol);
    
    // Lấy L2 data
    orderbook.get_l2_data(depth)
}

// Lấy thống kê orderbook
fn get_orderbook_stats(orderbook: &dyn OrderBook) -> OrderBookStats {
    OrderBookStats {
        total_ask_volume: orderbook.get_total_ask_volume(),
        total_bid_volume: orderbook.get_total_bid_volume(),
        ask_buckets_count: orderbook.get_ask_buckets_count(),
        bid_buckets_count: orderbook.get_bid_buckets_count(),
        best_ask: orderbook.get_l2_data(1).ask_prices.first().copied(),
        best_bid: orderbook.get_l2_data(1).bid_prices.first().copied(),
    }
}
```

### 2. Tính toán Statistics từ Trades

```rust
struct StatisticsService {
    db: Database,
    cache: RedisCache,
}

impl StatisticsService {
    /// Cập nhật statistics khi có trade mới
    async fn update_statistics(&self, trade: &TradeRecord) {
        // 1. Ticker Statistics (24h)
        let ticker = self.calculate_ticker(trade).await;
        self.cache.set_ticker(trade.symbol_id, &ticker).await;
        
        // 2. Volume Statistics
        self.update_volume_stats(trade).await;
        
        // 3. Price Statistics
        self.update_price_stats(trade).await;
        
        // 4. Trade Count
        self.increment_trade_count(trade.symbol_id).await;
    }
    
    /// Tính toán Ticker (24h statistics)
    async fn calculate_ticker(&self, trade: &TradeRecord) -> TickerStats {
        let now = current_timestamp();
        let day_start = now - 86400;  // 24 giờ trước
        
        // Lấy tất cả trades trong 24h
        let trades = self.db.get_trades(
            trade.symbol_id,
            day_start,
            now
        ).await;
        
        if trades.is_empty() {
            return TickerStats::default();
        }
        
        let prices: Vec<i64> = trades.iter().map(|t| t.price).collect();
        let volumes: Vec<i64> = trades.iter().map(|t| t.size).collect();
        
        TickerStats {
            symbol_id: trade.symbol_id,
            open: trades.first().unwrap().price,  // Giá đầu ngày
            high: *prices.iter().max().unwrap(),   // Giá cao nhất
            low: *prices.iter().min().unwrap(),    // Giá thấp nhất
            close: trade.price,                    // Giá hiện tại
            volume: volumes.iter().sum(),           // Tổng volume
            quote_volume: trades.iter()
                .map(|t| t.price * t.size)
                .sum(),                            // Tổng quote volume
            trade_count: trades.len() as u64,      // Số lượng giao dịch
            timestamp: now,
        }
    }
    
    /// Cập nhật Volume Statistics
    async fn update_volume_stats(&self, trade: &TradeRecord) {
        // Volume theo khung thời gian
        let intervals = vec![
            ("1m", 60),
            ("5m", 300),
            ("15m", 900),
            ("1h", 3600),
            ("24h", 86400),
        ];
        
        for (interval, seconds) in intervals {
            let start_time = current_timestamp() - seconds;
            let volume = self.db.get_volume(
                trade.symbol_id,
                start_time,
                current_timestamp()
            ).await;
            
            self.cache.set_volume(
                trade.symbol_id,
                interval,
                volume
            ).await;
        }
    }
    
    /// Cập nhật Price Statistics
    async fn update_price_stats(&self, trade: &TradeRecord) {
        // VWAP (Volume Weighted Average Price)
        let vwap = self.calculate_vwap(trade.symbol_id, 24 * 3600).await;
        self.cache.set_vwap(trade.symbol_id, vwap).await;
        
        // Last Price
        self.cache.set_last_price(trade.symbol_id, trade.price).await;
        
        // Price Change (24h)
        let price_change = self.calculate_price_change(trade.symbol_id).await;
        self.cache.set_price_change(trade.symbol_id, price_change).await;
    }
    
    /// Tính VWAP
    async fn calculate_vwap(&self, symbol_id: SymbolId, period: i64) -> i64 {
        let start_time = current_timestamp() - period;
        let trades = self.db.get_trades(symbol_id, start_time, current_timestamp()).await;
        
        if trades.is_empty() {
            return 0;
        }
        
        let total_value: i64 = trades.iter()
            .map(|t| t.price * t.size)
            .sum();
        let total_volume: i64 = trades.iter()
            .map(|t| t.size)
            .sum();
        
        if total_volume > 0 {
            total_value / total_volume
        } else {
            0
        }
    }
}
```

### 3. Lưu Statistics vào Database

```rust
async fn save_statistics_to_database(ticker: &TickerStats) {
    let db = get_database_connection().await;
    
    // Lưu ticker statistics
    db.upsert_ticker(TickerRecord {
        symbol_id: ticker.symbol_id,
        open: ticker.open,
        high: ticker.high,
        low: ticker.low,
        close: ticker.close,
        volume: ticker.volume,
        quote_volume: ticker.quote_volume,
        trade_count: ticker.trade_count,
        timestamp: ticker.timestamp,
    }).await;
    
    // Lưu lịch sử giá (cho biểu đồ)
    db.insert_price_history(PriceHistoryRecord {
        symbol_id: ticker.symbol_id,
        price: ticker.close,
        volume: ticker.volume,
        timestamp: ticker.timestamp,
    }).await;
}
```

### 4. Gửi Statistics qua WebSocket

```rust
async fn broadcast_statistics(ws_service: &WebSocketService, ticker: &TickerStats) {
    // Gửi ticker update
    ws_service.broadcast(WebSocketMessage {
        channel: format!("market:{}:ticker", ticker.symbol_id),
        event: "ticker",
        data: json!({
            "symbol_id": ticker.symbol_id,
            "open": ticker.open,
            "high": ticker.high,
            "low": ticker.low,
            "close": ticker.close,
            "volume": ticker.volume,
            "quote_volume": ticker.quote_volume,
            "trade_count": ticker.trade_count,
            "price_change": ticker.close - ticker.open,
            "price_change_percent": ((ticker.close - ticker.open) as f64 / ticker.open as f64) * 100.0,
        }),
    }).await;
}
```

## Schema Database

### Bảng Orders
```sql
CREATE TABLE orders (
    order_id BIGINT PRIMARY KEY,
    uid BIGINT NOT NULL,
    symbol_id INT NOT NULL,
    price BIGINT NOT NULL,
    size BIGINT NOT NULL,
    filled_size BIGINT DEFAULT 0,
    action VARCHAR(10) NOT NULL, -- 'Bid' or 'Ask'
    order_type VARCHAR(20) NOT NULL,
    status VARCHAR(20) NOT NULL, -- 'Pending', 'Filled', 'Cancelled', 'Rejected'
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    INDEX idx_uid (uid),
    INDEX idx_symbol (symbol_id),
    INDEX idx_status (status)
);
```

### Bảng Trades
```sql
CREATE TABLE trades (
    trade_id BIGINT PRIMARY KEY,
    order_id BIGINT NOT NULL,
    matched_order_id BIGINT NOT NULL,
    symbol_id INT NOT NULL,
    price BIGINT NOT NULL,
    size BIGINT NOT NULL,
    taker_uid BIGINT NOT NULL,
    maker_uid BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    INDEX idx_order (order_id),
    INDEX idx_symbol_time (symbol_id, timestamp),
    INDEX idx_taker (taker_uid),
    INDEX idx_maker (maker_uid)
);
```

### Bảng Balances
```sql
CREATE TABLE balances (
    uid BIGINT NOT NULL,
    currency INT NOT NULL,
    balance BIGINT NOT NULL,
    frozen BIGINT DEFAULT 0,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (uid, currency),
    INDEX idx_uid (uid)
);
```

### Bảng Balance Transactions
```sql
CREATE TABLE balance_transactions (
    transaction_id BIGINT PRIMARY KEY,
    uid BIGINT NOT NULL,
    currency INT NOT NULL,
    amount BIGINT NOT NULL,
    transaction_type VARCHAR(20) NOT NULL, -- 'Deposit', 'Withdraw', 'Trade'
    created_at BIGINT NOT NULL,
    INDEX idx_uid (uid),
    INDEX idx_time (created_at)
);
```

### Bảng Settlements
```sql
CREATE TABLE settlements (
    settlement_id BIGINT PRIMARY KEY AUTO_INCREMENT,
    trade_id BIGINT NOT NULL,
    uid BIGINT NOT NULL,
    role VARCHAR(10) NOT NULL, -- 'taker' or 'maker'
    symbol_id INT NOT NULL,
    quote_amount BIGINT NOT NULL, -- Số tiền quote currency (có thể âm nếu trả)
    base_amount BIGINT NOT NULL,  -- Số tiền base currency (có thể âm nếu trả)
    fee BIGINT NOT NULL,
    fee_currency INT NOT NULL,
    timestamp BIGINT NOT NULL,
    INDEX idx_trade (trade_id),
    INDEX idx_uid (uid),
    INDEX idx_symbol_time (symbol_id, timestamp)
);
```

### Bảng Tickers (24h Statistics)
```sql
CREATE TABLE tickers (
    symbol_id INT PRIMARY KEY,
    open BIGINT NOT NULL,           -- Giá mở cửa (24h trước)
    high BIGINT NOT NULL,           -- Giá cao nhất (24h)
    low BIGINT NOT NULL,            -- Giá thấp nhất (24h)
    close BIGINT NOT NULL,          -- Giá đóng cửa (hiện tại)
    volume BIGINT NOT NULL,         -- Tổng volume base currency
    quote_volume BIGINT NOT NULL,   -- Tổng volume quote currency
    trade_count BIGINT NOT NULL,   -- Số lượng giao dịch
    vwap BIGINT,                    -- Volume Weighted Average Price
    price_change BIGINT,            -- Thay đổi giá (close - open)
    price_change_percent DECIMAL(10,4), -- % thay đổi giá
    updated_at BIGINT NOT NULL,
    INDEX idx_updated (updated_at)
);
```

### Bảng Price History (cho biểu đồ)
```sql
CREATE TABLE price_history (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    symbol_id INT NOT NULL,
    price BIGINT NOT NULL,
    volume BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    INDEX idx_symbol_time (symbol_id, timestamp),
    INDEX idx_time (timestamp)
);
```

### Bảng Volume Statistics
```sql
CREATE TABLE volume_stats (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    symbol_id INT NOT NULL,
    interval VARCHAR(10) NOT NULL, -- '1m', '5m', '15m', '1h', '24h'
    volume BIGINT NOT NULL,
    quote_volume BIGINT NOT NULL,
    trade_count BIGINT NOT NULL,
    start_time BIGINT NOT NULL,
    end_time BIGINT NOT NULL,
    INDEX idx_symbol_interval (symbol_id, interval, end_time)
);
```

## WebSocket Channels

### User Channels
- `user:{uid}:orders` - Thông báo lệnh của user
- `user:{uid}:trades` - Thông báo giao dịch của user
- `user:{uid}:balance` - Thông báo số dư tổng thể (tất cả currencies) sau khi có thay đổi
- `user:{uid}:settlement` - Thông báo chi tiết settlement của từng trade cụ thể

### Market Channels
- `market:{symbol}:trades` - Giao dịch công khai của market
- `market:{symbol}:orderbook` - Cập nhật sổ lệnh
- `market:{symbol}:ticker` - Thông tin ticker (24h statistics)
- `market:{symbol}:stats` - Thống kê chi tiết (volume, VWAP, etc.)

### Settlement Channels
- `user:{uid}:settlement` - Thông báo settlement (thanh toán) cho user sau mỗi trade

**Ví dụ thông báo settlement:**
```rust
async fn send_settlement_notification(
    ws_service: &WebSocketService,
    settlement: &SettlementRecord,
    trade: &TradeRecord,
) {
    ws_service.broadcast(WebSocketMessage {
        channel: format!("user:{}:settlement", settlement.uid),
        event: "settlement",
        data: json!({
            "settlement_id": settlement.settlement_id,
            "trade_id": settlement.trade_id,
            "order_id": trade.order_id,
            "symbol_id": trade.symbol_id,
            "role": settlement.role,  // "taker" hoặc "maker"
            "quote_amount": settlement.quote_amount,  // Số tiền quote currency (có thể âm nếu trả)
            "base_amount": settlement.base_amount,    // Số tiền base currency (có thể âm nếu trả)
            "fee": settlement.fee,                    // Phí giao dịch
            "fee_currency": settlement.fee_currency,
            "timestamp": settlement.timestamp,
            // Thông tin chi tiết về trade
            "trade_price": trade.price,
            "trade_size": trade.size,
        }),
    }).await;
}
```

**Ví dụ thông báo thực tế:**

Khi user 1001 (Taker) mua BTC/USD với giá khớp $50,000 (giá đặt $51,000):
```json
{
  "channel": "user:1001:settlement",
  "event": "settlement",
  "data": {
    "settlement_id": 12345,
    "trade_id": 67890,
    "order_id": 11111,
    "symbol_id": 1,
    "role": "taker",
    "quote_amount": 1000000,      // Hoàn lại $1,000 (chênh lệch giá)
    "base_amount": 100000000,      // Nhận 1 BTC (100,000,000 satoshi)
    "fee": 50000,                  // Phí taker $50
    "fee_currency": 1,             // USD
    "timestamp": 1699123456,
    "trade_price": 50000,
    "trade_size": 100000000
  }
}
```

Khi user 1002 (Maker) bán BTC/USD với giá $50,000:
```json
{
  "channel": "user:1002:settlement",
  "event": "settlement",
  "data": {
    "settlement_id": 12346,
    "trade_id": 67890,
    "order_id": 22222,
    "symbol_id": 1,
    "role": "maker",
    "quote_amount": 499750000,     // Nhận $4,997.50 (sau khi trừ maker fee)
    "base_amount": -100000000,      // Trả 1 BTC (số âm = trả)
    "fee": 25000,                  // Phí maker $25
    "fee_currency": 1,             // USD
    "timestamp": 1699123456,
    "trade_price": 50000,
    "trade_size": 100000000
  }
}
```

**Mục đích của channel này:**
- Thông báo real-time cho user về việc thanh toán đã hoàn tất
- User có thể cập nhật UI hiển thị số dư mới
- Giúp user theo dõi lịch sử settlement chi tiết
- Hữu ích cho việc audit và reconciliation

## Sự khác biệt: `user:{uid}:settlement` vs `user:{uid}:balance`

### `user:{uid}:settlement` - Chi tiết từng giao dịch (Transaction-level)
- **Gửi khi:** Sau mỗi trade cụ thể
- **Nội dung:** Chi tiết settlement của trade đó
  - Quote amount (có thể âm/dương)
  - Base amount (có thể âm/dương)
  - Fee đã trừ
  - Role (taker/maker)
  - Trade ID, Order ID
- **Mục đích:** 
  - Audit trail chi tiết
  - Theo dõi từng giao dịch
  - Reconciliation
  - Hiển thị chi tiết trong lịch sử giao dịch

**Ví dụ:**
```json
{
  "channel": "user:1001:settlement",
  "event": "settlement",
  "data": {
    "trade_id": 67890,
    "quote_amount": 1000000,    // Hoàn lại $1,000
    "base_amount": 100000000,     // Nhận 1 BTC
    "fee": 50000,                 // Phí $50
    "role": "taker"
  }
}
```

### `user:{uid}:balance` - Tổng số dư (Account-level)
- **Gửi khi:** Số dư tổng thể thay đổi (có thể từ nhiều trades hoặc deposit/withdraw)
- **Nội dung:** Số dư hiện tại của tất cả currencies
  - Balance của từng currency
  - Frozen balance (nếu có)
  - Tổng giá trị (nếu tính)
- **Mục đích:**
  - Hiển thị số dư hiện tại cho user
  - Cập nhật UI wallet/balance
  - Kiểm tra số dư nhanh

**Ví dụ:**
```json
{
  "channel": "user:1001:balance",
  "event": "balance_updated",
  "data": {
    "balances": {
      "1": {  // USD
        "available": 999500000,   // $9,995.00
        "frozen": 0
      },
      "2": {  // BTC
        "available": 100000000,   // 1 BTC
        "frozen": 0
      }
    },
    "timestamp": 1699123456
  }
}
```

### So sánh trực tiếp:

| Tiêu chí | `user:{uid}:settlement` | `user:{uid}:balance` |
|----------|------------------------|----------------------|
| **Mức độ chi tiết** | Chi tiết từng trade | Tổng số dư |
| **Tần suất** | Mỗi trade | Khi số dư thay đổi (có thể tổng hợp) |
| **Nội dung** | Quote/base amount, fee, role, trade_id | Balance của tất cả currencies |
| **Use case** | Audit, lịch sử giao dịch | Hiển thị wallet, kiểm tra số dư |
| **Granularity** | Transaction-level | Account-level |

### Khi nào dùng channel nào?

**Dùng `settlement` khi:**
- Cần hiển thị chi tiết từng giao dịch
- Cần audit trail
- Cần hiển thị trong lịch sử giao dịch
- Cần biết role (taker/maker) và fee của từng trade

**Dùng `balance` khi:**
- Cần hiển thị số dư hiện tại
- Cần cập nhật UI wallet
- Cần kiểm tra số dư nhanh
- Không cần chi tiết từng giao dịch

### Implementation Example:

```rust
async fn send_balance_notification(
    ws_service: &WebSocketService,
    uid: UserId,
    balances: &HashMap<Currency, BalanceInfo>,
) {
    ws_service.broadcast(WebSocketMessage {
        channel: format!("user:{}:balance", uid),
        event: "balance_updated",
        data: json!({
            "balances": balances,
            "timestamp": current_timestamp(),
        }),
    }).await;
}

// Gửi cả 2 channels sau mỗi trade
async fn handle_trade_settlement(
    ws_service: &WebSocketService,
    settlement: &SettlementRecord,
    user_balances: &HashMap<Currency, BalanceInfo>,
) {
    // 1. Gửi settlement chi tiết
    send_settlement_notification(ws_service, settlement).await;
    
    // 2. Gửi balance tổng thể
    send_balance_notification(ws_service, settlement.uid, user_balances).await;
}
```

## Ví dụ Implementation đầy đủ

```rust
use matching_core::api::*;
use matching_core::core::exchange::{ExchangeCore, ExchangeConfig, ResultConsumer};
use std::sync::Arc;
use tokio::sync::broadcast;

struct TradingService {
    exchange: ExchangeCore,
    db: Database,
    ws_sender: broadcast::Sender<WebSocketMessage>,
}

impl TradingService {
    fn new() -> Self {
        let config = ExchangeConfig::default();
        let mut exchange = ExchangeCore::new(config);
        
        // Đăng ký result consumer
        let db = Database::new();
        let (ws_sender, _) = broadcast::channel(1000);
        
        let result_consumer: ResultConsumer = Arc::new({
            let db = db.clone();
            let ws_sender = ws_sender.clone();
            let stats_service = StatisticsService::new(db.clone());
            move |cmd: &OrderCommand| {
                let db = db.clone();
                let ws_sender = ws_sender.clone();
                let stats_service = stats_service.clone();
                let cmd = cmd.clone();
                
                tokio::spawn(async move {
                    // 1. Lưu orders/trades vào DB
                    save_to_database(&db, &cmd).await;
                    
                    // 2. Lưu settlement vào DB
                    save_settlement_to_database(&db, &cmd).await;
                    
                    // 3. Cập nhật và lưu statistics
                    for event in &cmd.matcher_events {
                        if event.event_type == MatcherEventType::Trade {
                            let trade = TradeRecord {
                                order_id: cmd.order_id,
                                matched_order_id: event.matched_order_id,
                                symbol: cmd.symbol,
                                price: event.price,
                                size: event.size,
                                taker_uid: cmd.uid,
                                maker_uid: event.matched_order_uid,
                                timestamp: cmd.timestamp,
                            };
                            
                            // Cập nhật statistics
                            stats_service.update_statistics(&trade).await;
                        }
                    }
                    
                    // 4. Gửi WebSocket notifications
                    send_websocket_notification(&ws_sender, &cmd).await;
                });
            }
        });
        
        exchange.set_result_consumer(result_consumer);
        
        Self {
            exchange,
            db,
            ws_sender,
        }
    }
    
    async fn handle_place_order(&mut self, order: PlaceOrderRequest) -> OrderResponse {
        let cmd = OrderCommand {
            command: OrderCommandType::PlaceOrder,
            uid: order.uid,
            order_id: generate_order_id(),
            symbol: order.symbol,
            price: order.price,
            size: order.size,
            action: order.action,
            order_type: order.order_type,
            reserve_price: order.price,
            timestamp: current_timestamp(),
            ..Default::default()
        };
        
        let result = self.exchange.submit_command(cmd);
        
        OrderResponse {
            order_id: result.order_id,
            status: result.result_code,
            trades: result.matcher_events,
        }
    }
}
```

## Tóm tắt

**Matching Engine đã xử lý:**
- ✅ Tất cả logic nghiệp vụ
- ✅ Khớp lệnh, risk, balance
- ✅ **Settlement** - Tự động thanh toán cho Taker và Maker
- ✅ **Market Data** - Cung cấp L2 orderbook data
- ✅ Độ tin cậy (journaling, snapshot)

**Các service khác chỉ cần:**
1. **Lưu vào DB** - Lưu trữ lệnh, giao dịch, số dư, settlement, statistics
2. **Tính toán Statistics** - Tổng hợp thống kê từ dữ liệu giao dịch (ticker, volume, VWAP, etc.)
3. **Gửi WebSocket** - Thông báo real-time cho clients
4. **API Gateway** - Nhận lệnh từ clients và gửi vào matching engine

**Kiến trúc đơn giản và rõ ràng!** 🚀

## Tóm tắt Settlement và Statistics

### Settlement
- ✅ **Tự động xử lý** trong Risk Engine R2
- ✅ Thanh toán cho cả Taker và Maker
- ✅ Tính phí giao dịch (taker_fee, maker_fee)
- ✅ Hoàn lại tiền thừa khi giá khớp thấp hơn giá đặt
- 📝 **Service chỉ cần lưu** thông tin settlement vào database

### Statistics
- ✅ **Market Data** - Lấy từ OrderBook API (L2 data, orderbook stats)
- 📊 **Tính toán từ Trades** - Ticker (24h), Volume, VWAP, Price History
- 📝 **Lưu vào DB** - Tickers, Price History, Volume Stats
- 📡 **Gửi qua WebSocket** - Real-time updates cho clients

