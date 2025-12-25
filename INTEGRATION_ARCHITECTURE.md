# Kiến trúc Tích hợp Matching Engine

Tài liệu này mô tả cách các service khác tích hợp với matching engine.

## Tổng quan

Matching engine đã xử lý **TẤT CẢ** logic nghiệp vụ:
- ✅ Khớp lệnh (Matching)
- ✅ Quản lý rủi ro (Risk Management)
- ✅ Quản lý số dư (Balance Management)
- ✅ Xử lý nhiều loại lệnh (GTC, IOC, FOK, Iceberg, Stop, GTD)
- ✅ Hỗ trợ nhiều pair và multi-currency
- ✅ Journaling và Snapshot (độ tin cậy)

**Các service khác chỉ cần:**
1. **Lưu vào Database** - Lưu trữ lệnh, giao dịch, số dư
2. **Gửi qua WebSocket** - Thông báo real-time cho clients

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

## WebSocket Channels

### User Channels
- `user:{uid}:orders` - Thông báo lệnh của user
- `user:{uid}:trades` - Thông báo giao dịch của user
- `user:{uid}:balance` - Thông báo thay đổi số dư

### Market Channels
- `market:{symbol}:trades` - Giao dịch công khai của market
- `market:{symbol}:orderbook` - Cập nhật sổ lệnh
- `market:{symbol}:ticker` - Thông tin ticker

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
            move |cmd: &OrderCommand| {
                // Lưu vào DB (async, có thể spawn task)
                let db = db.clone();
                let cmd = cmd.clone();
                tokio::spawn(async move {
                    save_to_database(&db, &cmd).await;
                });
                
                // Gửi WebSocket
                let _ = ws_sender.send(create_websocket_message(&cmd));
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
- ✅ Độ tin cậy (journaling, snapshot)

**Các service khác chỉ cần:**
1. **Lưu vào DB** - Lưu trữ lệnh, giao dịch, số dư
2. **Gửi WebSocket** - Thông báo real-time cho clients
3. **API Gateway** - Nhận lệnh từ clients và gửi vào matching engine

**Kiến trúc đơn giản và rõ ràng!** 🚀

