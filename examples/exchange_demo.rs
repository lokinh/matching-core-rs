use matching_core::api::*;
use matching_core::core::exchange::{ExchangeConfig, ExchangeCore};
use std::sync::Arc;

fn main() {
    let config = ExchangeConfig::default();
    let mut core = ExchangeCore::new(config);

    core.set_result_consumer(Arc::new(|cmd| {
        println!("Kết quả: {:?} - {:?}", cmd.command, cmd.result_code);
    }));

    // Thêm cặp giao dịch
    core.add_symbol(CoreSymbolSpecification {
        symbol_id: 100,
        symbol_type: SymbolType::CurrencyExchangePair,
        base_currency: 2,
        quote_currency: 1,
        base_scale_k: 100,
        quote_scale_k: 1,
        taker_fee: 10,
        maker_fee: 5,
        margin_buy: 0,
        margin_sell: 0,
    });

    // Thêm người dùng
    core.submit_command(OrderCommand {
        command: OrderCommandType::AddUser,
        uid: 1001,
        ..Default::default()
    });
    core.submit_command(OrderCommand {
        command: OrderCommandType::AddUser,
        uid: 1002,
        ..Default::default()
    });

    // Nạp tiền
    core.submit_command(OrderCommand {
        command: OrderCommandType::BalanceAdjustment,
        uid: 1001,
        symbol: 1,
        price: 100000,
        order_id: 1,
        ..Default::default()
    });
    core.submit_command(OrderCommand {
        command: OrderCommandType::BalanceAdjustment,
        uid: 1002,
        symbol: 2,
        price: 10000,
        order_id: 2,
        ..Default::default()
    });

    println!("\n=== Test quy trình khớp lệnh đầy đủ ===\n");

    // 1. Treo nhiều lệnh bán
    core.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1002,
        order_id: 5002,
        symbol: 100,
        price: 100,
        size: 5,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        ..Default::default()
    });

    core.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1002,
        order_id: 5003,
        symbol: 100,
        price: 105,
        size: 5,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        ..Default::default()
    });

    // 2. Test lệnh FOK_BUDGET
    println!("Test lệnh FOK_BUDGET:");
    let fok_result = core.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1001,
        order_id: 5010,
        symbol: 100,
        price: 1050, // Ngân sách: 10 * 100 + 0 * 105 = 1000 < 1050 (thỏa mãn)
        reserve_price: 105,
        size: 10,
        action: OrderAction::Bid,
        order_type: OrderType::FokBudget,
        ..Default::default()
    });
    println!("  Kết quả: {:?}, Số sự kiện: {}", fok_result.result_code, fok_result.matcher_events.len());

    // 3. Test ID lệnh trùng lặp
    println!("\nTest ID lệnh trùng lặp:");
    core.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1002,
        order_id: 5020,
        symbol: 100,
        price: 110,
        size: 3,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        ..Default::default()
    });

    let dup_result = core.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1001,
        order_id: 5020, // ID trùng lặp
        symbol: 100,
        price: 110,
        reserve_price: 110,
        size: 3,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        ..Default::default()
    });
    println!("  Kết quả lệnh trùng lặp: {:?}, Số sự kiện: {}", dup_result.result_code, dup_result.matcher_events.len());

    // 4. Test di chuyển lệnh
    println!("\nTest di chuyển lệnh:");
    core.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1002,
        order_id: 5030,
        symbol: 100,
        price: 120,
        size: 5,
        action: OrderAction::Ask,
        order_type: OrderType::Gtc,
        ..Default::default()
    });

    let move_result = core.submit_command(OrderCommand {
        command: OrderCommandType::MoveOrder,
        uid: 1002,
        order_id: 5030,
        symbol: 100,
        price: 115, // Di chuyển đến giá mới
        ..Default::default()
    });
    println!("  Kết quả di chuyển lệnh: {:?}", move_result.result_code);

    // 5. Test giảm lệnh
    println!("\nTest giảm lệnh:");
    let reduce_result = core.submit_command(OrderCommand {
        command: OrderCommandType::ReduceOrder,
        uid: 1002,
        order_id: 5030,
        symbol: 100,
        size: 2, // Giảm 2
        ..Default::default()
    });
    println!("  Kết quả giảm lệnh: {:?}", reduce_result.result_code);
    // 6. Test serialize và snapshot
    println!("\n=== Test serialize nhị phân (Bincode) ===\n");
    let state = core.serialize_state();
    let serialized = bincode::serialize(&state).expect("Serialize thất bại");
    println!("Serialize thành công, kích thước byte: {}", serialized.len());

    let deserialized_state: matching_core::core::exchange::ExchangeState = 
        bincode::deserialize(&serialized).expect("Deserialize thất bại");
    
    let mut core2 = ExchangeCore::from_state(deserialized_state);
    println!("Khôi phục từ snapshot thành công");

    // Tiếp tục test trên core đã khôi phục
    core2.set_result_consumer(Arc::new(|cmd| {
        println!("Kết quả core đã khôi phục: {:?} - {:?}", cmd.command, cmd.result_code);
    }));

    println!("Gửi lệnh mới trên core đã khôi phục:");
    core2.submit_command(OrderCommand {
        command: OrderCommandType::PlaceOrder,
        uid: 1001,
        order_id: 6001,
        symbol: 100,
        price: 115,
        reserve_price: 115,
        size: 3,
        action: OrderAction::Bid,
        order_type: OrderType::Gtc,
        ..Default::default()
    });

    // 7. Test Journaling (WAL)
    println!("\n=== Test nhật ký ghi trước (WAL) ===\n");
    let journal_path = "exchange.wal";
    
    // Nếu file đã tồn tại thì xóa, đảm bảo test sạch
    let _ = std::fs::remove_file(journal_path);

    let mut core_wal = ExchangeCore::new(ExchangeConfig::default());
    core_wal.add_symbol(CoreSymbolSpecification {
        symbol_id: 200,
        symbol_type: SymbolType::CurrencyExchangePair,
        base_currency: 2,
        quote_currency: 1,
        base_scale_k: 100,
        quote_scale_k: 1,
        taker_fee: 10,
        maker_fee: 5,
        ..Default::default()
    });
    
    core_wal.enable_journaling(journal_path).expect("Bật WAL thất bại");
    
    println!("Gửi các lệnh ghi vào WAL...");
    core_wal.submit_command(OrderCommand {
        command: OrderCommandType::AddUser,
        uid: 2001,
        ..Default::default()
    });
    core_wal.submit_command(OrderCommand {
        command: OrderCommandType::BalanceAdjustment,
        uid: 2001,
        symbol: 1,
        price: 500000,
        ..Default::default()
    });

    println!("Khôi phục từ nhật ký sang core mới...");
    let mut core_recovered = ExchangeCore::new(ExchangeConfig::default());
    core_recovered.add_symbol(CoreSymbolSpecification {
        symbol_id: 200,
        symbol_type: SymbolType::CurrencyExchangePair,
        base_currency: 2,
        quote_currency: 1,
        base_scale_k: 100,
        quote_scale_k: 1,
        taker_fee: 10,
        maker_fee: 5,
        ..Default::default()
    });

    core_recovered.replay_journal(journal_path).expect("Phát lại WAL thất bại");
    println!("Khôi phục WAL thành công");

    // Dọn dẹp file test
    let _ = std::fs::remove_file(journal_path);

    // 8. Test cơ chế Snapshotting
    println!("\n=== Test snapshot trạng thái (Snapshotting) ===\n");
    let snapshot_dir = "snapshots";
    
    // Dọn dẹp thư mục snapshot trước đó
    let _ = std::fs::remove_dir_all(snapshot_dir);

    let mut core_snap = ExchangeCore::new(ExchangeConfig::default());
    core_snap.enable_snapshotting(snapshot_dir).expect("Bật snapshot thất bại");
    
    core_snap.add_symbol(CoreSymbolSpecification {
        symbol_id: 300,
        symbol_type: SymbolType::CurrencyExchangePair,
        base_currency: 2,
        quote_currency: 1,
        base_scale_k: 100,
        quote_scale_k: 1,
        taker_fee: 10,
        maker_fee: 5,
        ..Default::default()
    });

    println!("Tạo snapshot (ID: 1)...");
    core_snap.take_snapshot(1).expect("Tạo snapshot thất bại");

    println!("Khôi phục từ thư mục snapshot sang core mới...");
    let mut core_restored = ExchangeCore::new(ExchangeConfig::default());
    core_restored.enable_snapshotting(snapshot_dir).expect("Core mới bật snapshot thất bại");
    
    let recovered = core_restored.load_latest_snapshot().expect("Tải snapshot thất bại");
    assert!(recovered, "Không tìm thấy snapshot hợp lệ");
    println!("Khôi phục từ snapshot thành công");

    // Dọn dẹp thư mục test
    let _ = std::fs::remove_dir_all(snapshot_dir);
}
