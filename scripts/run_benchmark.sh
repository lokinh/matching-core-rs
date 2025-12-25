#!/bin/bash

# Chạy benchmark và tạo báo cáo hiệu năng

echo "=== Chạy benchmark engine khớp lệnh ==="

# Chạy benchmark
cargo bench --bench comprehensive_bench 2>&1 | tee benchmark_output.txt

# Nếu đã tạo file CSV, thử tạo biểu đồ
if [ -f "benchmark_results.csv" ]; then
    echo "Tạo biểu đồ hiệu năng..."
    python3 plot_benchmark.py 2>/dev/null || echo "Cần cài đặt matplotlib và pandas: pip3 install matplotlib pandas"
fi

echo "Benchmark hoàn thành!"

