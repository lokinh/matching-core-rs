use matching_core::api::*;
use matching_core::core::exchange::{ExchangeCore, ExchangeConfig, ProducerType, WaitStrategyType};
use std::time::{Duration, Instant};
use std::sync::Arc;

/// Cấu hình test tải
struct LoadTestConfig {
    num_orders: usize,
    batch_size: usize,
}

fn main() {
    println!("=== Test tải hiệu năng Matching Engine (TPS/QPS) ===\n");

    let config = LoadTestConfig {
        num_orders: 1_000_000, // 1 triệu lệnh
        batch_size: 1000,
    };

    println!("Quy mô test: {} lệnh", config.num_orders);

    // 1. Test DirectOrderBook (pipeline nghiệp vụ core)
    run_load_test("DirectOrderBook + Pipeline", &config);
    
    // 2. Lưu ý về môi trường
    println!("\nTest hoàn thành. Lưu ý, hiệu năng bị ảnh hưởng bởi môi trường phần cứng, cài đặt CPU affinity và tần suất cập nhật dữ liệu L2.");
}

fn run_load_test(name: &str, config: &LoadTestConfig) {
    let exchange_config = ExchangeConfig {
        ring_buffer_size: 64 * 1024,
        matching_engines_num: 1,
        risk_engines_num: 1,
        producer_type: ProducerType::Single,
        wait_strategy: WaitStrategyType::BusySpin,
    };
    
    let mut core = ExchangeCore::new(exchange_config);
    
    // Sử dụng bộ đếm nguyên tử để theo dõi tiến trình xử lý
    use std::sync::atomic::{AtomicUsize, Ordering};
    let processed_count = Arc::new(AtomicUsize::new(0));
    let count_clone = processed_count.clone();
    
    core.set_result_consumer(Arc::new(move |_cmd| {
        count_clone.fetch_add(1, Ordering::SeqCst);
    }));

    // Khởi tạo cặp giao dịch (thực hiện trước khi khởi động để đồng bộ thiết lập vào Pipeline)
    core.add_symbol(CoreSymbolSpecification {
        symbol_id: 1,
        symbol_type: SymbolType::CurrencyExchangePair,
        base_currency: 1,
        quote_currency: 2,
        base_scale_k: 100,
        quote_scale_k: 1,
        taker_fee: 10,
        maker_fee: 5,
        ..Default::default()
    });

    // Khởi động pipeline bất đồng bộ
    core.startup();
    
    // Khởi tạo nhiều người dùng và nạp tiền
    println!("[{}] Đang khởi tạo môi trường...", name);
    let init_user_count = 10000;
    for uid in 1..=init_user_count {
        core.submit_command(OrderCommand {
            command: OrderCommandType::AddUser,
            uid,
            ..Default::default()
        });
        core.submit_command(OrderCommand {
            command: OrderCommandType::BalanceAdjustment,
            uid,
            symbol: 2,
            price: 10_000_000,
            ..Default::default()
        });
        core.submit_command(OrderCommand {
            command: OrderCommandType::BalanceAdjustment,
            uid,
            symbol: 1,
            price: 10_000_000,
            ..Default::default()
        });
    }

    // Giai đoạn làm nóng
    println!("[{}] Đang làm nóng...", name);
    let warmup_count = 1000;
    for i in 0..warmup_count {
        simulate_order(&mut core, i);
    }

    // Test chính thức
    println!("[{}] Đang thực hiện test tải...", name);
    let start = Instant::now();
    
    for i in 0..config.num_orders {
        simulate_order(&mut core, i as u64);
    }
    
    // Chờ tất cả tin nhắn bất đồng bộ được xử lý xong (init + warmup + num_orders)
    // Lưu ý: init bao gồm 3*init_user_count
    let expected = 3 * (init_user_count as usize) + (warmup_count as usize) + (config.num_orders as usize);
    while processed_count.load(Ordering::Acquire) < expected {
        std::hint::spin_loop();
    }
    
    let duration = start.elapsed();
    let tps = config.num_orders as f64 / duration.as_secs_f64();
    let latency_avg = duration.as_nanos() as f64 / config.num_orders as f64;

    println!("\n--- Kết quả {} ---", name);
    println!("Tổng thời gian: {:?}", duration);
    println!("Thông lượng trung bình (TPS): {:.2}", tps);
    println!("Độ trễ trung bình: {:.2} ns", latency_avg);
}

#[inline(always)]
fn simulate_order(core: &mut ExchangeCore, i: u64) {
    let uid = (i % 10000) + 1;
    let action = if i % 2 == 0 { OrderAction::Bid } else { OrderAction::Ask };
    
    // Mô phỏng mua bán thực tế luân phiên, ID lệnh tăng dần
    core.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid,
        order_id: i + 100_000,
        symbol: 1,
        price: 100 + (i % 10) as i64,
        size: 1,
        action,
        order_type: OrderType::Gtc,
        ..Default::default()
    });
}
