use matching_core::api::*;
use matching_core::core::orderbook::{DirectOrderBookOptimized, OrderBook};

fn main() {
    println!("=== Test DirectOrderBookOptimized ===\n");

    let spec = CoreSymbolSpecification {
        symbol_id: 100,
        symbol_type: SymbolType::CurrencyExchangePair,
        base_currency: 2,
        quote_currency: 1,
        base_scale_k: 100,
        quote_scale_k: 1,
        taker_fee: 10,
        maker_fee: 5,
        ..Default::default()
    };

    let mut orderbook = DirectOrderBookOptimized::new(spec);

    // Test 1: Treo lệnh bán
    println!("Test 1: Treo lệnh bán");
    let mut cmd1 = OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1001,
        order_id: 5001,
        symbol: 100,
        price: 100,
        size: 10,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        ..Default::default()
    };
    orderbook.new_order(&mut cmd1);
    println!("  Lệnh bán treo thành công, giá: 100, số lượng: 10");

    // Test 2: Treo lệnh mua (khớp ngay)
    println!("\nTest 2: Lệnh mua khớp ngay");
    let mut cmd2 = OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1002,
        order_id: 5002,
        symbol: 100,
        price: 100,
        reserve_price: 100,
        size: 5,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        ..Default::default()
    };
    orderbook.new_order(&mut cmd2);
    println!("  Số sự kiện khớp lệnh: {}", cmd2.matcher_events.len());
    for event in &cmd2.matcher_events {
        println!("    Khớp lệnh: Giá={}, Số lượng={}", event.price, event.size);
    }

    // Test 3: Dữ liệu L2
    println!("\nTest 3: Độ sâu thị trường L2");
    let l2 = orderbook.get_l2_data(5);
    println!("  Giá lệnh bán: {:?}", l2.ask_prices);
    println!("  Số lượng lệnh bán: {:?}", l2.ask_volumes);
    println!("  Giá lệnh mua: {:?}", l2.bid_prices);
    println!("  Số lượng lệnh mua: {:?}", l2.bid_volumes);

    // Test 4: Lệnh IOC
    println!("\nTest 4: Lệnh IOC");
    let mut cmd3 = OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1003,
        order_id: 5003,
        symbol: 100,
        price: 100,
        reserve_price: 100,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::Ioc,
        ..Default::default()
    };
    orderbook.new_order(&mut cmd3);
    println!("  Số sự kiện khớp lệnh: {}", cmd3.matcher_events.len());
    for event in &cmd3.matcher_events {
        match event.event_type {
            MatcherEventType::Trade => println!("    Khớp lệnh: Số lượng={}", event.size),
            MatcherEventType::Reject => println!("    Từ chối: Số lượng={}", event.size),
            _ => {}
        }
    }

    // Test 5: Thống kê hiệu năng
    println!("\nTest 5: Thống kê hiệu năng");
    println!("  Tổng lượng lệnh bán: {}", orderbook.get_total_ask_volume());
    println!("  Tổng lượng lệnh mua: {}", orderbook.get_total_bid_volume());
    println!("  Số lớp giá lệnh bán: {}", orderbook.get_ask_buckets_count());
    println!("  Số lớp giá lệnh mua: {}", orderbook.get_bid_buckets_count());

    println!("\n=== Test hoàn thành ===");
    println!("Tính năng tối ưu: Bố cục bộ nhớ SOA + Cấp phát trước hồ chứa lệnh + Khớp lệnh hàng loạt SIMD");
}

