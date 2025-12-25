use crate::api::*;
use crate::core::pipeline::Pipeline;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// Cấu hình core sàn giao dịch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeConfig {
    pub ring_buffer_size: usize,
    pub matching_engines_num: usize,
    pub risk_engines_num: usize,
    pub producer_type: ProducerType,
    pub wait_strategy: WaitStrategyType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ProducerType {
    Single,
    Multi,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WaitStrategyType {
    BusySpin,
    Yielding,
    Blocking,
    Sleeping,
}

impl ExchangeConfig {
    // Ở đây không cần phương thức chuyển đổi đồng bộ nữa, vì startup đã xử lý ánh xạ cấu hình sang kiểu cụ thể
}

#[derive(Serialize, Deserialize)]
pub struct ExchangeState {
    pub config: ExchangeConfig,
    pub pipeline_state: crate::core::pipeline::PipelineState,
}

impl Default for ExchangeConfig {
    fn default() -> Self {
        Self {
            ring_buffer_size: 64 * 1024,
            matching_engines_num: 1,
            risk_engines_num: 1,
            producer_type: ProducerType::Single,
            wait_strategy: WaitStrategyType::BusySpin,
        }
    }
}

/// Callback người tiêu dùng kết quả
pub type ResultConsumer = Arc<dyn Fn(&OrderCommand) + Send + Sync>;

use crate::core::journal::Journaler;
use std::path::Path;

use crate::core::snapshot::SnapshotStore;

/// Giao diện nội bộ, dùng để xóa kiểu Producer generic của Disruptor
trait Publisher {
    fn publish(&mut self, cmd: OrderCommand);
}

struct ProducerWrapper<P: disruptor::Producer<OrderCommand>>(P);

impl<P: disruptor::Producer<OrderCommand>> Publisher for ProducerWrapper<P> {
    fn publish(&mut self, cmd: OrderCommand) {
        self.0.publish(|event| {
            *event = cmd;
        });
    }
}

/// Core sàn giao dịch
pub struct ExchangeCore {
    config: ExchangeConfig,
    // Sử dụng đối tượng trait Publisher để ẩn kiểu producer disruptor cụ thể
    producer: Option<Box<dyn Publisher>>,
    pipeline: Option<Pipeline>,
    journaler: Option<Journaler>,
    snapshot_store: Option<SnapshotStore>,
}

impl ExchangeCore {
    pub fn new(config: ExchangeConfig) -> Self {
        let pipeline = Pipeline::new(&config);
        Self { 
            config, 
            pipeline: Some(pipeline),
            producer: None,
            journaler: None,
            snapshot_store: None,
        }
    }

    /// Khởi động pipeline Disruptor
    pub fn startup(&mut self) {
        if self.producer.is_some() {
            return;
        }

        if let Some(mut pipeline) = self.pipeline.take() {
            let ring_size = self.config.ring_buffer_size;
            
            // Đóng gói logic xử lý sự kiện
            // Handler của Disruptor 3.6.1 nhận &E (bất biến)
            // Để duy trì logic có thể thay đổi của Pipeline ban đầu, chúng ta clone trước khi xử lý
            let handler = move |event: &OrderCommand, sequence: i64, end_of_batch: bool| {
                let mut cmd_mut = event.clone();
                pipeline.handle_event(&mut cmd_mut, sequence, end_of_batch);
            };

            // Sử dụng build_single_producer / build_multi_producer
            // Hiện tại 3.6.1 chỉ hỗ trợ rõ ràng một số chiến lược như BusySpin trong wait_strategies
            let producer: Box<dyn Publisher> = match self.config.producer_type {
                ProducerType::Single => {
                    Box::new(ProducerWrapper(disruptor::build_single_producer(ring_size, || OrderCommand::default(), disruptor::wait_strategies::BusySpin)
                        .handle_events_with(handler)
                        .build()))
                },
                ProducerType::Multi => {
                    Box::new(ProducerWrapper(disruptor::build_multi_producer(ring_size, || OrderCommand::default(), disruptor::wait_strategies::BusySpin)
                        .handle_events_with(handler)
                        .build()))
                }
            };

            self.producer = Some(producer);
        }
    }

    /// Bật quản lý snapshot
    pub fn enable_snapshotting<P: AsRef<Path>>(&mut self, path: P) -> anyhow::Result<()> {
        self.snapshot_store = Some(SnapshotStore::new(path)?);
        Ok(())
    }

    /// Tạo snapshot trạng thái hiện tại
    pub fn take_snapshot(&self, seq_id: u64) -> anyhow::Result<()> {
        if let Some(store) = &self.snapshot_store {
            let state = self.serialize_state();
            store.save_snapshot(&state, seq_id)?;
        }
        Ok(())
    }

    /// Tải snapshot mới nhất và khôi phục trạng thái
    pub fn load_latest_snapshot(&mut self) -> anyhow::Result<bool> {
        if let Some(store) = &self.snapshot_store {
            if let Some(seq_id) = store.get_latest_seq_id()? {
                let state = store.load_snapshot(seq_id)?;
                *self = Self::from_state(state);
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Bật ghi nhật ký bền vững
    pub fn enable_journaling<P: AsRef<Path>>(&mut self, path: P) -> anyhow::Result<()> {
        self.journaler = Some(Journaler::new(path)?);
        Ok(())
    }

    /// Callback người tiêu dùng kết quả
    pub fn set_result_consumer(&mut self, consumer: ResultConsumer) {
        if let Some(p) = &mut self.pipeline {
            p.set_result_consumer(consumer);
        }
    }

    pub fn add_symbol(&mut self, spec: CoreSymbolSpecification) {
        if let Some(p) = &mut self.pipeline {
            p.add_symbol(spec);
        }
    }

    /// Gửi lệnh
    pub fn submit_command(&mut self, mut cmd: OrderCommand) -> OrderCommand {
        if let Some(j) = &mut self.journaler {
            let _ = j.write_command(&cmd);
        }
        
        if let Some(producer) = &mut self.producer {
            producer.publish(cmd.clone());
            cmd
        } else if let Some(pipeline) = &mut self.pipeline {
            pipeline.handle_event(&mut cmd, 0, true);
            cmd
        } else {
            panic!("ExchangeCore chưa sẵn sàng");
        }
    }

    /// Phát lại từ nhật ký
    pub fn replay_journal<P: AsRef<Path>>(&mut self, path: P) -> anyhow::Result<()> {
        let commands = Journaler::read_commands(path)?;
        for mut cmd in commands {
            if let Some(pipeline) = &mut self.pipeline {
                pipeline.handle_event(&mut cmd, 0, true);
            } else {
                self.submit_command(cmd);
            }
        }
        Ok(())
    }

    pub fn serialize_state(&self) -> ExchangeState {
        ExchangeState {
            config: self.config.clone(),
            pipeline_state: self.pipeline.as_ref().expect("Chỉ có thể serialize trước khi khởi động").serialize_state(),
        }
    }

    pub fn from_state(state: ExchangeState) -> Self {
        Self {
            config: state.config,
            pipeline: Some(Pipeline::from_state(state.pipeline_state)),
            producer: None,
            journaler: None,
            snapshot_store: None,
        }
    }
}

