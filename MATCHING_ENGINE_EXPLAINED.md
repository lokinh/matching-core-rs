# Giải thích Matching Engine

Tài liệu này giải thích chi tiết cách hoạt động của matching engine trong dự án `matching-core`.

## Tổng quan kiến trúc

Matching engine được xây dựng theo kiến trúc **LMAX Disruptor** với pipeline xử lý lệnh qua nhiều giai đoạn:

```
Lệnh vào → Risk Engine (R1) → Matching Engine → Risk Engine (R2) → Kết quả
```

## Cấu trúc thư mục

```
src/
├── api/              # Định nghĩa các kiểu dữ liệu và API
│   ├── types.rs      # Các kiểu cơ bản (OrderCommand, OrderType, etc.)
│   ├── commands.rs   # Lệnh đặt hàng
│   └── events.rs     # Sự kiện khớp lệnh
├── core/
│   ├── exchange.rs   # Core sàn giao dịch (ExchangeCore)
│   ├── pipeline.rs   # Pipeline xử lý lệnh
│   ├── processors/
│   │   ├── risk_engine.rs      # Engine quản lý rủi ro
│   │   └── matching_engine.rs # Engine khớp lệnh
│   └── orderbook/    # Các triển khai sổ lệnh
│       ├── naive.rs           # Triển khai cơ bản
│       ├── direct.rs          # Triển khai hiệu năng cao
│       ├── direct_optimized.rs # Tối ưu sâu
│       └── advanced.rs        # Hỗ trợ lệnh nâng cao
```

## Luồng xử lý lệnh

### 1. ExchangeCore (exchange.rs)

**Vai trò:** Điểm vào chính của hệ thống, quản lý Disruptor ring buffer và pipeline.

**Chức năng chính:**
- Nhận lệnh từ bên ngoài qua `submit_command()`
- Ghi nhật ký (journaling) nếu được bật
- Đưa lệnh vào Disruptor ring buffer
- Quản lý snapshot và recovery

**Ví dụ:**
```rust
let mut exchange = ExchangeCore::new(config);
exchange.startup();  // Khởi động Disruptor
exchange.add_symbol(symbol_spec);
exchange.submit_command(order_command);
```

### 2. Pipeline (pipeline.rs)

**Vai trò:** Điều phối luồng xử lý lệnh qua các engine.

**Luồng xử lý:**
1. **Risk Engine R1 (Pre-process):** Kiểm tra rủi ro trước khi khớp
   - Kiểm tra số dư tài khoản
   - Đóng băng số tiền cần thiết
   - Xác thực người dùng

2. **Matching Engine:** Khớp lệnh với sổ lệnh
   - Tìm lệnh đối lập để khớp
   - Tạo sự kiện khớp lệnh (trade events)
   - Cập nhật sổ lệnh

3. **Risk Engine R2 (Post-process):** Thanh toán sau khi khớp
   - Cập nhật số dư tài khoản
   - Hoàn lại tiền thừa
   - Tính phí giao dịch

4. **Result Consumer:** Gửi kết quả ra ngoài

**Code:**
```rust
pub fn handle_event(&mut self, cmd: &mut OrderCommand, ...) {
    // 1. Risk R1
    for engine in &mut self.risk_engines {
        engine.pre_process(cmd);
    }
    
    // 2. Matching Engine
    for engine in &mut self.matching_engines {
        engine.process_order(cmd);
    }
    
    // 3. Risk R2
    for engine in &mut self.risk_engines {
        engine.post_process(cmd);
    }
    
    // 4. Result Consumer
    if let Some(consumer) = &self.result_consumer {
        consumer(cmd);
    }
}
```

### 3. Risk Engine (risk_engine.rs)

**Vai trò:** Quản lý rủi ro và tài khoản người dùng.

#### Pre-process (R1) - Kiểm tra trước khi khớp

**Chức năng:**
- Kiểm tra người dùng có tồn tại không
- Kiểm tra số dư đủ để đặt lệnh không
- Đóng băng số tiền cần thiết:
  - **Lệnh mua (Bid):** Đóng băng `size * price * quote_scale_k + fee`
  - **Lệnh bán (Ask):** Đóng băng `size * base_scale_k`

**Code:**
```rust
fn place_order_risk_check(&mut self, cmd: &OrderCommand) -> CommandResultCode {
    // Kiểm tra người dùng
    let profile = self.user_service.get_user_mut(cmd.uid)?;
    
    // Tính số tiền cần đóng băng
    let hold_amount = match cmd.action {
        OrderAction::Bid => cmd.size * price * spec.quote_scale_k + fee,
        OrderAction::Ask => cmd.size * spec.base_scale_k,
    };
    
    // Kiểm tra và đóng băng
    if balance >= hold_amount {
        *balance -= hold_amount;
        CommandResultCode::ValidForMatchingEngine
    } else {
        CommandResultCode::RiskNsf  // Not Sufficient Funds
    }
}
```

#### Post-process (R2) - Thanh toán sau khi khớp

**Chức năng:**
- Cập nhật số dư cho Taker (người đặt lệnh mới)
- Cập nhật số dư cho Maker (người có lệnh treo)
- Hoàn lại tiền thừa nếu giá khớp thấp hơn giá đặt
- Tính phí giao dịch (taker fee, maker fee)

**Ví dụ lệnh mua khớp:**
- Taker mua: Hoàn lại `(reserve_price - match_price) * size` + nhận base currency
- Maker bán: Nhận `match_price * size - maker_fee`

### 4. Matching Engine (matching_engine.rs)

**Vai trò:** Điều phối khớp lệnh với các sổ lệnh (orderbook).

**Chức năng:**
- Phân đoạn (sharding) theo symbol_id
- Định tuyến lệnh đến đúng sổ lệnh
- Xử lý các loại lệnh:
  - `PlaceOrder`: Đặt lệnh mới
  - `CancelOrder`: Hủy lệnh
  - `MoveOrder`: Di chuyển lệnh (thay đổi giá)
  - `ReduceOrder`: Giảm số lượng lệnh

**Code:**
```rust
pub fn process_order(&mut self, cmd: &mut OrderCommand) {
    match cmd.command {
        OrderCommandType::PlaceOrder => {
            if cmd.result_code == CommandResultCode::ValidForMatchingEngine {
                book.new_order(cmd);  // Khớp lệnh
                cmd.result_code = CommandResultCode::Success;
            }
        }
        OrderCommandType::CancelOrder => {
            cmd.result_code = book.cancel_order(cmd);
        }
        // ...
    }
}
```

**Sharding:**
- Phân chia symbol_id vào các shard khác nhau
- Mỗi shard quản lý một tập hợp symbol
- Tăng khả năng xử lý song song

### 5. OrderBook (orderbook/)

**Vai trò:** Lưu trữ và khớp lệnh theo giá.

#### Các triển khai:

1. **NaiveOrderBook:** Triển khai cơ bản, dùng BTreeMap
2. **DirectOrderBook:** Tối ưu hơn, dùng cấu trúc dữ liệu tùy chỉnh
3. **DirectOrderBookOptimized:** Tối ưu sâu, giảm allocation
4. **AdvancedOrderBook:** Hỗ trợ tất cả loại lệnh nâng cao

#### Cấu trúc dữ liệu:

```
OrderBook
├── Ask side (lệnh bán) - Sắp xếp tăng dần theo giá
│   └── BTreeMap<Price, Bucket>
│       └── Bucket chứa các lệnh cùng giá
└── Bid side (lệnh mua) - Sắp xếp giảm dần theo giá
    └── BTreeMap<Price, Bucket>
        └── Bucket chứa các lệnh cùng giá
```

#### Quá trình khớp lệnh:

**Ví dụ: Lệnh mua mới (Bid) vào:**

1. Tìm lệnh bán (Ask) có giá thấp nhất
2. Nếu giá Ask ≤ giá Bid → Khớp được
3. Khớp theo thứ tự:
   - Ưu tiên giá tốt nhất (giá thấp nhất cho Ask)
   - Ưu tiên thời gian (FIFO) trong cùng giá
4. Tạo sự kiện khớp lệnh (MatcherTradeEvent)
5. Cập nhật sổ lệnh:
   - Giảm số lượng lệnh treo
   - Xóa lệnh nếu đã khớp hết
   - Thêm lệnh mới nếu chưa khớp hết

**Code (AdvancedOrderBook):**
```rust
pub fn new_order(&mut self, cmd: &mut OrderCommand) {
    match cmd.action {
        OrderAction::Bid => {
            // Tìm lệnh bán để khớp
            while let Some((&ask_price, bucket)) = self.asks.first_entry() {
                if ask_price > cmd.price {
                    break;  // Không khớp được
                }
                
                // Khớp với bucket này
                let (matched, events) = bucket.match_order(...);
                cmd.matcher_events.extend(events);
                
                // Xóa bucket nếu hết lệnh
                if bucket.is_empty() {
                    self.asks.remove(&ask_price);
                }
            }
            
            // Nếu còn số lượng, thêm vào sổ lệnh
            if cmd.size > 0 {
                self.bids.entry(cmd.price).or_insert(...).add(...);
            }
        }
        // Tương tự cho Ask
    }
}
```

## Các loại lệnh được hỗ trợ

### 1. GTC (Good-Till-Cancel)
- Lệnh treo cho đến khi hủy
- Khớp một phần hoặc toàn bộ

### 2. IOC (Immediate-or-Cancel)
- Khớp ngay hoặc hủy
- Không treo lệnh

### 3. FOK (Fill-or-Kill)
- Khớp toàn bộ hoặc hủy toàn bộ
- Không khớp một phần

### 4. Post-Only
- Chỉ làm Maker (không ăn lệnh)
- Nếu khớp ngay → Từ chối

### 5. Iceberg
- Ẩn khối lượng thực tế
- Chỉ hiển thị một phần (visible_size)
- Tự động bổ sung khi khớp hết phần hiển thị

### 6. Stop Order
- Kích hoạt khi giá đạt stop_price
- Sau đó trở thành lệnh thường

### 7. GTD (Good-Till-Date)
- Hết hạn tại thời điểm chỉ định
- Tự động hủy khi hết hạn

## Tối ưu hóa hiệu năng

### 1. LMAX Disruptor
- Ring buffer không khóa
- Xử lý hàng loạt (batching)
- Giảm contention

### 2. Sharding
- Phân chia symbol_id vào nhiều shard
- Xử lý song song
- Giảm lock contention

### 3. Memory Layout (SOA)
- Structure of Arrays thay vì Array of Structures
- Tăng cache locality
- Giảm memory fragmentation

### 4. SmallVec
- Dùng stack allocation cho vector nhỏ
- Giảm heap allocation

### 5. AHashMap
- Hash map nhanh hơn HashMap chuẩn
- Tối ưu cho key nhỏ

### 6. Serialization (rkyv)
- Zero-copy serialization
- Nhanh hơn serde/bincode
- Dùng cho WAL (Write-Ahead Log)

## Ví dụ luồng hoàn chỉnh

**Lệnh mua 100 BTC @ $50,000:**

1. **ExchangeCore.submit_command()**
   - Ghi journal (nếu bật)
   - Đưa vào Disruptor ring buffer

2. **Pipeline.handle_event()**
   - **Risk R1:** Kiểm tra số dư ≥ $5,000,000 + fee → Đóng băng
   - **Matching Engine:** 
     - Tìm lệnh bán ≤ $50,000
     - Khớp với lệnh bán $49,999 (50 BTC)
     - Khớp với lệnh bán $50,000 (50 BTC)
     - Tạo 2 trade events
   - **Risk R2:**
     - Taker: Hoàn lại $50,000 (chênh lệch giá) + Nhận 100 BTC
     - Maker 1: Nhận $49,999 * 50 - fee, Trả 50 BTC
     - Maker 2: Nhận $50,000 * 50 - fee, Trả 50 BTC
   - **Result Consumer:** Gửi kết quả ra ngoài

3. **Kết quả:**
   - `cmd.matcher_events` chứa 2 trade events
   - `cmd.result_code = Success`
   - Số dư tài khoản đã được cập nhật

## Hỗ trợ nhiều Pair và Multi-Currency

### Hỗ trợ nhiều Trading Pair

**Hệ thống hỗ trợ NHIỀU pair cùng lúc**, không chỉ 1 pair:

- Mỗi `MatchingEngineRouter` có `order_books: AHashMap<SymbolId, Box<dyn OrderBook>>`
- Mỗi `symbol_id` là một pair riêng biệt (ví dụ: BTC/USD = 100, ETH/USD = 200)
- Có thể thêm nhiều pair bằng `add_symbol()`:

```rust
// Thêm pair BTC/USD
exchange.add_symbol(CoreSymbolSpecification {
    symbol_id: 100,
    base_currency: 2,   // BTC
    quote_currency: 1,  // USD
    ...
});

// Thêm pair ETH/USD
exchange.add_symbol(CoreSymbolSpecification {
    symbol_id: 200,
    base_currency: 3,   // ETH
    quote_currency: 1,  // USD
    ...
});
```

**Sharding theo Symbol:**
- Mỗi matching engine shard quản lý một tập hợp symbol
- Symbol được phân bổ vào shard dựa trên `symbol_id & shard_mask`

### Xử lý Balance đầy đủ

**Hệ thống có quản lý balance hoàn chỉnh:**

#### 1. Cấu trúc Balance

Mỗi user có:
```rust
pub struct UserProfile {
    pub accounts: AHashMap<Currency, i64>,  // Số dư theo từng currency
    pub positions: AHashMap<SymbolId, SymbolPositionRecord>,  // Vị thế theo từng symbol
}
```

**Ví dụ:**
- User 1001 có: USD = 1,000,000, BTC = 10, ETH = 50
- User 1002 có: USD = 500,000, BTC = 5

#### 2. Quản lý Balance trong Risk Engine

**Pre-process (R1) - Đóng băng tiền:**
```rust
// Lệnh mua BTC/USD: Đóng băng USD
hold_amount = size * price * quote_scale_k + fee
balance[USD] -= hold_amount

// Lệnh bán BTC/USD: Đóng băng BTC
hold_amount = size * base_scale_k
balance[BTC] -= hold_amount
```

**Post-process (R2) - Thanh toán:**
```rust
// Sau khi khớp lệnh mua:
// - Hoàn lại tiền thừa: (reserve_price - match_price) * size
// - Nhận base currency: size * base_scale_k

// Sau khi khớp lệnh bán:
// - Nhận quote currency: match_price * size - fee
```

#### 3. Nạp/Rút tiền

Có thể nạp/rút tiền qua `BalanceAdjustment`:

```rust
// Nạp 100,000 USD cho user 1001
exchange.submit_command(OrderCommand {
    command: OrderCommandType::BalanceAdjustment,
    uid: 1001,
    symbol: 1,        // Currency ID (USD = 1)
    price: 100000,    // Số tiền nạp
    order_id: 1,      // Transaction ID
    ..Default::default()
});
```

#### 4. Multi-Currency Support

Hệ thống hỗ trợ nhiều currency:
- Mỗi symbol có `base_currency` và `quote_currency` riêng
- User có thể giữ nhiều currency trong cùng một tài khoản
- Balance được quản lý độc lập cho từng currency

**Ví dụ:**
```rust
// BTC/USD pair
base_currency: 2   // BTC
quote_currency: 1  // USD

// ETH/BTC pair  
base_currency: 3   // ETH
quote_currency: 2  // BTC

// User có thể giao dịch cả 2 pair với balance:
// USD: 1,000,000
// BTC: 10
// ETH: 50
```

### Ví dụ thực tế: Giao dịch nhiều Pair

```rust
let mut exchange = ExchangeCore::new(config);

// 1. Thêm nhiều pair
exchange.add_symbol(CoreSymbolSpecification {
    symbol_id: 100,  // BTC/USD
    base_currency: 2, quote_currency: 1,
    ...
});

exchange.add_symbol(CoreSymbolSpecification {
    symbol_id: 200,  // ETH/USD
    base_currency: 3, quote_currency: 1,
    ...
});

// 2. Thêm user và nạp tiền
exchange.submit_command(OrderCommand {
    command: OrderCommandType::AddUser,
    uid: 1001,
    ..Default::default()
});

// Nạp USD
exchange.submit_command(OrderCommand {
    command: OrderCommandType::BalanceAdjustment,
    uid: 1001, symbol: 1, price: 1000000,  // 1M USD
    ..Default::default()
});

// 3. Giao dịch trên nhiều pair
// Lệnh mua BTC/USD
exchange.submit_command(OrderCommand {
    symbol: 100,  // BTC/USD
    action: OrderAction::Bid,
    price: 50000, size: 1,
    ...
});

// Lệnh mua ETH/USD
exchange.submit_command(OrderCommand {
    symbol: 200,  // ETH/USD
    action: OrderAction::Bid,
    price: 3000, size: 10,
    ...
});
```

## Tóm tắt

Matching engine là một hệ thống xử lý lệnh giao dịch hiệu năng cao với:

- **Kiến trúc:** LMAX Disruptor + Pipeline
- **Xử lý:** Risk Engine → Matching Engine → Risk Engine
- **Lưu trữ:** OrderBook với nhiều triển khai tối ưu
- **Tính năng:** Hỗ trợ nhiều loại lệnh (GTC, IOC, FOK, Iceberg, Stop, GTD)
- **Multi-Pair:** Hỗ trợ nhiều trading pair cùng lúc (mỗi symbol_id là một pair)
- **Balance Management:** Quản lý balance đầy đủ cho nhiều currency, đóng băng/thanh toán tự động
- **Hiệu năng:** Sharding, SOA, SmallVec, AHashMap, rkyv
- **Độ tin cậy:** Journaling, Snapshot, Recovery

