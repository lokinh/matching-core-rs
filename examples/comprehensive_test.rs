use matching_core::api::*;
use matching_core::core::orderbook::{OrderBook, AdvancedOrderBook, DirectOrderBookOptimized, NaiveOrderBook};
use std::time::Instant;

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
    println!("=== Bộ test tổng hợp engine khớp lệnh ===\n");

    // 1. Test chức năng cơ bản
    test_basic_matching();
    
    // 2. Test loại lệnh nâng cao
    test_advanced_order_types();
    
    // 3. Test nhiều sản phẩm giao dịch
    test_multiple_symbol_types();
    
    // 4. Test so sánh hiệu năng
    test_performance_comparison();
    
    // 5. Test tải
    test_stress_scenarios();
    
    // 6. Test điều kiện biên
    test_edge_cases();
    
    println!("\n=== Tất cả test đã hoàn thành ===");
}

fn test_basic_matching() {
    println!("1. Test chức năng khớp lệnh cơ bản");
    
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Treo lệnh bán
    let mut ask = create_order(1, 1, 10000, 100, OrderAction::Ask, OrderType::Gtc);
    book.new_order(&mut ask);
    assert_eq!(book.get_total_ask_volume(), 100);
    
    // Lệnh mua khớp
    let mut bid = create_order(2, 2, 10000, 50, OrderAction::Bid, OrderType::Ioc);
    book.new_order(&mut bid);
    assert_eq!(bid.matcher_events.len(), 1);
    assert_eq!(bid.matcher_events[0].size, 50);
    assert_eq!(book.get_total_ask_volume(), 50);
    
    println!("   ✓ Khớp lệnh cơ bản hoạt động bình thường\n");
}

fn test_advanced_order_types() {
    println!("2. Test loại lệnh nâng cao");
    
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Post-Only
    let mut ask1 = create_order(1, 1, 10000, 10, OrderAction::Ask, OrderType::Gtc);
    book.new_order(&mut ask1);
    
    let mut post_only = create_order(2, 2, 10000, 5, OrderAction::Bid, OrderType::PostOnly);
    book.new_order(&mut post_only);
    assert_eq!(post_only.matcher_events[0].event_type, MatcherEventType::Reject);
    
    // Iceberg
    let mut iceberg = OrderCommand {
        uid: 3,
        order_id: 3,
        symbol: 1,
        price: 9999,
        size: 100,
        action: OrderAction::Bid,
        order_type: OrderType::Iceberg,
        reserve_price: 9999,
        timestamp: 1000,
        visible_size: Some(10),
        ..Default::default()
    };
    book.new_order(&mut iceberg);
    let l2 = book.get_l2_data(5);
    assert!(l2.bid_volumes.iter().any(|&v| v == 10));
    
    // FOK
    let mut fok = create_order(4, 4, 10000, 20, OrderAction::Bid, OrderType::Fok);
    book.new_order(&mut fok);
    assert_eq!(fok.matcher_events[0].event_type, MatcherEventType::Reject);
    
    // GTD
    let mut gtd = OrderCommand {
        uid: 5,
        order_id: 5,
        symbol: 1,
        price: 10001,
        size: 20,
        action: OrderAction::Ask,
        order_type: OrderType::Gtd(2000),
        reserve_price: 10001,
        timestamp: 1000,
        expire_time: Some(2000),
        ..Default::default()
    };
    book.new_order(&mut gtd);
    assert_eq!(book.get_total_ask_volume(), 30); // 10 (ask1) + 20 (gtd)
    
    println!("   ✓ Post-Only、Iceberg、FOK、GTD test đã qua\n");
}

fn test_multiple_symbol_types() {
    println!("3. Test nhiều sản phẩm giao dịch");
    
    let types = vec![
        (SymbolType::CurrencyExchangePair, "Giao ngay"),
        (SymbolType::FuturesContract, "Hợp đồng tương lai"),
        (SymbolType::PerpetualSwap, "Hợp đồng vĩnh viễn"),
        (SymbolType::CallOption, "Quyền chọn mua"),
        (SymbolType::PutOption, "Quyền chọn bán"),
    ];
    
    for (symbol_type, name) in types {
        let spec = CoreSymbolSpecification {
            symbol_id: 1,
            symbol_type,
            base_currency: 0,
            quote_currency: 1,
            base_scale_k: 1,
            quote_scale_k: 1,
            taker_fee: 0,
            maker_fee: 0,
            margin_buy: 0,
            margin_sell: 0,
        };
        
        let mut book = AdvancedOrderBook::new(spec);
        
        let mut ask = create_order(1, 1, 10000, 10, OrderAction::Ask, OrderType::Gtc);
        book.new_order(&mut ask);
        
        let mut bid = create_order(2, 2, 10000, 5, OrderAction::Bid, OrderType::Ioc);
        book.new_order(&mut bid);
        
        assert_eq!(bid.matcher_events[0].event_type, MatcherEventType::Trade);
        println!("   ✓ {} khớp lệnh bình thường", name);
    }
    println!();
}

fn test_performance_comparison() {
    println!("4. Test so sánh hiệu năng");
    
    let num_orders = 10000;
    
    // AdvancedOrderBook
    let start = Instant::now();
    let mut advanced = AdvancedOrderBook::new(create_symbol_spec());
    for i in 0..num_orders {
        let mut cmd = create_order(1, i, 10000 + (i % 100) as i64, 10, 
            if i % 2 == 0 { OrderAction::Ask } else { OrderAction::Bid }, 
            OrderType::Gtc);
        advanced.new_order(&mut cmd);
    }
    let advanced_time = start.elapsed();
    
    // DirectOrderBookOptimized
    let start = Instant::now();
    let mut optimized = DirectOrderBookOptimized::new(create_symbol_spec());
    for i in 0..num_orders {
        let mut cmd = create_order(1, i + num_orders as u64, 10000 + (i % 100) as i64, 10,
            if i % 2 == 0 { OrderAction::Ask } else { OrderAction::Bid },
            OrderType::Gtc);
        optimized.new_order(&mut cmd);
    }
    let optimized_time = start.elapsed();
    
    // NaiveOrderBook
    let start = Instant::now();
    let mut naive = NaiveOrderBook::new(create_symbol_spec());
    for i in 0..num_orders {
        let mut cmd = create_order(1, i + (num_orders * 2) as u64, 10000 + (i % 100) as i64, 10,
            if i % 2 == 0 { OrderAction::Ask } else { OrderAction::Bid },
            OrderType::Gtc);
        naive.new_order(&mut cmd);
    }
    let naive_time = start.elapsed();
    
    println!("   AdvancedOrderBook: {:?} ({:.2} ops/s)", 
        advanced_time, num_orders as f64 / advanced_time.as_secs_f64());
    println!("   DirectOrderBookOptimized: {:?} ({:.2} ops/s)", 
        optimized_time, num_orders as f64 / optimized_time.as_secs_f64());
    println!("   NaiveOrderBook: {:?} ({:.2} ops/s)", 
        naive_time, num_orders as f64 / naive_time.as_secs_f64());
    println!();
}

fn test_stress_scenarios() {
    println!("5. Các kịch bản test tải");
    
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Kịch bản 1: Nhiều lệnh iceberg
    println!("   Kịch bản 1: 1000 lệnh iceberg");
    let start = Instant::now();
    for i in 0..1000 {
        let mut cmd = OrderCommand {
            uid: 1,
            order_id: i,
            symbol: 1,
            price: 10000,
            size: 1000,
            action: OrderAction::Ask,
            order_type: OrderType::Iceberg,
            reserve_price: 10000,
            timestamp: 1000,
            visible_size: Some(10),
            ..Default::default()
        };
        book.new_order(&mut cmd);
    }
    println!("      Thời gian hoàn thành: {:?}", start.elapsed());
    
    // Kịch bản 2: Nhiều khớp lệnh
    println!("   Kịch bản 2: 10000 lần khớp lệnh");
    let start = Instant::now();
    for i in 0..10000 {
        let mut cmd = create_order(2, 1000 + i, 10000, 1, OrderAction::Bid, OrderType::Ioc);
        book.new_order(&mut cmd);
    }
    println!("      Thời gian hoàn thành: {:?}", start.elapsed());
    
    // Kịch bản 3: Các loại lệnh hỗn hợp
    println!("   Kịch bản 3: Các loại lệnh hỗn hợp (5000 lệnh)");
    let start = Instant::now();
    for i in 0..5000 {
        let order_type = match i % 5 {
            0 => OrderType::Gtc,
            1 => OrderType::Ioc,
            2 => OrderType::PostOnly,
            3 => OrderType::Fok,
            _ => OrderType::Day,
        };
        let mut cmd = create_order(3, 11000 + i, 10000 + (i % 10) as i64, 10,
            if i % 2 == 0 { OrderAction::Ask } else { OrderAction::Bid },
            order_type);
        book.new_order(&mut cmd);
    }
    println!("      Thời gian hoàn thành: {:?}", start.elapsed());
    println!();
}

fn test_edge_cases() {
    println!("6. Test điều kiện biên");
    
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Lệnh số lượng bằng không
    let mut zero = create_order(1, 1, 10000, 0, OrderAction::Ask, OrderType::Gtc);
    book.new_order(&mut zero);
    assert_eq!(book.get_total_ask_volume(), 0);
    
    // Giá cực cao
    let mut high = create_order(1, 2, i64::MAX / 2, 10, OrderAction::Ask, OrderType::Gtc);
    book.new_order(&mut high);
    assert_eq!(book.get_total_ask_volume(), 10);
    
    // Nhiều ID lệnh (sử dụng book mới để tránh ảnh hưởng từ các lệnh trước)
    let mut book2 = AdvancedOrderBook::new(create_symbol_spec());
    for i in 0..100 {
        let mut cmd = create_order(1, u64::MAX - 100 + i, 10000 + i as i64, 1, 
            OrderAction::Ask, OrderType::Gtc);
        book2.new_order(&mut cmd);
    }
    assert_eq!(book2.get_ask_buckets_count(), 100);
    
    // Hủy lệnh không tồn tại
    let mut cancel = create_order(1, 99999, 0, 0, OrderAction::Bid, OrderType::Gtc);
    let result = book.cancel_order(&mut cancel);
    assert_eq!(result, CommandResultCode::MatchingUnknownOrderId);
    
    println!("   ✓ Test điều kiện biên đã qua\n");
}

fn create_order(
    uid: UserId,
    order_id: OrderId,
    price: Price,
    size: Size,
    action: OrderAction,
    order_type: OrderType,
) -> OrderCommand {
    OrderCommand {
        uid,
        order_id,
        symbol: 1,
        price,
        size,
        action,
        order_type,
        reserve_price: price,
        timestamp: 1000,
        ..Default::default()
    }
}

