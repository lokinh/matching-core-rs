use crate::api::*;
use crate::core::exchange::{ExchangeConfig, ResultConsumer};
use crate::core::processors::{matching_engine::{MatchingEngineRouter, MatchingEngineState}, risk_engine::RiskEngine};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PipelineState {
    pub risk_engines: Vec<RiskEngine>,
    pub matching_engines: Vec<MatchingEngineState>,
}

/// Pipeline - Tổ chức các bộ xử lý
pub struct Pipeline {
    risk_engines: Vec<RiskEngine>,
    matching_engines: Vec<MatchingEngineRouter>,
    result_consumer: Option<ResultConsumer>,
}

impl Pipeline {
    /// Xử lý một lệnh đơn (pipeline đầy đủ)
    pub fn handle_event(&mut self, cmd: &mut OrderCommand, _sequence: i64, _end_of_batch: bool) {
        // 1. Risk R1 (Xử lý trước)
        for engine in &mut self.risk_engines {
            engine.pre_process(cmd);
        }

        // 2. Matching Engine
        for engine in &mut self.matching_engines {
            engine.process_order(cmd);
        }

        // 3. Risk R2 (Xử lý sau)
        for engine in &mut self.risk_engines {
            engine.post_process(cmd);
        }

        // 4. Result Consumer
        if let Some(consumer) = &self.result_consumer {
            consumer(cmd);
        }
    }
    pub fn serialize_state(&self) -> PipelineState {
        PipelineState {
            risk_engines: self.risk_engines.clone(),
            matching_engines: self.matching_engines.iter().map(|e| e.serialize_state()).collect(),
        }
    }

    pub fn from_state(state: PipelineState) -> Self {
        Self {
            risk_engines: state.risk_engines,
            matching_engines: state.matching_engines.into_iter().map(MatchingEngineRouter::from_state).collect(),
            result_consumer: None,
        }
    }
    pub fn new(config: &ExchangeConfig) -> Self {
        // Tạo các phân đoạn engine rủi ro
        let risk_engines = (0..config.risk_engines_num)
            .map(|shard_id| RiskEngine::new(shard_id, config.risk_engines_num))
            .collect();

        // Tạo các phân đoạn engine khớp lệnh
        let matching_engines = (0..config.matching_engines_num)
            .map(|shard_id| MatchingEngineRouter::new(shard_id, config.matching_engines_num))
            .collect();

        Self {
            risk_engines,
            matching_engines,
            result_consumer: None,
        }
    }

    pub fn set_result_consumer(&mut self, consumer: ResultConsumer) {
        self.result_consumer = Some(consumer);
    }

    pub fn add_symbol(&mut self, spec: CoreSymbolSpecification) {
        for engine in &mut self.risk_engines {
            engine.add_symbol(spec.clone());
        }
        for engine in &mut self.matching_engines {
            engine.add_symbol(spec.clone());
        }
    }
}
