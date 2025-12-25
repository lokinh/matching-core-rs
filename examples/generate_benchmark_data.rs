use matching_core::api::*;
use matching_core::core::orderbook::{OrderBook, AdvancedOrderBook};
use std::time::Instant;
use std::fs::File;
use std::io::Write;

fn create_symbol_spec() -> CoreSymbolSpecification {
    CoreSymbolSpecification {
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
    }
}

fn main() {
    println!("=== Tạo dữ liệu benchmark ===\n");
    
    let mut file = File::create("benchmark_results.csv").unwrap();
    writeln!(file, "Orders,TPS,QPS,Memory_MB,Duration_MS").unwrap();
    
    let sizes = vec![1000, 5000, 10000, 50000, 100000];
    
    for &size in &sizes {
        println!("Quy mô test: {} lệnh", size);
        
        let start = Instant::now();
        let mut book = AdvancedOrderBook::new(create_symbol_spec());
        let mut trades = 0;
        
        for i in 0..size {
            let mut cmd = OrderCommand {
                uid: 1,
                order_id: i as u64,
                symbol: 1,
                price: 10000 + (i % 100) as i64,
                size: 10,
                action: if i % 2 == 0 { OrderAction::Ask } else { OrderAction::Bid },
                order_type: OrderType::Gtc,
                reserve_price: 10000 + (i % 100) as i64,
                timestamp: 1000,
                ..Default::default()
            };
            book.new_order(&mut cmd);
            trades += cmd.matcher_events.len();
        }
        
        let duration = start.elapsed();
        let tps = size as f64 / duration.as_secs_f64();
        let qps = trades as f64 / duration.as_secs_f64();
        let memory = estimate_memory(size);
        let duration_ms = duration.as_secs_f64() * 1000.0;
        
        writeln!(file, "{},{:.2},{:.2},{:.2},{:.2}", 
            size, tps, qps, memory, duration_ms).unwrap();
        
        println!("  TPS: {:.2}, QPS: {:.2}, Bộ nhớ: {:.2} MB, Thời gian: {:.2} ms\n",
            tps, qps, memory, duration_ms);
    }
    
    println!("Dữ liệu đã lưu vào benchmark_results.csv");
    println!("Chạy 'python3 scripts/plot_benchmark.py' để tạo biểu đồ");
}

fn estimate_memory(orders: usize) -> f64 {
    // Ước tính sử dụng bộ nhớ (MB)
    // AdvancedOrderBook mỗi lệnh khoảng 200 byte
    let bytes_per_order = 200;
    (orders * bytes_per_order) as f64 / (1024.0 * 1024.0)
}

