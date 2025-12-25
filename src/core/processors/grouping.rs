use crate::api::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// Bộ xử lý nhóm - Chịu trách nhiệm xử lý và nhóm lệnh theo lô
pub struct GroupingProcessor {
    group_counter: AtomicU64,
    msgs_in_group_limit: usize,
}

impl GroupingProcessor {
    pub fn new(msgs_in_group_limit: usize) -> Self {
        Self {
            group_counter: AtomicU64::new(0),
            msgs_in_group_limit,
        }
    }

    /// Xử lý lệnh, phân bổ events_group
    pub fn process(&self, cmd: &mut OrderCommand, msgs_in_current_group: &mut usize) {
        // Một số lệnh cần buộc kích hoạt nhóm mới
        if matches!(
            cmd.command,
            OrderCommandType::Reset
                | OrderCommandType::PersistStateMatching
                | OrderCommandType::GroupingControl
        ) {
            self.group_counter.fetch_add(1, Ordering::SeqCst);
            *msgs_in_current_group = 0;
        }

        cmd.events_group = self.group_counter.load(Ordering::SeqCst);

        *msgs_in_current_group += 1;

        // Đạt đến giới hạn kích thước lô, chuyển sang nhóm mới
        if *msgs_in_current_group >= self.msgs_in_group_limit {
            self.group_counter.fetch_add(1, Ordering::SeqCst);
            *msgs_in_current_group = 0;
        }
    }
}
