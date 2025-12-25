
import matplotlib.pyplot as plt
import pandas as pd
import numpy as np

# Đọc dữ liệu
df = pd.read_csv('benchmark_results.csv')

# Tạo biểu đồ
fig, axes = plt.subplots(2, 2, figsize=(14, 10))
fig.suptitle('Chỉ số hiệu năng engine khớp lệnh', fontsize=16, fontweight='bold')

# Biểu đồ đường TPS
axes[0, 0].plot(df['Orders'], df['TPS'], marker='o', linewidth=2, markersize=8, color='#2E86AB')
axes[0, 0].set_xlabel('Số lượng lệnh', fontsize=12)
axes[0, 0].set_ylabel('TPS (lệnh/giây)', fontsize=12)
axes[0, 0].set_title('Thông lượng (TPS)', fontsize=13, fontweight='bold')
axes[0, 0].grid(True, alpha=0.3)
axes[0, 0].set_xscale('log')

# Biểu đồ đường QPS
axes[0, 1].plot(df['Orders'], df['QPS'], marker='s', linewidth=2, markersize=8, color='#A23B72')
axes[0, 1].set_xlabel('Số lượng lệnh', fontsize=12)
axes[0, 1].set_ylabel('QPS (khớp/giây)', fontsize=12)
axes[0, 1].set_title('Tốc độ khớp lệnh (QPS)', fontsize=13, fontweight='bold')
axes[0, 1].grid(True, alpha=0.3)
axes[0, 1].set_xscale('log')

# Biểu đồ đường sử dụng bộ nhớ
axes[1, 0].plot(df['Orders'], df['Memory_MB'], marker='^', linewidth=2, markersize=8, color='#F18F01')
axes[1, 0].set_xlabel('Số lượng lệnh', fontsize=12)
axes[1, 0].set_ylabel('Sử dụng bộ nhớ (MB)', fontsize=12)
axes[1, 0].set_title('Chiếm dụng bộ nhớ', fontsize=13, fontweight='bold')
axes[1, 0].grid(True, alpha=0.3)
axes[1, 0].set_xscale('log')

# Biểu đồ đường độ trễ
axes[1, 1].plot(df['Orders'], df['Duration_MS'], marker='d', linewidth=2, markersize=8, color='#C73E1D')
axes[1, 1].set_xlabel('Số lượng lệnh', fontsize=12)
axes[1, 1].set_ylabel('Thời gian xử lý (mili giây)', fontsize=12)
axes[1, 1].set_title('Độ trễ', fontsize=13, fontweight='bold')
axes[1, 1].grid(True, alpha=0.3)
axes[1, 1].set_xscale('log')

plt.tight_layout()
plt.savefig('benchmark_results.png', dpi=300, bbox_inches='tight')
print('Biểu đồ đã lưu vào benchmark_results.png')

