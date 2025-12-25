use crate::api::*;
use crate::core::users::UserProfileService;
use ahash::AHashMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct RiskEngine {
    shard_id: usize,
    shard_mask: u64,
    user_service: UserProfileService,
    symbols: AHashMap<SymbolId, CoreSymbolSpecification>, // Sử dụng AHashMap khi chạy
}

impl RiskEngine {
    pub fn new(shard_id: usize, num_shards: usize) -> Self {
        assert!(num_shards.is_power_of_two());
        Self {
            shard_id,
            shard_mask: (num_shards - 1) as u64,
            user_service: UserProfileService::new(),
            symbols: AHashMap::new(),
        }
    }

    fn uid_for_this_shard(&self, uid: UserId) -> bool {
        self.shard_mask == 0 || (uid & self.shard_mask) == self.shard_id as u64
    }

    pub fn add_symbol(&mut self, spec: CoreSymbolSpecification) {
        self.symbols.insert(spec.symbol_id, spec);
    }

    // R1: Pre-process
    pub fn pre_process(&mut self, cmd: &mut OrderCommand) {
        match cmd.command {
            OrderCommandType::PlaceOrder => {
                if self.uid_for_this_shard(cmd.uid) {
                    cmd.result_code = self.place_order_risk_check(cmd);
                }
            }
            OrderCommandType::AddUser => {
                if self.uid_for_this_shard(cmd.uid) {
                    cmd.result_code = if self.user_service.add_user(cmd.uid) {
                        CommandResultCode::Success
                    } else {
                        CommandResultCode::UserMgmtUserAlreadyExists
                    };
                }
            }
            OrderCommandType::BalanceAdjustment => {
                if self.uid_for_this_shard(cmd.uid) {
                    cmd.result_code = self.user_service.balance_adjustment(
                        cmd.uid,
                        cmd.symbol,
                        cmd.price,
                        cmd.order_id as i64,
                    );
                }
            }
            _ => {}
        }
    }

    fn place_order_risk_check(&mut self, cmd: &OrderCommand) -> CommandResultCode {
        let Some(profile) = self.user_service.get_user_mut(cmd.uid) else {
            return CommandResultCode::AuthInvalidUser;
        };

        let Some(spec) = self.symbols.get(&cmd.symbol) else {
            return CommandResultCode::InvalidSymbol;
        };

        let currency = match cmd.action {
            OrderAction::Bid => spec.quote_currency,
            OrderAction::Ask => spec.base_currency,
        };

        let hold_amount = match cmd.action {
            OrderAction::Bid => {
                let price = if matches!(cmd.order_type, OrderType::FokBudget | OrderType::IocBudget) {
                    cmd.price
                } else {
                    cmd.reserve_price
                };
                cmd.size * price * spec.quote_scale_k + cmd.size * spec.taker_fee
            }
            OrderAction::Ask => cmd.size * spec.base_scale_k,
        };

        let balance = profile.accounts.entry(currency).or_insert(0);
        if *balance >= hold_amount {
            *balance -= hold_amount;
            CommandResultCode::ValidForMatchingEngine
        } else {
            CommandResultCode::RiskNsf
        }
    }

    // R2: Xử lý sau - Thanh toán
    pub fn post_process(&mut self, cmd: &mut OrderCommand) {
        if cmd.matcher_events.is_empty() {
            return;
        }

        let Some(spec) = self.symbols.get(&cmd.symbol).cloned() else {
            return;
        };

        let taker_sell = cmd.action == OrderAction::Ask;

        for event in &cmd.matcher_events {
            match event.event_type {
                MatcherEventType::Trade => {
                    self.handle_trade_event(cmd, event, &spec, taker_sell);
                }
                MatcherEventType::Reject | MatcherEventType::Reduce => {
                    self.handle_reject_event(cmd, event, &spec, taker_sell);
                }
            }
        }
        cmd.result_code = CommandResultCode::Success;
    }

    /// Xử lý sự kiện khớp lệnh
    fn handle_trade_event(
        &mut self,
        cmd: &OrderCommand,
        event: &MatcherTradeEvent,
        spec: &CoreSymbolSpecification,
        taker_sell: bool,
    ) {
        // Thanh toán cho Taker
        if self.uid_for_this_shard(cmd.uid) {
            if let Some(taker) = self.user_service.get_user_mut(cmd.uid) {
                if taker_sell {
                    // Lệnh bán: Thu về tiền quote
                    let amount = event.size * event.price * spec.quote_scale_k - event.size * spec.taker_fee;
                    *taker.accounts.entry(spec.quote_currency).or_insert(0) += amount;
                } else {
                    // Lệnh mua: Hoàn lại chênh lệch giá + Thu về tiền base
                    let price_diff = event.bidder_hold_price - event.price;
                    let refund = event.size * price_diff * spec.quote_scale_k;
                    *taker.accounts.entry(spec.quote_currency).or_insert(0) += refund;
                    *taker.accounts.entry(spec.base_currency).or_insert(0) += event.size * spec.base_scale_k;
                }
            }
        }

        // Thanh toán cho Maker
        if self.uid_for_this_shard(event.matched_order_uid) {
            if let Some(maker) = self.user_service.get_user_mut(event.matched_order_uid) {
                if taker_sell {
                    // Taker bán => Maker mua
                    let price_diff = event.bidder_hold_price - event.price;
                    let refund = event.size * price_diff * spec.quote_scale_k;
                    *maker.accounts.entry(spec.quote_currency).or_insert(0) += refund;
                    *maker.accounts.entry(spec.base_currency).or_insert(0) += event.size * spec.base_scale_k;
                } else {
                    // Taker mua => Maker bán
                    let amount = event.size * event.price * spec.quote_scale_k - event.size * spec.maker_fee;
                    *maker.accounts.entry(spec.quote_currency).or_insert(0) += amount;
                }
            }
        }
    }

    /// Xử lý sự kiện từ chối/hủy
    fn handle_reject_event(
        &mut self,
        cmd: &OrderCommand,
        event: &MatcherTradeEvent,
        spec: &CoreSymbolSpecification,
        taker_sell: bool,
    ) {
        if !self.uid_for_this_shard(cmd.uid) {
            return;
        }

        let Some(profile) = self.user_service.get_user_mut(cmd.uid) else {
            return;
        };

        // Hoàn lại tiền đã đóng băng
        if taker_sell {
            let refund = event.size * spec.base_scale_k;
            *profile.accounts.entry(spec.base_currency).or_insert(0) += refund;
        } else {
            let refund = event.size * event.bidder_hold_price * spec.quote_scale_k + event.size * spec.taker_fee;
            *profile.accounts.entry(spec.quote_currency).or_insert(0) += refund;
        }
    }
}

