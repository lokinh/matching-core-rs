#!/usr/bin/env python3
"""
Tạo biểu đồ đường chỉ số hiệu năng engine khớp lệnh
"""

import matplotlib.pyplot as plt
import pandas as pd
import numpy as np
import sys
import os

# Thiết lập font tiếng Việt
plt.rcParams['font.sans-serif'] = ['Arial Unicode MS', 'SimHei', 'DejaVu Sans']
plt.rcParams['axes.unicode_minus'] = False

def plot_benchmark_results():
    """Đọc dữ liệu CSV và tạo biểu đồ đường nhiều chỉ số"""
    
    csv_file = 'benchmark_results.csv'
    if not os.path.exists(csv_file):
        print(f"Lỗi: Không tìm thấy {csv_file}")
        print("Vui lòng chạy trước: cargo run --example generate_benchmark_data --release")
        sys.exit(1)
    
    # Đọc dữ liệu
    try:
        df = pd.read_csv(csv_file)
    except Exception as e:
        print(f"Đọc file CSV thất bại: {e}")
        sys.exit(1)
    
    # Tạo biểu đồ
    fig, axes = plt.subplots(2, 2, figsize=(16, 12))
    fig.suptitle('Chỉ số hiệu năng engine khớp lệnh', fontsize=18, fontweight='bold', y=0.995)
    
    # Bảng màu
    colors = {
        'tps': '#2E86AB',      # Xanh dương
        'qps': '#A23B72',      # Tím
        'memory': '#F18F01',   # Cam
        'duration': '#C73E1D'  # Đỏ
    }
    
    # 1. Biểu đồ đường TPS
    ax1 = axes[0, 0]
    ax1.plot(df['Orders'], df['TPS'], marker='o', linewidth=2.5, 
             markersize=10, color=colors['tps'], label='TPS')
    ax1.set_xlabel('Số lượng lệnh', fontsize=13, fontweight='bold')
    ax1.set_ylabel('TPS (lệnh/giây)', fontsize=13, fontweight='bold')
    ax1.set_title('Thông lượng (Transactions Per Second)', fontsize=14, fontweight='bold')
    ax1.grid(True, alpha=0.3, linestyle='--')
    ax1.set_xscale('log')
    ax1.legend(fontsize=11)
    
    # Thêm nhãn giá trị
    for i, (x, y) in enumerate(zip(df['Orders'], df['TPS'])):
        if i % 2 == 0:  # Chỉ đánh dấu một số điểm
            ax1.annotate(f'{y:.0f}', (x, y), textcoords="offset points", 
                        xytext=(0,10), ha='center', fontsize=9)
    
    # 2. Biểu đồ đường QPS
    ax2 = axes[0, 1]
    ax2.plot(df['Orders'], df['QPS'], marker='s', linewidth=2.5, 
             markersize=10, color=colors['qps'], label='QPS')
    ax2.set_xlabel('Số lượng lệnh', fontsize=13, fontweight='bold')
    ax2.set_ylabel('QPS (khớp/giây)', fontsize=13, fontweight='bold')
    ax2.set_title('Tốc độ khớp lệnh (Queries Per Second)', fontsize=14, fontweight='bold')
    ax2.grid(True, alpha=0.3, linestyle='--')
    ax2.set_xscale('log')
    ax2.legend(fontsize=11)
    
    # Thêm nhãn giá trị
    for i, (x, y) in enumerate(zip(df['Orders'], df['QPS'])):
        if i % 2 == 0:
            ax2.annotate(f'{y:.0f}', (x, y), textcoords="offset points", 
                        xytext=(0,10), ha='center', fontsize=9)
    
    # 3. Biểu đồ đường sử dụng bộ nhớ
    ax3 = axes[1, 0]
    ax3.plot(df['Orders'], df['Memory_MB'], marker='^', linewidth=2.5, 
             markersize=10, color=colors['memory'], label='Sử dụng bộ nhớ')
    ax3.set_xlabel('Số lượng lệnh', fontsize=13, fontweight='bold')
    ax3.set_ylabel('Sử dụng bộ nhớ (MB)', fontsize=13, fontweight='bold')
    ax3.set_title('Chiếm dụng bộ nhớ', fontsize=14, fontweight='bold')
    ax3.grid(True, alpha=0.3, linestyle='--')
    ax3.set_xscale('log')
    ax3.legend(fontsize=11)
    
    # Thêm nhãn giá trị
    for i, (x, y) in enumerate(zip(df['Orders'], df['Memory_MB'])):
        if i % 2 == 0:
            ax3.annotate(f'{y:.1f}MB', (x, y), textcoords="offset points", 
                        xytext=(0,10), ha='center', fontsize=9)
    
    # 4. Biểu đồ đường độ trễ
    ax4 = axes[1, 1]
    ax4.plot(df['Orders'], df['Duration_MS'], marker='d', linewidth=2.5, 
             markersize=10, color=colors['duration'], label='Thời gian xử lý')
    ax4.set_xlabel('Số lượng lệnh', fontsize=13, fontweight='bold')
    ax4.set_ylabel('Thời gian xử lý (mili giây)', fontsize=13, fontweight='bold')
    ax4.set_title('Độ trễ', fontsize=14, fontweight='bold')
    ax4.grid(True, alpha=0.3, linestyle='--')
    ax4.set_xscale('log')
    ax4.legend(fontsize=11)
    
    # Thêm nhãn giá trị
    for i, (x, y) in enumerate(zip(df['Orders'], df['Duration_MS'])):
        if i % 2 == 0:
            ax4.annotate(f'{y:.1f}ms', (x, y), textcoords="offset points", 
                        xytext=(0,10), ha='center', fontsize=9)
    
    # Điều chỉnh bố cục
    plt.tight_layout(rect=[0, 0, 1, 0.98])
    
    # Lưu biểu đồ
    output_file = 'benchmark_results.png'
    plt.savefig(output_file, dpi=300, bbox_inches='tight', facecolor='white')
    print(f'✓ Biểu đồ đã lưu vào {output_file}')
    
    # Hiển thị thống kê
    print('\n=== Thống kê hiệu năng ===')
    print(f"TPS tối đa: {df['TPS'].max():.2f}")
    print(f"QPS tối đa: {df['QPS'].max():.2f}")
    print(f"Bộ nhớ tối đa: {df['Memory_MB'].max():.2f} MB")
    print(f"Độ trễ tối đa: {df['Duration_MS'].max():.2f} ms")

if __name__ == '__main__':
    plot_benchmark_results()

