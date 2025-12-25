# Matching Core

Thư viện core engine khớp lệnh hiệu năng cao, được xây dựng bằng Rust, hỗ trợ nhiều loại lệnh và sản phẩm giao dịch.

## Tính năng

### Chức năng cốt lõi
- **Engine khớp lệnh hiệu năng cao**: Hỗ trợ khớp lệnh với độ trễ cấp mili giây
- **Nhiều loại lệnh**: GTC、IOC、FOK、Post-Only、Stop Order、Iceberg、GTD、Day
- **Nhiều sản phẩm giao dịch**: Giao dịch giao ngay, hợp đồng tương lai, hợp đồng vĩnh viễn, quyền chọn mua, quyền chọn bán
- **Tối ưu bộ nhớ**: Bố cục bộ nhớ SOA, cấp phát trước hồ chứa lệnh, SmallVec giảm cấp phát heap
- **Serialize không sao chép**: Sử dụng rkyv để triển khai WAL hiệu năng cao

### Điểm nổi bật kỹ thuật
- **Mô hình LMAX Disruptor**: Bộ đệm vòng không khóa, đạt được thông lượng cao
- **Kiến trúc phân đoạn**: Hỗ trợ phân đoạn nhiều engine rủi ro và engine khớp lệnh
- **Bền vững**: Cơ chế nhật ký WAL và snapshot
- **Tối ưu SIMD**: Tối ưu khớp lệnh hàng loạt
- **Chỉ mục ART**: Cây cơ số thích ứng dùng cho chỉ mục giá

## Bắt đầu nhanh

### Cài đặt

```bash
git clone <repository-url>
cd matching-core
cargo build --release
```

### Sử dụng cơ bản

```rust
use matching_core::api::*;
use matching_core::core::orderbook::{OrderBook, AdvancedOrderBook};

// Tạo cấu hình cặp giao dịch
let spec = CoreSymbolSpecification {
    symbol_id: 1,
    symbol_type: SymbolType::CurrencyExchangePair,
    base_currency: 0,
    quote_currency: 1,
    base_scale_k: 1,
    quote_scale_k: 1,
    taker_fee: 0,
    maker_fee: 0,
    margin_buy: 0,
    margin_sell: 0,
};

// Tạo sổ lệnh
let mut book = AdvancedOrderBook::new(spec);

// Treo lệnh bán
let mut ask = OrderCommand {
    uid: 1,
    order_id: 1,
    symbol: 1,
    price: 10000,
    size: 100,
    action: OrderAction::Ask,
    order_type: OrderType::Gtc,
    reserve_price: 10000,
    timestamp: 1000,
    ..Default::default()
};
book.new_order(&mut ask);

// Lệnh mua khớp
let mut bid = OrderCommand {
    uid: 2,
    order_id: 2,
    symbol: 1,
    price: 10000,
    size: 50,
    action: OrderAction::Bid,
    order_type: OrderType::Ioc,
    reserve_price: 10000,
    timestamp: 1001,
    ..Default::default()
};
book.new_order(&mut bid);

// Xem sự kiện khớp lệnh
for event in bid.matcher_events {
    println!("Khớp lệnh: {} @ {}", event.size, event.price);
}
```

### Ví dụ loại lệnh nâng cao

#### Lệnh Post-Only (chỉ làm Maker)

```rust
let mut post_only = OrderCommand {
    uid: 1,
    order_id: 1,
    symbol: 1,
    price: 9999,
    size: 10,
    action: OrderAction::Bid,
    order_type: OrderType::PostOnly,
    reserve_price: 9999,
    timestamp: 1000,
    ..Default::default()
};
book.new_order(&mut post_only);
```

#### Lệnh Iceberg (Iceberg Order)

```rust
let mut iceberg = OrderCommand {
    uid: 1,
    order_id: 1,
    symbol: 1,
    price: 10000,
    size: 1000,        // Tổng số lượng
    action: OrderAction::Ask,
    order_type: OrderType::Iceberg,
    reserve_price: 10000,
    timestamp: 1000,
    visible_size: Some(100),  // Số lượng hiển thị
    ..Default::default()
};
book.new_order(&mut iceberg);
```

#### Lệnh cắt lỗ (Stop Order)

```rust
let mut stop = OrderCommand {
    uid: 1,
    order_id: 1,
    symbol: 1,
    price: 11000,      // Giá giới hạn
    size: 10,
    action: OrderAction::Bid,
    order_type: OrderType::StopLimit,
    reserve_price: 11000,
    timestamp: 1000,
    stop_price: Some(10500),  // Giá kích hoạt
    ..Default::default()
};
book.new_order(&mut stop);
```

#### Lệnh GTD (Good-Till-Date)

```rust
let mut gtd = OrderCommand {
    uid: 1,
    order_id: 1,
    symbol: 1,
    price: 10000,
    size: 100,
    action: OrderAction::Ask,
    order_type: OrderType::Gtd(2000),
    reserve_price: 10000,
    timestamp: 1000,
    expire_time: Some(2000),  // Timestamp hết hạn
    ..Default::default()
};
book.new_order(&mut gtd);
```

## Chỉ số hiệu năng

### Thông lượng
- **TPS (Transactions Per Second)**: Hỗ trợ xử lý hàng triệu lệnh
  - 10,000 lệnh: **7,247,910 TPS**
  - 100,000 lệnh: **4,968,213 TPS**
- **QPS (Queries Per Second)**: Truy vấn khớp lệnh đồng thời cao
  - 10,000 lệnh: **3,623,955 QPS**
  - 100,000 lệnh: **2,484,106 QPS**

### Độ trễ
- **Độ trễ trung bình**: < 1 micro giây (xử lý đơn lệnh)
- **Xử lý hàng loạt**: 10,000 lệnh khoảng 1.38 mili giây
- **Độ trễ P99**: < 10 micro giây

### Bộ nhớ
- **Sử dụng bộ nhớ**: Bố cục SOA được tối ưu, giảm phân mảnh bộ nhớ
  - 10,000 lệnh: **1.91 MB**
  - 100,000 lệnh: **19.07 MB**
- **Hồ chứa lệnh**: Cơ chế cấp phát trước, giảm cấp phát động

### Bảng dữ liệu hiệu năng

| Số lượng lệnh | TPS | QPS | Bộ nhớ (MB) | Độ trễ (ms) |
|---------|-----|-----|-----------|----------|
| 1,000 | 6,559,183 | 3,279,591 | 0.19 | 0.15 |
| 5,000 | 7,242,000 | 3,621,000 | 0.95 | 0.69 |
| 10,000 | 7,247,910 | 3,623,955 | 1.91 | 1.38 |
| 50,000 | 3,834,037 | 1,917,018 | 9.54 | 13.04 |
| 100,000 | 4,968,213 | 2,484,106 | 19.07 | 20.13 |

### Benchmark

Tạo dữ liệu hiệu năng:

```bash
cargo run --example generate_benchmark_data --release
```

Tạo biểu đồ hiệu năng (cần cài đặt matplotlib và pandas):

```bash
pip3 install matplotlib pandas
python3 scripts/plot_benchmark.py
```

Xem báo cáo test tổng hợp:

```bash
cargo run --example comprehensive_test --release
```

Chạy benchmark Criterion:

```bash
cargo bench --bench comprehensive_bench
```

## Cấu trúc dự án

```
matching-core/
├── src/
│   ├── api/              # Định nghĩa kiểu API
│   │   ├── types.rs      # Kiểu cơ bản
│   │   ├── commands.rs   # Lệnh đặt hàng
│   │   └── events.rs     # Sự kiện khớp lệnh
│   ├── core/             # Engine cốt lõi
│   │   ├── exchange.rs   # Core sàn giao dịch
│   │   ├── pipeline.rs   # Pipeline xử lý
│   │   ├── orderbook/    # Triển khai sổ lệnh
│   │   │   ├── naive.rs           # Triển khai cơ bản
│   │   │   ├── direct.rs          # Triển khai hiệu năng cao
│   │   │   ├── direct_optimized.rs # Tối ưu sâu
│   │   │   └── advanced.rs        # Loại lệnh nâng cao
│   │   ├── processors/   # Bộ xử lý
│   │   │   ├── risk_engine.rs     # Engine quản lý rủi ro
│   │   │   └── matching_engine.rs # Engine khớp lệnh
│   │   ├── journal.rs    # Nhật ký WAL
│   │   └── snapshot.rs   # Snapshot
│   └── lib.rs
├── examples/             # Mã ví dụ
│   ├── advanced_demo.rs      # Demo lệnh nâng cao
│   ├── comprehensive_test.rs # Test tổng hợp
│   └── load_test.rs          # Test tải
├── benches/              # Benchmark
│   ├── comprehensive_bench.rs # Benchmark tổng hợp
│   └── advanced_orderbook_bench.rs
└── tests/                # Unit test
    ├── advanced_orders_test.rs
    └── edge_cases_test.rs
```

## Hỗ trợ loại lệnh

| Loại lệnh | Mô tả | Trạng thái |
|---------|------|------|
| GTC | Good-Till-Cancel，cho đến khi hủy | ✅ |
| IOC | Immediate-or-Cancel，khớp ngay hoặc hủy | ✅ |
| FOK | Fill-or-Kill，khớp toàn bộ hoặc hủy toàn bộ | ✅ |
| Post-Only | Chỉ làm Maker，từ chối lệnh sẽ khớp ngay | ✅ |
| Stop Limit | Lệnh giới hạn cắt lỗ | ✅ |
| Stop Market | Lệnh thị trường cắt lỗ | ✅ |
| Iceberg | Lệnh iceberg，ẩn khối lượng lệnh treo thực tế | ✅ |
| Day | Có hiệu lực trong ngày | ✅ |
| GTD | Good-Till-Date，hết hạn vào ngày chỉ định | ✅ |

## Hỗ trợ sản phẩm giao dịch

| Loại sản phẩm | Mô tả | Trạng thái |
|---------|------|------|
| CurrencyExchangePair | Cặp giao dịch giao ngay | ✅ |
| FuturesContract | Hợp đồng tương lai | ✅ |
| PerpetualSwap | Hợp đồng vĩnh viễn | ✅ |
| CallOption | Quyền chọn mua | ✅ |
| PutOption | Quyền chọn bán | ✅ |

## Chạy ví dụ

### Demo khớp lệnh cơ bản

```bash
cargo run --example advanced_demo --release
```

### Bộ test tổng hợp

```bash
cargo run --example comprehensive_test --release
```

### Test tải

```bash
cargo run --example load_test --release
```

## Test

Chạy tất cả test:

```bash
cargo test --release
```

Chạy test cụ thể:

```bash
cargo test --test advanced_orders_test --release
cargo test --test edge_cases_test --release
```

## Phụ thuộc

Các phụ thuộc chính:
- `disruptor`: Triển khai mô hình LMAX Disruptor
- `rkyv`: Serialize không sao chép
- `ahash`: Thuật toán hash nhanh
- `slab`: Hồ chứa đối tượng
- `smallvec`: Tối ưu vector nhỏ
- `serde`: Framework serialize

## Giấy phép

Sử dụng tự do

## Liên hệ
Chỉ cần tạo issue, đây là code lấy từ môi trường production, logic tổng thể giống nhau, bộ code này chưa được áp dụng trong công ty, nên chỉ dùng để trao đổi học tập.
