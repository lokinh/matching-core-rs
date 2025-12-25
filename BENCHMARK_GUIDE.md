# Hướng dẫn chạy Benchmark

Tài liệu này hướng dẫn cách chạy các benchmark để đo hiệu năng của matching-core engine.

## Yêu cầu

- Rust đã được cài đặt (phiên bản 2021 edition trở lên)
- Cargo đã được cài đặt

## Các loại Benchmark

Dự án có 4 loại benchmark:

1. **comprehensive_bench** - Benchmark tổng hợp với nhiều kích thước dữ liệu (1K, 5K, 10K, 50K, 100K lệnh)
2. **exchange_bench** - Benchmark cho exchange engine và risk engine
3. **orderbook_optimized_bench** - So sánh hiệu năng các loại orderbook (Naive, Direct, DirectOptimized)
4. **advanced_orderbook_bench** - Benchmark cho các loại lệnh nâng cao (Post-Only, Iceberg, FOK, GTD, Stop Orders)

## Cách chạy Benchmark

### 1. Chạy Benchmark tổng hợp (Comprehensive Benchmark)

Benchmark này đo hiệu năng với nhiều kích thước dữ liệu khác nhau và tạo file CSV kết quả:

```bash
cargo bench --profile release --bench comprehensive_bench
```

**Lưu ý:** Với `cargo bench`, bạn cần dùng `--profile release` thay vì `--release`. Hoặc có thể bỏ qua vì `cargo bench` mặc định đã build ở release mode.

**Kết quả:**
- Tạo file `benchmark_results.csv` với các chỉ số: TPS, QPS, Memory, Duration
- Tạo file `plot_benchmark.py` để vẽ biểu đồ

**Xem kết quả:**
```bash
# Xem file CSV
cat benchmark_results.csv

# Tạo biểu đồ (cần cài matplotlib và pandas)
pip install matplotlib pandas
python plot_benchmark.py
```

### 2. Chạy Benchmark Exchange

```bash
cargo bench --profile release --bench exchange_bench
```

Benchmark này đo hiệu năng của:
- Exchange engine
- Risk engine
- So sánh NaiveOrderBook vs DirectOrderBook

### 3. Chạy Benchmark OrderBook Optimized

```bash
cargo bench --profile release --bench orderbook_optimized_bench
```

Benchmark này so sánh hiệu năng của:
- NaiveOrderBook
- DirectOrderBook
- DirectOrderBookOptimized

### 4. Chạy Benchmark Advanced OrderBook

```bash
cargo bench --profile release --bench advanced_orderbook_bench
```

Benchmark này đo hiệu năng các loại lệnh nâng cao:
- Post-Only orders
- Iceberg orders
- FOK (Fill-or-Kill) orders
- GTD (Good-Till-Date) orders
- Stop orders
- Mixed orders

## Chạy tất cả Benchmark

Để chạy tất cả các benchmark:

```bash
cargo bench --profile release
```

Hoặc đơn giản hơn (vì `cargo bench` mặc định đã build ở release mode):

```bash
cargo bench
```

## Xem kết quả chi tiết

Sau khi chạy benchmark, kết quả sẽ được lưu trong:
- `target/criterion/` - Kết quả chi tiết của Criterion (có thể xem bằng trình duyệt)
- `benchmark_results.csv` - Kết quả dạng CSV (chỉ từ comprehensive_bench)

### Xem báo cáo HTML của Criterion

Criterion tạo báo cáo HTML chi tiết. Để xem:

```bash
# Mở file HTML trong trình duyệt
# Windows:
start target/criterion/<benchmark_name>/report/index.html

# Linux/Mac:
open target/criterion/<benchmark_name>/report/index.html
# hoặc
xdg-open target/criterion/<benchmark_name>/report/index.html
```

## Tùy chọn Benchmark

### Chạy với số lần lặp cụ thể

```bash
cargo bench --profile release --bench comprehensive_bench -- --sample-size 100
```

### Chạy với thời gian tối thiểu

```bash
cargo bench --profile release --bench comprehensive_bench -- --measurement-time 10
```

### Chạy benchmark cụ thể trong nhóm

```bash
# Chỉ chạy benchmark "place_orders" trong comprehensive_bench
cargo bench --profile release --bench comprehensive_bench -- --bench advanced_orderbook/place_orders/1000
```

## Giải thích các chỉ số

- **TPS (Transactions Per Second)**: Số lượng lệnh xử lý mỗi giây
- **QPS (Queries Per Second)**: Số lượng khớp lệnh mỗi giây
- **Memory (MB)**: Sử dụng bộ nhớ tính bằng MB
- **Duration (ms)**: Thời gian xử lý tính bằng mili giây

## Lưu ý

1. **Luôn chạy với `--profile release`**: Benchmark chỉ có ý nghĩa khi chạy ở chế độ release (tối ưu hóa). Tuy nhiên, `cargo bench` mặc định đã build ở release mode, nên bạn có thể bỏ qua flag này.
2. **Đóng các ứng dụng khác**: Để có kết quả chính xác, nên đóng các ứng dụng khác đang chạy
3. **Chạy nhiều lần**: Để có kết quả ổn định, nên chạy benchmark nhiều lần và lấy giá trị trung bình
4. **Kiểm tra CPU**: Đảm bảo CPU không bị throttle (giảm tốc độ) do nhiệt độ

## Ví dụ kết quả mong đợi

Dựa trên README, kết quả benchmark mong đợi:

| Số lượng lệnh | TPS | QPS | Bộ nhớ (MB) | Độ trễ (ms) |
|---------|-----|-----|-----------|----------|
| 1,000 | ~6,559,183 | ~3,279,591 | ~0.19 | ~0.15 |
| 5,000 | ~7,242,000 | ~3,621,000 | ~0.95 | ~0.69 |
| 10,000 | ~7,247,910 | ~3,623,955 | ~1.91 | ~1.38 |
| 50,000 | ~3,834,037 | ~1,917,018 | ~9.54 | ~13.04 |
| 100,000 | ~4,968,213 | ~2,484,106 | ~19.07 | ~20.13 |

## Troubleshooting

### Lỗi: "benchmark not found"

Đảm bảo file benchmark đã được cấu hình trong `Cargo.toml`:
```toml
[[bench]]
name = "benchmark_name"
harness = false
```

### Kết quả không ổn định

- Chạy lại với `--sample-size` lớn hơn
- Đảm bảo không có ứng dụng khác đang sử dụng CPU
- Kiểm tra nhiệt độ CPU

### Không tạo được biểu đồ

Cài đặt các thư viện Python cần thiết:
```bash
pip install matplotlib pandas numpy
```

