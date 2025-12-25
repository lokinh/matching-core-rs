use super::types::*;
use super::events::*;
use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub enum OrderCommandType {
    PlaceOrder,
    MoveOrder,
    CancelOrder,
    ReduceOrder,
    OrderBookRequest,
    AddUser,
    BalanceAdjustment,
    SuspendUser,
    ResumeUser,
    BinaryDataCommand,
    BinaryDataQuery,
    Reset,
    Nop,
    PersistStateMatching,
    PersistStateRisk,
    GroupingControl,
    ShutdownSignal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct OrderCommand {
    pub command: OrderCommandType,
    pub result_code: CommandResultCode,
    
    pub uid: UserId,
    pub order_id: OrderId,
    pub symbol: SymbolId,
    pub price: Price,
    pub reserve_price: Price,
    pub size: Size,
    pub action: OrderAction,
    pub order_type: OrderType,
    
    pub timestamp: i64,
    pub events_group: u64,
    pub service_flags: i32,
    
    // Các trường mở rộng
    pub stop_price: Option<Price>,      // Giá kích hoạt cắt lỗ
    pub visible_size: Option<Size>,     // Số lượng hiển thị của lệnh iceberg
    pub expire_time: Option<i64>,       // Thời gian hết hạn (GTD)
    
    // Danh sách sự kiện khớp lệnh (dung lượng được cấp phát trước)
    pub matcher_events: Vec<MatcherTradeEvent>,
}

impl Default for OrderCommand {
    fn default() -> Self {
        Self {
            command: OrderCommandType::Nop,
            result_code: CommandResultCode::New,
            uid: 0,
            order_id: 0,
            symbol: 0,
            price: 0,
            reserve_price: 0,
            size: 0,
            action: OrderAction::Bid,
            order_type: OrderType::Gtc,
            timestamp: 0,
            events_group: 0,
            service_flags: 0,
            stop_price: None,
            visible_size: None,
            expire_time: None,
            matcher_events: Vec::with_capacity(4), // Cấp phát trước dung lượng cho 4 sự kiện
        }
    }
}
