use matching_core::api::*;
use matching_core::core::orderbook::{OrderBook, AdvancedOrderBook};

fn main() {
    println!("=== Demo các loại lệnh nâng cao ===\n");

    // Tạo cặp giao dịch giao ngay
    let spot_spec = CoreSymbolSpecification {
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

    let mut book = AdvancedOrderBook::new(spot_spec);

    // 1. Lệnh Post-Only
    println!("1. Lệnh Post-Only (chỉ làm Maker)");
    let mut ask1 = create_order(1, 1, 10000, 10, OrderAction::Ask, OrderType::Gtc);
    book.new_order(&mut ask1);
    println!("   Treo lệnh bán @10000 x 10");

    let mut post_only = create_order(2, 2, 10000, 5, OrderAction::Bid, OrderType::PostOnly);
    book.new_order(&mut post_only);
    if !post_only.matcher_events.is_empty() && post_only.matcher_events[0].event_type == MatcherEventType::Reject {
        println!("   ✓ Lệnh mua Post-Only @10000 bị từ chối (sẽ khớp ngay)");
    }

    let mut post_only2 = create_order(2, 3, 9999, 5, OrderAction::Bid, OrderType::PostOnly);
    book.new_order(&mut post_only2);
    println!("   ✓ Lệnh mua Post-Only @9999 treo lệnh thành công\n");

    // 2. Lệnh iceberg
    println!("2. Lệnh iceberg (Iceberg Order)");
    let mut iceberg = OrderCommand {
        uid: 3,
        order_id: 4,
        symbol: 1,
        price: 9998,
        size: 100,
        action: OrderAction::Bid,
        order_type: OrderType::Iceberg,
        reserve_price: 9998,
        timestamp: 1000,
        visible_size: Some(10),
        ..Default::default()
    };
    book.new_order(&mut iceberg);
    println!("   Treo lệnh mua iceberg @9998 tổng 100, hiển thị 10");
    
    let l2 = book.get_l2_data(5);
    for (i, (price, vol)) in l2.bid_prices.iter().zip(l2.bid_volumes.iter()).enumerate() {
        println!("   Mua{}: {} x {}", i+1, price, vol);
    }
    println!();

    // 3. Lệnh FOK
    println!("3. Lệnh FOK (Fill-or-Kill)");
    let mut fok = create_order(4, 5, 10000, 20, OrderAction::Bid, OrderType::Fok);
    book.new_order(&mut fok);
    if !fok.matcher_events.is_empty() && fok.matcher_events[0].event_type == MatcherEventType::Reject {
        println!("   ✓ Lệnh mua FOK bị từ chối (không thể khớp toàn bộ 20, chỉ có 10)\n");
    }

    let mut fok2 = create_order(4, 6, 10000, 5, OrderAction::Bid, OrderType::Fok);
    book.new_order(&mut fok2);
    if !fok2.matcher_events.is_empty() && fok2.matcher_events[0].event_type == MatcherEventType::Trade {
        println!("   ✓ Lệnh mua FOK thành công (khớp toàn bộ 5)\n");
    }

    // 4. Lệnh GTD
    println!("4. Lệnh GTD (Good-Till-Date)");
    let mut gtd = OrderCommand {
        uid: 5,
        order_id: 7,
        symbol: 1,
        price: 10100,
        size: 20,
        action: OrderAction::Ask,
        order_type: OrderType::Gtd(2000),
        reserve_price: 10100,
        timestamp: 1000,
        expire_time: Some(2000),
        ..Default::default()
    };
    book.new_order(&mut gtd);
    println!("   Treo lệnh bán GTD @10100 x 20 (thời gian hết hạn 2000)");

    let mut bid_before = create_order(6, 8, 10100, 5, OrderAction::Bid, OrderType::Ioc);
    bid_before.timestamp = 1500;
    book.new_order(&mut bid_before);
    println!("   Thời gian 1500: Khớp {} đơn vị", 
        bid_before.matcher_events.iter().filter(|e| e.event_type == MatcherEventType::Trade)
            .map(|e| e.size).sum::<i64>());

    let mut bid_after = create_order(6, 9, 10100, 5, OrderAction::Bid, OrderType::Ioc);
    bid_after.timestamp = 2500;
    book.new_order(&mut bid_after);
    println!("   Thời gian 2500: Khớp {} đơn vị (lệnh đã hết hạn)\n", 
        bid_after.matcher_events.iter().filter(|e| e.event_type == MatcherEventType::Trade)
            .map(|e| e.size).sum::<i64>());

    // 5. Lệnh cắt lỗ
    println!("5. Lệnh cắt lỗ (Stop Order)");
    let mut stop = OrderCommand {
        uid: 7,
        order_id: 10,
        symbol: 1,
        price: 11000,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::StopLimit,
        reserve_price: 11000,
        timestamp: 3000,
        stop_price: Some(10500),
        ..Default::default()
    };
    book.new_order(&mut stop);
    println!("   Đặt lệnh mua cắt lỗ: Giá kích hoạt 10500, giá giới hạn 11000");
    println!("   (Khi giá tăng đến 10500 sẽ tự động kích hoạt)\n");

    // 6. Hợp đồng vĩnh viễn
    println!("6. Hợp đồng vĩnh viễn (Perpetual Swap)");
    let perp_spec = CoreSymbolSpecification {
        symbol_id: 2,
        symbol_type: SymbolType::PerpetualSwap,
        base_currency: 0,
        quote_currency: 1,
        base_scale_k: 1,
        quote_scale_k: 1,
        taker_fee: 0,
        maker_fee: 0,
        margin_buy: 0,
        margin_sell: 0,
    };
    let mut perp_book = AdvancedOrderBook::new(perp_spec);
    
    let mut perp_bid = OrderCommand {
        uid: 8,
        order_id: 11,
        symbol: 2,
        price: 50000,
        size: 1,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        reserve_price: 50000,
        timestamp: 4000,
        ..Default::default()
    };
    perp_book.new_order(&mut perp_bid);
    
    let mut perp_ask = OrderCommand {
        uid: 9,
        order_id: 12,
        symbol: 2,
        price: 50000,
        size: 1,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 50000,
        timestamp: 4001,
        ..Default::default()
    };
    perp_book.new_order(&mut perp_ask);
    println!("   ✓ Giao dịch hợp đồng vĩnh viễn: BTC-PERP @50000\n");

    // 7. Quyền chọn
    println!("7. Quyền chọn (Options)");
    let call_spec = CoreSymbolSpecification {
        symbol_id: 3,
        symbol_type: SymbolType::CallOption,
        base_currency: 0,
        quote_currency: 1,
        base_scale_k: 1,
        quote_scale_k: 1,
        taker_fee: 0,
        maker_fee: 0,
        margin_buy: 0,
        margin_sell: 0,
    };
    let mut option_book = AdvancedOrderBook::new(call_spec);
    
    let mut option_bid = OrderCommand {
        uid: 10,
        order_id: 13,
        symbol: 3,
        price: 500,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        reserve_price: 500,
        timestamp: 5000,
        ..Default::default()
    };
    option_book.new_order(&mut option_bid);
    
    let mut option_ask = OrderCommand {
        uid: 11,
        order_id: 14,
        symbol: 3,
        price: 500,
        size: 5,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 500,
        timestamp: 5001,
        ..Default::default()
    };
    option_book.new_order(&mut option_ask);
    println!("   ✓ Giao dịch quyền chọn mua: Phí quyền chọn 500, khớp 5 hợp đồng\n");

    // 8. Lệnh Day
    println!("8. Lệnh Day (có hiệu lực trong ngày)");
    let mut day = create_order(12, 15, 9900, 15, OrderAction::Bid, OrderType::Day);
    book.new_order(&mut day);
    println!("   ✓ Lệnh Day @9900 x 15 (có hiệu lực trong ngày)\n");

    // Độ sâu thị trường cuối cùng
    println!("=== Độ sâu thị trường cuối cùng (giao ngay) ===");
    let l2 = book.get_l2_data(10);
    
    println!("Bên bán:");
    for (price, vol) in l2.ask_prices.iter().rev().zip(l2.ask_volumes.iter().rev()) {
        println!("  {} x {}", price, vol);
    }
    
    println!("---");
    
    println!("Bên mua:");
    for (price, vol) in l2.bid_prices.iter().zip(l2.bid_volumes.iter()) {
        println!("  {} x {}", price, vol);
    }
    
    println!("\nTổng lượng mua: {}", book.get_total_bid_volume());
    println!("Tổng lượng bán: {}", book.get_total_ask_volume());
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

