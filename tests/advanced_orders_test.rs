use matching_core::api::*;
use matching_core::core::orderbook::{OrderBook, AdvancedOrderBook};

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

#[test]
fn test_post_only_order() {
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Treo lệnh bán giá 10000
    let mut ask_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 10000,
        size: 10,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 10000,
        timestamp: 1000,
        ..Default::default()
    };
    book.new_order(&mut ask_cmd);
    
    // Lệnh mua Post-Only giá 10000 (sẽ khớp ngay, nên bị từ chối)
    let mut bid_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 10000,
        size: 5,
        action: OrderAction::Bid,
        order_type: OrderType::PostOnly,
        reserve_price: 10000,
        timestamp: 1001,
        ..Default::default()
    };
    book.new_order(&mut bid_cmd);
    
    // Nên tạo sự kiện từ chối
    assert_eq!(bid_cmd.matcher_events.len(), 1);
    assert_eq!(bid_cmd.matcher_events[0].event_type, MatcherEventType::Reject);
    assert_eq!(bid_cmd.matcher_events[0].size, 5);
    
    // Lệnh mua Post-Only giá 9999 (không khớp, nên treo thành công)
    let mut bid_cmd2 = OrderCommand {
        uid: 2,
        order_id: 3,
        symbol: 1,
        price: 9999,
        size: 5,
        action: OrderAction::Bid,
        order_type: OrderType::PostOnly,
        reserve_price: 9999,
        timestamp: 1002,
        ..Default::default()
    };
    book.new_order(&mut bid_cmd2);
    
    // Không nên có sự kiện (treo thành công)
    assert_eq!(bid_cmd2.matcher_events.len(), 0);
    assert_eq!(book.get_total_bid_volume(), 5);
}

#[test]
fn test_stop_limit_order() {
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Đặt lệnh mua cắt lỗ: giá kích hoạt 10500
    let mut stop_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 10600,  // Giá giới hạn
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::StopLimit,
        reserve_price: 10600,
        timestamp: 1000,
        stop_price: Some(10500),  // Giá kích hoạt
        ..Default::default()
    };
    book.new_order(&mut stop_cmd);
    
    // Lệnh cắt lỗ không nên vào sổ lệnh ngay
    assert_eq!(book.get_total_bid_volume(), 0);
    
    // Treo lệnh bán 10400
    let mut ask_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 10400,
        size: 5,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 10400,
        timestamp: 1001,
        ..Default::default()
    };
    book.new_order(&mut ask_cmd);
    
    // Lệnh mua khớp giá 10400, kích hoạt lệnh cắt lỗ
    let mut bid_cmd = OrderCommand {
        uid: 3,
        order_id: 3,
        symbol: 1,
        price: 10600,
        size: 5,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        reserve_price: 10600,
        timestamp: 1002,
        ..Default::default()
    };
    book.new_order(&mut bid_cmd);
    
    // Nên có sự kiện khớp lệnh
    assert!(!bid_cmd.matcher_events.is_empty());
    
    // Lưu ý: Lệnh cắt lỗ sau khi kích hoạt nên chuyển thành lệnh giới hạn vào sổ lệnh
    // Do giá khớp < giá kích hoạt cắt lỗ, lệnh cắt lỗ không nên kích hoạt (cắt lỗ mua cần giá tăng)
}

#[test]
fn test_iceberg_order() {
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Lệnh bán iceberg: tổng 100, hiển thị 10
    let mut iceberg_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 10000,
        size: 100,
        action: OrderAction::Ask,
        order_type: OrderType::Iceberg,
        reserve_price: 10000,
        timestamp: 1000,
        visible_size: Some(10),
        ..Default::default()
    };
    book.new_order(&mut iceberg_cmd);
    
    // Truy vấn độ sâu thị trường, chỉ nên hiển thị 10
    let l2 = book.get_l2_data(1);
    assert_eq!(l2.ask_volumes.len(), 1);
    assert_eq!(l2.ask_volumes[0], 10);  // Chỉ hiển thị 10
    
    // Lệnh mua khớp 10
    let mut bid_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 10000,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::Ioc,
        reserve_price: 10000,
        timestamp: 1001,
        ..Default::default()
    };
    book.new_order(&mut bid_cmd);
    
    // Nên khớp 10
    assert_eq!(bid_cmd.matcher_events.len(), 1);
    assert_eq!(bid_cmd.matcher_events[0].size, 10);
    
    // Sổ lệnh nên còn 90 (sau khi làm mới hiển thị 10)
    assert_eq!(book.get_total_ask_volume(), 90);
}

#[test]
fn test_fok_order() {
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Treo lệnh bán 5
    let mut ask_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 10000,
        size: 5,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 10000,
        timestamp: 1000,
        ..Default::default()
    };
    book.new_order(&mut ask_cmd);
    
    // Lệnh mua FOK 10 (không thể khớp toàn bộ)
    let mut fok_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 10000,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::Fok,
        reserve_price: 10000,
        timestamp: 1001,
        ..Default::default()
    };
    book.new_order(&mut fok_cmd);
    
    // Nên bị từ chối toàn bộ
    assert_eq!(fok_cmd.matcher_events.len(), 1);
    assert_eq!(fok_cmd.matcher_events[0].event_type, MatcherEventType::Reject);
    assert_eq!(fok_cmd.matcher_events[0].size, 10);
    
    // Sổ lệnh nên giữ nguyên
    assert_eq!(book.get_total_ask_volume(), 5);
}

#[test]
fn test_gtd_order() {
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Lệnh bán GTD, thời gian hết hạn 2000
    let mut gtd_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 10000,
        size: 10,
        action: OrderAction::Ask,
        order_type: OrderType::Gtd(2000),
        reserve_price: 10000,
        timestamp: 1000,
        expire_time: Some(2000),
        ..Default::default()
    };
    book.new_order(&mut gtd_cmd);
    
    assert_eq!(book.get_total_ask_volume(), 10);
    
    // Thời gian chưa hết hạn, lệnh mua nên khớp được
    let mut bid_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 10000,
        size: 5,
        action: OrderAction::Bid,
        order_type: OrderType::Ioc,
        reserve_price: 10000,
        timestamp: 1500,  // Chưa hết hạn
        ..Default::default()
    };
    book.new_order(&mut bid_cmd);
    
    assert_eq!(bid_cmd.matcher_events.len(), 1);
    assert_eq!(bid_cmd.matcher_events[0].size, 5);
    
    // Sau khi thời gian hết hạn, lệnh mua không nên khớp được
    let mut bid_cmd2 = OrderCommand {
        uid: 2,
        order_id: 3,
        symbol: 1,
        price: 10000,
        size: 5,
        action: OrderAction::Bid,
        order_type: OrderType::Ioc,
        reserve_price: 10000,
        timestamp: 2500,  // Đã hết hạn
        ..Default::default()
    };
    book.new_order(&mut bid_cmd2);
    
    // Nên bị từ chối (vì lệnh đã hết hạn)
    assert_eq!(bid_cmd2.matcher_events.len(), 1);
    assert_eq!(bid_cmd2.matcher_events[0].event_type, MatcherEventType::Reject);
}

#[test]
fn test_perpetual_swap() {
    let mut spec = create_symbol_spec();
    spec.symbol_type = SymbolType::PerpetualSwap;
    
    let mut book = AdvancedOrderBook::new(spec);
    
    // Lệnh mua hợp đồng vĩnh viễn
    let mut bid_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 50000,
        size: 1,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        reserve_price: 50000,
        timestamp: 1000,
        ..Default::default()
    };
    book.new_order(&mut bid_cmd);
    
    // Lệnh bán hợp đồng vĩnh viễn
    let mut ask_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 50000,
        size: 1,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 50000,
        timestamp: 1001,
        ..Default::default()
    };
    book.new_order(&mut ask_cmd);
    
    // Nên khớp lệnh
    assert_eq!(ask_cmd.matcher_events.len(), 1);
    assert_eq!(ask_cmd.matcher_events[0].event_type, MatcherEventType::Trade);
}

#[test]
fn test_call_option() {
    let mut spec = create_symbol_spec();
    spec.symbol_type = SymbolType::CallOption;
    
    let mut book = AdvancedOrderBook::new(spec);
    
    // Lệnh mua quyền chọn mua
    let mut bid_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 500,  // Phí quyền chọn
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        reserve_price: 500,
        timestamp: 1000,
        ..Default::default()
    };
    book.new_order(&mut bid_cmd);
    
    // Lệnh bán quyền chọn mua
    let mut ask_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 500,
        size: 5,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 500,
        timestamp: 1001,
        ..Default::default()
    };
    book.new_order(&mut ask_cmd);
    
    // Nên khớp 5
    assert_eq!(ask_cmd.matcher_events.len(), 1);
    assert_eq!(ask_cmd.matcher_events[0].size, 5);
    assert_eq!(book.get_total_bid_volume(), 5);
}

#[test]
fn test_put_option() {
    let mut spec = create_symbol_spec();
    spec.symbol_type = SymbolType::PutOption;
    
    let mut book = AdvancedOrderBook::new(spec);
    
    // Giao dịch quyền chọn bán
    let mut ask_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 300,
        size: 20,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        reserve_price: 300,
        timestamp: 1000,
        ..Default::default()
    };
    book.new_order(&mut ask_cmd);
    
    let mut bid_cmd = OrderCommand {
        uid: 2,
        order_id: 2,
        symbol: 1,
        price: 300,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::Ioc,
        reserve_price: 300,
        timestamp: 1001,
        ..Default::default()
    };
    book.new_order(&mut bid_cmd);
    
    // Nên khớp 10
    assert_eq!(bid_cmd.matcher_events.len(), 1);
    assert_eq!(bid_cmd.matcher_events[0].size, 10);
    assert_eq!(book.get_total_ask_volume(), 10);
}

#[test]
fn test_day_order() {
    let mut book = AdvancedOrderBook::new(create_symbol_spec());
    
    // Lệnh Day (có hiệu lực trong ngày)
    let mut day_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 10000,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::Day,
        reserve_price: 10000,
        timestamp: 1000,
        ..Default::default()
    };
    book.new_order(&mut day_cmd);
    
    assert_eq!(book.get_total_bid_volume(), 10);
    
    // Lệnh Day nên có thể bị hủy
    let mut cancel_cmd = OrderCommand {
        uid: 1,
        order_id: 1,
        symbol: 1,
        price: 0,
        size: 0,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        reserve_price: 0,
        timestamp: 1001,
        ..Default::default()
    };
    let result = book.cancel_order(&mut cancel_cmd);
    
    assert_eq!(result, CommandResultCode::Success);
    assert_eq!(book.get_total_bid_volume(), 0);
}

