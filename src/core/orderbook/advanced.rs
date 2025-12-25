use crate::api::*;
use ahash::AHashMap;
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Lệnh mở rộng (hỗ trợ tất cả loại lệnh)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdvancedOrder {
    order_id: OrderId,
    uid: UserId,
    price: Price,
    size: Size,
    filled: Size,
    action: OrderAction,
    order_type: OrderType,
    reserve_price: Price,
    timestamp: i64,
    
    // Các trường mở rộng
    stop_price: Option<Price>,      // Giá kích hoạt cắt lỗ
    visible_size: Option<Size>,     // Số lượng hiển thị của lệnh iceberg
    expire_time: Option<i64>,       // Thời gian hết hạn
    is_triggered: bool,             // Lệnh cắt lỗ đã được kích hoạt chưa
}

/// Mức giá (hỗ trợ lệnh iceberg)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdvancedBucket {
    price: Price,
    orders: SmallVec<[AdvancedOrder; 8]>,
    total_volume: Size,      // Tổng khối lượng lệnh treo thực tế
    visible_volume: Size,    // Tổng khối lượng lệnh treo hiển thị
}

impl AdvancedBucket {
    fn new(price: Price) -> Self {
        Self {
            price,
            orders: SmallVec::new(),
            total_volume: 0,
            visible_volume: 0,
        }
    }

    fn add(&mut self, order: AdvancedOrder) {
        let remaining = order.size - order.filled;
        self.total_volume += remaining;
        
        // Lệnh iceberg chỉ hiển thị một phần số lượng
        if let Some(visible) = order.visible_size {
            self.visible_volume += visible.min(remaining);
        } else {
            self.visible_volume += remaining;
        }
        
        self.orders.push(order);
    }

    fn remove(&mut self, order_id: OrderId) -> Option<AdvancedOrder> {
        if let Some(pos) = self.orders.iter().position(|o| o.order_id == order_id) {
            let order = self.orders.remove(pos);
            let remaining = order.size - order.filled;
            self.total_volume -= remaining;
            
            if let Some(visible) = order.visible_size {
                self.visible_volume -= visible.min(remaining);
            } else {
                self.visible_volume -= remaining;
            }
            
            Some(order)
        } else {
            None
        }
    }

    /// Khớp lệnh (hỗ trợ lệnh iceberg)
    fn match_order(&mut self, taker_size: Size, _taker_uid: UserId, current_time: i64) 
        -> (Size, SmallVec<[MatcherTradeEvent; 4]>) 
    {
        let mut matched_size = 0;
        let mut events = SmallVec::new();
        let mut to_remove = SmallVec::<[OrderId; 4]>::new();

        for order in &mut self.orders {
            // Kiểm tra lệnh đã hết hạn chưa
            if let Some(expire) = order.expire_time {
                if current_time > expire {
                    to_remove.push(order.order_id);
                    continue;
                }
            }

            let remaining = order.size - order.filled;
            let match_size = remaining.min(taker_size - matched_size);

            if match_size > 0 {
                order.filled += match_size;
                matched_size += match_size;
                
                // Cập nhật tổng lượng
                self.total_volume -= match_size;
                
                // Cập nhật lượng hiển thị (xử lý đặc biệt cho lệnh iceberg)
                if let Some(visible) = order.visible_size {
                    let old_visible = visible.min(remaining);
                    let new_remaining = order.size - order.filled;
                    let new_visible = visible.min(new_remaining);
                    self.visible_volume = self.visible_volume.saturating_sub(old_visible) + new_visible;
                } else {
                    self.visible_volume -= match_size;
                }

                events.push(MatcherTradeEvent::new_trade(
                    match_size,
                    self.price,
                    order.order_id,
                    order.uid,
                    order.reserve_price,
                ));

                if order.filled >= order.size {
                    to_remove.push(order.order_id);
                }

                if matched_size >= taker_size {
                    break;
                }
            }
        }

        // Loại bỏ các lệnh đã hoàn thành (không cập nhật tổng lượng, đã cập nhật ở trên)
        for oid in to_remove {
            if let Some(pos) = self.orders.iter().position(|o| o.order_id == oid) {
                self.orders.remove(pos);
            }
        }

        (matched_size, events)
    }
}

/// Sổ lệnh nâng cao (hỗ trợ tất cả loại lệnh)
#[derive(Clone, Serialize, Deserialize)]
pub struct AdvancedOrderBook {
    symbol_spec: CoreSymbolSpecification,
    
    // Các lệnh đang hoạt động
    ask_buckets: BTreeMap<Price, AdvancedBucket>,
    bid_buckets: BTreeMap<Price, AdvancedBucket>,
    order_map: AHashMap<OrderId, (Price, OrderAction)>,
    
    // Hồ chứa lệnh cắt lỗ (chưa kích hoạt)
    stop_orders: Vec<AdvancedOrder>,
    
    // Giá khớp lệnh mới nhất (dùng để kích hoạt lệnh cắt lỗ)
    last_trade_price: Option<Price>,
    
    // Bộ nhớ đệm giá tối ưu
    best_ask_price: Option<Price>,
    best_bid_price: Option<Price>,
}

impl AdvancedOrderBook {
    pub fn new(spec: CoreSymbolSpecification) -> Self {
        Self {
            symbol_spec: spec,
            ask_buckets: BTreeMap::new(),
            bid_buckets: BTreeMap::new(),
            order_map: AHashMap::with_capacity(1024),
            stop_orders: Vec::new(),
            last_trade_price: None,
            best_ask_price: None,
            best_bid_price: None,
        }
    }

    #[inline]
    fn update_best_prices(&mut self) {
        self.best_ask_price = self.ask_buckets.keys().next().copied();
        self.best_bid_price = self.bid_buckets.keys().next_back().copied();
    }

    /// Kiểm tra xem lệnh có khớp ngay lập tức không
    fn would_match(&self, cmd: &OrderCommand) -> bool {
        match cmd.action {
            OrderAction::Bid => {
                if let Some(best_ask) = self.best_ask_price {
                    cmd.price >= best_ask
                } else {
                    false
                }
            }
            OrderAction::Ask => {
                if let Some(best_bid) = self.best_bid_price {
                    cmd.price <= best_bid
                } else {
                    false
                }
            }
        }
    }

    /// Kiểm tra Post-Only
    fn check_post_only(&self, cmd: &OrderCommand) -> CommandResultCode {
        if self.would_match(cmd) {
            CommandResultCode::MatchingUnsupportedCommand // Post-Only từ chối
        } else {
            CommandResultCode::ValidForMatchingEngine
        }
    }

    /// Xử lý lệnh cắt lỗ
    fn process_stop_orders(&mut self, cmd: &mut OrderCommand) {
        if let Some(last_price) = self.last_trade_price {
            let mut triggered = Vec::new();
            
            for (idx, stop_order) in self.stop_orders.iter_mut().enumerate() {
                if let Some(stop_price) = stop_order.stop_price {
                    let should_trigger = match stop_order.action {
                        OrderAction::Bid => last_price >= stop_price,  // Cắt lỗ mua
                        OrderAction::Ask => last_price <= stop_price,  // Cắt lỗ bán
                    };

                    if should_trigger && !stop_order.is_triggered {
                        stop_order.is_triggered = true;
                        triggered.push(idx);
                    }
                }
            }

            // Kích hoạt các lệnh cắt lỗ đã được kích hoạt
            for idx in triggered.iter().rev() {
                let order = self.stop_orders.remove(*idx);
                let mut activate_cmd = OrderCommand {
                    uid: order.uid,
                    order_id: order.order_id,
                    symbol: cmd.symbol,
                    price: order.price,
                    size: order.size,
                    action: order.action,
                    order_type: order.order_type,
                    reserve_price: order.reserve_price,
                    timestamp: order.timestamp,
                    ..Default::default()
                };
                
                self.place_order_internal(&mut activate_cmd);
            }
        }
    }

    /// Đặt lệnh (tất cả loại)
    fn place_order(&mut self, cmd: &mut OrderCommand) {
        // Kiểm tra Post-Only
        if cmd.order_type == OrderType::PostOnly {
            if self.check_post_only(cmd) != CommandResultCode::ValidForMatchingEngine {
                cmd.matcher_events.push(MatcherTradeEvent::new_reject(cmd.size, cmd.price));
                return;
            }
        }

        // Lệnh cắt lỗ: Lưu tạm vào hồ chứa cắt lỗ
        if matches!(cmd.order_type, OrderType::StopLimit | OrderType::StopMarket) {
            let order = AdvancedOrder {
                order_id: cmd.order_id,
                uid: cmd.uid,
                price: cmd.price,
                size: cmd.size,
                filled: 0,
                action: cmd.action,
                order_type: cmd.order_type,
                reserve_price: cmd.reserve_price,
                timestamp: cmd.timestamp,
                stop_price: cmd.stop_price,
                visible_size: cmd.visible_size,
                expire_time: cmd.expire_time,
                is_triggered: false,
            };
            self.stop_orders.push(order);
            return;
        }

        self.place_order_internal(cmd);
    }

    /// Logic đặt lệnh nội bộ
    fn place_order_internal(&mut self, cmd: &mut OrderCommand) {
        // Kiểm tra lệnh trùng lặp
        if self.order_map.contains_key(&cmd.order_id) {
            let filled = self.try_match(cmd);
            if filled < cmd.size {
                cmd.matcher_events.push(MatcherTradeEvent::new_reject(cmd.size - filled, cmd.price));
            }
            return;
        }

        // FOK: Khớp toàn bộ hoặc hủy toàn bộ
        if cmd.order_type == OrderType::Fok {
            if !self.can_fill_completely(cmd) {
                cmd.matcher_events.push(MatcherTradeEvent::new_reject(cmd.size, cmd.price));
                return;
            }
        }

        let filled = self.try_match(cmd);

        // Cập nhật giá khớp lệnh mới nhất
        if filled > 0 {
            self.last_trade_price = Some(cmd.price);
            self.process_stop_orders(cmd);
        }

        // IOC/FOK: Không treo lệnh
        if matches!(cmd.order_type, OrderType::Ioc | OrderType::Fok) {
            if filled < cmd.size {
                cmd.matcher_events.push(MatcherTradeEvent::new_reject(cmd.size - filled, cmd.price));
            }
            return;
        }

        // GTC/Day/GTD/PostOnly/Iceberg: Treo lệnh
        if filled < cmd.size {
            let order = AdvancedOrder {
                order_id: cmd.order_id,
                uid: cmd.uid,
                price: cmd.price,
                size: cmd.size,
                filled,
                action: cmd.action,
                order_type: cmd.order_type,
                reserve_price: cmd.reserve_price,
                timestamp: cmd.timestamp,
                stop_price: None,
                visible_size: cmd.visible_size,
                expire_time: cmd.expire_time,
                is_triggered: false,
            };

            self.order_map.insert(cmd.order_id, (cmd.price, cmd.action));

            match cmd.action {
                OrderAction::Ask => {
                    self.ask_buckets
                        .entry(cmd.price)
                        .or_insert_with(|| AdvancedBucket::new(cmd.price))
                        .add(order);
                    if self.best_ask_price.is_none() || cmd.price < self.best_ask_price.unwrap() {
                        self.best_ask_price = Some(cmd.price);
                    }
                }
                OrderAction::Bid => {
                    self.bid_buckets
                        .entry(cmd.price)
                        .or_insert_with(|| AdvancedBucket::new(cmd.price))
                        .add(order);
                    if self.best_bid_price.is_none() || cmd.price > self.best_bid_price.unwrap() {
                        self.best_bid_price = Some(cmd.price);
                    }
                }
            }
        }
    }

    /// Kiểm tra xem có thể khớp hoàn toàn không (FOK)
    fn can_fill_completely(&self, cmd: &OrderCommand) -> bool {
        let buckets = match cmd.action {
            OrderAction::Bid => &self.ask_buckets,
            OrderAction::Ask => &self.bid_buckets,
        };

        let mut available = 0;
        for (price, bucket) in buckets.iter() {
            if (cmd.action == OrderAction::Bid && *price > cmd.price) ||
               (cmd.action == OrderAction::Ask && *price < cmd.price) {
                break;
            }
            available += bucket.total_volume;
            if available >= cmd.size {
                return true;
            }
        }
        false
    }

    /// Thử khớp lệnh
    fn try_match(&mut self, cmd: &mut OrderCommand) -> Size {
        let mut filled = 0;

        // Kiểm tra đường dẫn nhanh
        if (cmd.action == OrderAction::Bid && self.best_ask_price.map_or(true, |p| p > cmd.price)) ||
           (cmd.action == OrderAction::Ask && self.best_bid_price.map_or(true, |p| p < cmd.price)) {
            return 0;
        }

        let current_time = cmd.timestamp;

        match cmd.action {
            OrderAction::Bid => {
                let prices: Vec<Price> = self.ask_buckets.range(..=cmd.price).map(|(p, _)| *p).collect();
                
                for price in prices {
                    if filled >= cmd.size {
                        break;
                    }

                    if let Some(bucket) = self.ask_buckets.get_mut(&price) {
                        let (matched, events) = bucket.match_order(cmd.size - filled, cmd.uid, current_time);
                        filled += matched;
                        cmd.matcher_events.extend(events);

                        if bucket.total_volume == 0 {
                            self.ask_buckets.remove(&price);
                        }
                    }
                }
                self.update_best_prices();
            }
            OrderAction::Ask => {
                let prices: Vec<Price> = self.bid_buckets.range(cmd.price..).rev().map(|(p, _)| *p).collect();
                
                for price in prices {
                    if filled >= cmd.size {
                        break;
                    }

                    if let Some(bucket) = self.bid_buckets.get_mut(&price) {
                        let (matched, events) = bucket.match_order(cmd.size - filled, cmd.uid, current_time);
                        filled += matched;
                        cmd.matcher_events.extend(events);

                        if bucket.total_volume == 0 {
                            self.bid_buckets.remove(&price);
                        }
                    }
                }
                self.update_best_prices();
            }
        }

        filled
    }

    /// Hủy lệnh
    fn cancel_order(&mut self, cmd: &mut OrderCommand) -> CommandResultCode {
        // Kiểm tra lệnh đang hoạt động
        if let Some((price, action)) = self.order_map.remove(&cmd.order_id) {
            let buckets = match action {
                OrderAction::Ask => &mut self.ask_buckets,
                OrderAction::Bid => &mut self.bid_buckets,
            };

            if let Some(bucket) = buckets.get_mut(&price) {
                if let Some(order) = bucket.remove(cmd.order_id) {
                    cmd.matcher_events.push(MatcherTradeEvent::new_reject(
                        order.size - order.filled,
                        price
                    ));
                    cmd.action = action;

                    if bucket.total_volume == 0 {
                        buckets.remove(&price);
                        self.update_best_prices();
                    }

                    return CommandResultCode::Success;
                }
            }
        }

        // Kiểm tra hồ chứa lệnh cắt lỗ
        if let Some(pos) = self.stop_orders.iter().position(|o| o.order_id == cmd.order_id) {
            let order = self.stop_orders.remove(pos);
            cmd.matcher_events.push(MatcherTradeEvent::new_reject(order.size, order.price));
            return CommandResultCode::Success;
        }

        CommandResultCode::MatchingUnknownOrderId
    }
}

impl super::OrderBook for AdvancedOrderBook {
    fn new_order(&mut self, cmd: &mut OrderCommand) -> CommandResultCode {
        self.place_order(cmd);
        CommandResultCode::Success
    }

    fn cancel_order(&mut self, cmd: &mut OrderCommand) -> CommandResultCode {
        self.cancel_order(cmd)
    }

    fn move_order(&mut self, cmd: &mut OrderCommand) -> CommandResultCode {
        // Đơn giản hóa: Hủy trước rồi mới đặt lệnh
        let cancel_result = self.cancel_order(cmd);
        if cancel_result == CommandResultCode::Success {
            self.place_order(cmd);
        }
        cancel_result
    }

    fn reduce_order(&mut self, _cmd: &mut OrderCommand) -> CommandResultCode {
        CommandResultCode::MatchingUnsupportedCommand
    }

    fn get_symbol_spec(&self) -> &CoreSymbolSpecification {
        &self.symbol_spec
    }

    fn get_l2_data(&self, depth: usize) -> L2MarketData {
        let mut data = L2MarketData::new(depth);

        for (price, bucket) in self.ask_buckets.iter().take(depth) {
            data.ask_prices.push(*price);
            data.ask_volumes.push(bucket.visible_volume); // Lượng hiển thị
        }

        for (price, bucket) in self.bid_buckets.iter().rev().take(depth) {
            data.bid_prices.push(*price);
            data.bid_volumes.push(bucket.visible_volume); // Lượng hiển thị
        }

        data
    }

    fn get_order_by_id(&self, order_id: OrderId) -> Option<(Price, OrderAction)> {
        self.order_map.get(&order_id).copied()
    }

    fn get_total_ask_volume(&self) -> Size {
        self.ask_buckets.values().map(|b| b.total_volume).sum()
    }

    fn get_total_bid_volume(&self) -> Size {
        self.bid_buckets.values().map(|b| b.total_volume).sum()
    }

    fn get_ask_buckets_count(&self) -> usize {
        self.ask_buckets.len()
    }

    fn get_bid_buckets_count(&self) -> usize {
        self.bid_buckets.len()
    }

    fn serialize_state(&self) -> crate::core::orderbook::OrderBookState {
        crate::core::orderbook::OrderBookState::Advanced(self.clone())
    }
}

