use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

pub type UserId = u64;
pub type OrderId = u64;
pub type SymbolId = i32;
pub type Currency = i32;
pub type Price = i64;
pub type Size = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub enum OrderAction {
    Ask,
    Bid,
}

impl OrderAction {
    pub fn opposite(self) -> Self {
        match self {
            OrderAction::Ask => OrderAction::Bid,
            OrderAction::Bid => OrderAction::Ask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub enum OrderType {
    Gtc,              // Good-Till-Cancel
    Ioc,              // Immediate-or-Cancel
    Fok,              // Fill-or-Kill
    FokBudget,        // FOK with budget
    IocBudget,        // IOC with budget
    PostOnly,         // Chỉ làm Maker, không ăn lệnh
    StopLimit,        // Lệnh giới hạn cắt lỗ
    StopMarket,       // Lệnh thị trường cắt lỗ
    Iceberg,          // Lệnh iceberg
    Day,              // Có hiệu lực trong ngày
    Gtd(i64),         // Good-Till-Date (timestamp)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub enum SymbolType {
    CurrencyExchangePair,  // Giao dịch giao ngay
    FuturesContract,       // Hợp đồng tương lai
    PerpetualSwap,         // Hợp đồng vĩnh viễn
    CallOption,            // Quyền chọn mua
    PutOption,             // Quyền chọn bán
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub enum CommandResultCode {
    New,
    ValidForMatchingEngine,
    Success,
    Accepted,
    
    // Auth
    AuthInvalidUser,
    
    // Risk
    RiskNsf,
    RiskInvalidReserveBidPrice,
    RiskAskPriceLowerThanFee,
    RiskMarginTradingDisabled,
    
    // Matching
    MatchingInvalidOrderBookId,
    MatchingUnknownOrderId,
    MatchingUnsupportedCommand,
    MatchingMoveFailedPriceOverRiskLimit,
    MatchingReduceFailedWrongSize,
    MatchingInvalidOrderSize,
    
    // State
    StatePersistRiskEngineFailed,
    StatePersistMatchingEngineFailed,
    
    // User
    UserMgmtUserAlreadyExists,
    
    // Other
    InvalidSymbol,
    UnsupportedSymbolType,
    BinaryCommandFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct CoreSymbolSpecification {
    pub symbol_id: SymbolId,
    pub symbol_type: SymbolType,
    pub base_currency: Currency,
    pub quote_currency: Currency,
    pub base_scale_k: i64,
    pub quote_scale_k: i64,
    pub taker_fee: i64,
    pub maker_fee: i64,
    pub margin_buy: i64,
    pub margin_sell: i64,
}

impl Default for CoreSymbolSpecification {
    fn default() -> Self {
        Self {
            symbol_id: 0,
            symbol_type: SymbolType::CurrencyExchangePair,
            base_currency: 0,
            quote_currency: 0,
            base_scale_k: 1,
            quote_scale_k: 1,
            taker_fee: 0,
            maker_fee: 0,
            margin_buy: 0,
            margin_sell: 0,
        }
    }
}
