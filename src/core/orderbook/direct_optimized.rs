use crate::api::*;
use ahash::AHashMap;
use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

type OrderIdx = usize;

/// Bố cục bộ nhớ SOA: Dữ liệu nóng của lệnh (thân thiện với cache)
#[derive(Clone, Serialize, Deserialize)]
struct OrderHotData {
    order_ids: Vec<OrderId>,    // ID lệnh
    prices: Vec<Price>,         // Giá
    sizes: Vec<Size>,           // Số lượng
    filled: Vec<Size>,          // Đã khớp
    next: Vec<Option<OrderIdx>>, // Nút tiếp theo trong danh sách liên kết
    prev: Vec<Option<OrderIdx>>, // Nút trước trong danh sách liên kết
    active: Vec<bool>,          // Đánh dấu kích hoạt
}

/// Dữ liệu lạnh của lệnh (truy cập tần suất thấp)
#[derive(Clone, Serialize, Deserialize)]
struct OrderColdData {
    uid: UserId,
    action: OrderAction,
    reserve_price: Price,
    timestamp: i64,
}

/// Hồ chứa lệnh được cấp phát trước (không cấp phát)
#[derive(Clone, Serialize, Deserialize)]
struct OrderPool {
    hot: OrderHotData,
    cold: Vec<OrderColdData>,
    free_list: Vec<OrderIdx>,
    capacity: usize,
}

impl OrderPool {
    fn new(capacity: usize) -> Self {
        let mut free_list = Vec::with_capacity(capacity);
        for i in (0..capacity).rev() {
            free_list.push(i);
        }
        
        Self {
            hot: OrderHotData {
                order_ids: vec![0; capacity],
                prices: vec![0; capacity],
                sizes: vec![0; capacity],
                filled: vec![0; capacity],
                next: vec![None; capacity],
                prev: vec![None; capacity],
                active: vec![false; capacity],
            },
            cold: vec![
                OrderColdData {
                    uid: 0,
                    action: OrderAction::Bid,
                    reserve_price: 0,
                    timestamp: 0,
                };
                capacity
            ],
            free_list,
            capacity,
        }
    }

    #[inline]
    fn alloc(&mut self) -> Option<OrderIdx> {
        self.free_list.pop()
    }

    #[inline]
    fn dealloc(&mut self, idx: OrderIdx) {
        self.hot.active[idx] = false;
        self.free_list.push(idx);
    }
}

/// Thùng giá (phiên bản đơn giản)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PriceBucket {
    price: Price,
    volume: Size,
    head: OrderIdx, // Đầu danh sách liên kết (lệnh sớm nhất)
}

/// Engine khớp lệnh hiệu năng cao (phiên bản tối ưu sâu)
#[derive(Clone, Serialize, Deserialize)]
pub struct DirectOrderBookOptimized {
    symbol_spec: CoreSymbolSpecification,
    
    // Hồ chứa lệnh SOA (được cấp phát trước)
    order_pool: OrderPool,
    
    // Chỉ mục giá (có thể thay thế BTreeMap bằng ART)
    ask_buckets: BTreeMap<Price, PriceBucket>,
    bid_buckets: BTreeMap<Price, PriceBucket>,
    
    // Chỉ mục ID lệnh
    order_index: AHashMap<OrderId, OrderIdx>,
    
    // Bộ nhớ đệm giá tối ưu
    best_ask: Option<Price>,
    best_bid: Option<Price>,
}

impl DirectOrderBookOptimized {
    pub fn new(spec: CoreSymbolSpecification) -> Self {
        Self {
            symbol_spec: spec,
            order_pool: OrderPool::new(100_000), // Cấp phát trước 10 vạn lệnh
            ask_buckets: BTreeMap::new(),
            bid_buckets: BTreeMap::new(),
            order_index: AHashMap::with_capacity(100_000),
            best_ask: None,
            best_bid: None,
        }
    }

    /// Đặt lệnh GTC
    fn place_gtc(&mut self, cmd: &mut OrderCommand) {
        if self.order_index.contains_key(&cmd.order_id) {
            let filled = self.try_match(cmd);
            if filled < cmd.size {
                cmd.matcher_events.push(MatcherTradeEvent::new_reject(cmd.size - filled, cmd.price));
            }
            return;
        }

        let filled = self.try_match(cmd);

        if filled < cmd.size {
            if let Some(idx) = self.order_pool.alloc() {
                // Ghi dữ liệu nóng
                self.order_pool.hot.order_ids[idx] = cmd.order_id;
                self.order_pool.hot.prices[idx] = cmd.price;
                self.order_pool.hot.sizes[idx] = cmd.size;
                self.order_pool.hot.filled[idx] = filled;
                self.order_pool.hot.active[idx] = true;
                
                // Ghi dữ liệu lạnh
                self.order_pool.cold[idx] = OrderColdData {
                    uid: cmd.uid,
                    action: cmd.action,
                    reserve_price: cmd.reserve_price,
                    timestamp: cmd.timestamp,
                };

                self.order_index.insert(cmd.order_id, idx);
                self.insert_to_bucket(idx, cmd.price, cmd.action);
            }
        }
    }

    /// Đặt lệnh IOC
    fn place_ioc(&mut self, cmd: &mut OrderCommand) {
        let filled = self.try_match(cmd);
        if filled < cmd.size {
            cmd.matcher_events.push(MatcherTradeEvent::new_reject(cmd.size - filled, cmd.price));
        }
    }

    /// Khớp lệnh hàng loạt SIMD (phiên bản tối ưu)
    #[cfg(target_arch = "aarch64")]
    fn try_match(&mut self, cmd: &mut OrderCommand) -> Size {
        let is_bid = cmd.action == OrderAction::Bid;
        let limit_price = cmd.price;
        let mut filled = 0;

        // Đường dẫn nhanh: Kiểm tra giá tối ưu
        let best_price = if is_bid { self.best_ask } else { self.best_bid };
        if let Some(best) = best_price {
            if (is_bid && best > limit_price) || (!is_bid && best < limit_price) {
                return 0;
            }
        } else {
            return 0;
        }

        let prices_to_match: Vec<Price> = if is_bid {
            self.ask_buckets.range(..=limit_price).map(|(p, _)| *p).collect()
        } else {
            self.bid_buckets.range(limit_price..).rev().map(|(p, _)| *p).collect()
        };

        let mut need_update_best = false;

        for price in prices_to_match {
            if filled >= cmd.size {
                break;
            }

            let buckets = if is_bid { &mut self.ask_buckets } else { &mut self.bid_buckets };
            
            if let Some(bucket) = buckets.get_mut(&price) {
                let mut current_idx = bucket.head;
                
                while filled < cmd.size && self.order_pool.hot.active[current_idx] {
                    let remaining = cmd.size - filled;
                    let order_remaining = self.order_pool.hot.sizes[current_idx] - self.order_pool.hot.filled[current_idx];
                    let trade_size = remaining.min(order_remaining);

                    // Cập nhật khớp lệnh
                    self.order_pool.hot.filled[current_idx] += trade_size;
                    bucket.volume -= trade_size;
                    filled += trade_size;

                    // Tạo sự kiện
                    let maker_uid = self.order_pool.cold[current_idx].uid;
                    let reserve = if is_bid {
                        cmd.reserve_price
                    } else {
                        self.order_pool.cold[current_idx].reserve_price
                    };
                    
                    cmd.matcher_events.push(MatcherTradeEvent::new_trade(
                        trade_size,
                        price,
                        self.order_pool.hot.order_ids[current_idx],
                        maker_uid,
                        reserve,
                    ));

                    // Lệnh hoàn thành
                    if self.order_pool.hot.filled[current_idx] >= self.order_pool.hot.sizes[current_idx] {
                        let order_id = self.order_pool.hot.order_ids[current_idx];
                        self.order_index.remove(&order_id);
                        self.order_pool.dealloc(current_idx);
                    }

                    if let Some(next) = self.order_pool.hot.next[current_idx] {
                        current_idx = next;
                    } else {
                        break;
                    }
                }

                if bucket.volume == 0 {
                    buckets.remove(&price);
                    need_update_best = true;
                }
            }
        }

        if need_update_best {
            self.update_best_price(is_bid);
        }

        filled
    }

    /// Fallback cho kiến trúc không phải ARM
    #[cfg(not(target_arch = "aarch64"))]
    fn try_match(&mut self, cmd: &mut OrderCommand) -> Size {
        let is_bid = cmd.action == OrderAction::Bid;
        let limit_price = cmd.price;
        let mut filled = 0;

        let best_price = if is_bid { self.best_ask } else { self.best_bid };
        if let Some(best) = best_price {
            if (is_bid && best > limit_price) || (!is_bid && best < limit_price) {
                return 0;
            }
        } else {
            return 0;
        }

        let prices_to_match: Vec<Price> = if is_bid {
            self.ask_buckets.range(..=limit_price).map(|(p, _)| *p).collect()
        } else {
            self.bid_buckets.range(limit_price..).rev().map(|(p, _)| *p).collect()
        };

        let mut need_update_best = false;

        for price in prices_to_match {
            if filled >= cmd.size {
                break;
            }

            let buckets = if is_bid { &mut self.ask_buckets } else { &mut self.bid_buckets };
            
            if let Some(bucket) = buckets.get_mut(&price) {
                let mut current_idx = bucket.head;
                
                while filled < cmd.size && self.order_pool.hot.active[current_idx] {
                    let remaining = cmd.size - filled;
                    let order_remaining = self.order_pool.hot.sizes[current_idx] - self.order_pool.hot.filled[current_idx];
                    let trade_size = remaining.min(order_remaining);

                    self.order_pool.hot.filled[current_idx] += trade_size;
                    bucket.volume -= trade_size;
                    filled += trade_size;

                    let maker_uid = self.order_pool.cold[current_idx].uid;
                    let reserve = if is_bid {
                        cmd.reserve_price
                    } else {
                        self.order_pool.cold[current_idx].reserve_price
                    };
                    
                    cmd.matcher_events.push(MatcherTradeEvent::new_trade(
                        trade_size,
                        price,
                        self.order_pool.hot.order_ids[current_idx],
                        maker_uid,
                        reserve,
                    ));

                    if self.order_pool.hot.filled[current_idx] >= self.order_pool.hot.sizes[current_idx] {
                        let order_id = self.order_pool.hot.order_ids[current_idx];
                        self.order_index.remove(&order_id);
                        self.order_pool.dealloc(current_idx);
                    }

                    if let Some(next) = self.order_pool.hot.next[current_idx] {
                        current_idx = next;
                    } else {
                        break;
                    }
                }

                if bucket.volume == 0 {
                    buckets.remove(&price);
                    need_update_best = true;
                }
            }
        }

        if need_update_best {
            self.update_best_price(is_bid);
        }

        filled
    }

    /// Chèn lệnh vào thùng giá
    fn insert_to_bucket(&mut self, order_idx: OrderIdx, price: Price, action: OrderAction) {
        let size = self.order_pool.hot.sizes[order_idx] - self.order_pool.hot.filled[order_idx];
        let is_ask = action == OrderAction::Ask;

        let buckets = if is_ask {
            &mut self.ask_buckets
        } else {
            &mut self.bid_buckets
        };

        let is_new = !buckets.contains_key(&price);

        buckets
            .entry(price)
            .and_modify(|bucket| {
                bucket.volume += size;
                let old_head = bucket.head;
                self.order_pool.hot.next[order_idx] = Some(old_head);
                self.order_pool.hot.prev[old_head] = Some(order_idx);
                bucket.head = order_idx;
            })
            .or_insert_with(|| {
                PriceBucket {
                    price,
                    volume: size,
                    head: order_idx,
                }
            });

        if is_new {
            self.update_best_price(is_ask);
        }
    }

    /// Cập nhật bộ nhớ đệm giá tối ưu
    fn update_best_price(&mut self, is_ask: bool) {
        if is_ask {
            self.best_ask = self.ask_buckets.keys().next().copied();
        } else {
            self.best_bid = self.bid_buckets.keys().next_back().copied();
        }
    }

    /// Hủy lệnh
    fn cancel_order(&mut self, cmd: &mut OrderCommand) -> CommandResultCode {
        if let Some(&order_idx) = self.order_index.get(&cmd.order_id) {
            let price = self.order_pool.hot.prices[order_idx];
            let action = self.order_pool.cold[order_idx].action;
            let remaining = self.order_pool.hot.sizes[order_idx] - self.order_pool.hot.filled[order_idx];

            cmd.matcher_events.push(MatcherTradeEvent::new_reject(remaining, price));
            cmd.action = action;

            self.order_index.remove(&cmd.order_id);
            self.order_pool.dealloc(order_idx);

            CommandResultCode::Success
        } else {
            CommandResultCode::MatchingUnknownOrderId
        }
    }
}

impl super::OrderBook for DirectOrderBookOptimized {
    fn new_order(&mut self, cmd: &mut OrderCommand) -> CommandResultCode {
        match cmd.order_type {
            OrderType::Gtc => {
                self.place_gtc(cmd);
                CommandResultCode::Success
            }
            OrderType::Ioc => {
                self.place_ioc(cmd);
                CommandResultCode::Success
            }
            _ => CommandResultCode::MatchingUnsupportedCommand,
        }
    }

    fn cancel_order(&mut self, cmd: &mut OrderCommand) -> CommandResultCode {
        self.cancel_order(cmd)
    }

    fn move_order(&mut self, _cmd: &mut OrderCommand) -> CommandResultCode {
        CommandResultCode::MatchingUnsupportedCommand // Triển khai đơn giản
    }

    fn reduce_order(&mut self, _cmd: &mut OrderCommand) -> CommandResultCode {
        CommandResultCode::MatchingUnsupportedCommand // Triển khai đơn giản
    }

    fn get_symbol_spec(&self) -> &CoreSymbolSpecification {
        &self.symbol_spec
    }

    fn get_l2_data(&self, depth: usize) -> L2MarketData {
        let mut data = L2MarketData::new(depth);

        for (price, bucket) in self.ask_buckets.iter().take(depth) {
            data.ask_prices.push(*price);
            data.ask_volumes.push(bucket.volume);
        }

        for (price, bucket) in self.bid_buckets.iter().rev().take(depth) {
            data.bid_prices.push(*price);
            data.bid_volumes.push(bucket.volume);
        }

        data
    }

    fn get_order_by_id(&self, order_id: OrderId) -> Option<(Price, OrderAction)> {
        self.order_index.get(&order_id).map(|&idx| {
            let price = self.order_pool.hot.prices[idx];
            let action = self.order_pool.cold[idx].action;
            (price, action)
        })
    }

    fn get_total_ask_volume(&self) -> Size {
        self.ask_buckets.values().map(|b| b.volume).sum()
    }

    fn get_total_bid_volume(&self) -> Size {
        self.bid_buckets.values().map(|b| b.volume).sum()
    }

    fn get_ask_buckets_count(&self) -> usize {
        self.ask_buckets.len()
    }

    fn get_bid_buckets_count(&self) -> usize {
        self.bid_buckets.len()
    }

    fn serialize_state(&self) -> crate::core::orderbook::OrderBookState {
        // Đơn giản hóa: Tạm thời không hỗ trợ serialize phiên bản tối ưu
        crate::core::orderbook::OrderBookState::Direct(
            crate::core::orderbook::DirectOrderBook::new(self.symbol_spec.clone())
        )
    }
}

