use crate::core::exchange::ExchangeState;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

/// Trình quản lý snapshot (sử dụng bincode, tương thích tốt)
pub struct SnapshotStore {
    base_path: PathBuf,
}

impl SnapshotStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let base_path = path.as_ref().to_path_buf();
        if !base_path.exists() {
            fs::create_dir_all(&base_path).context("Không thể tạo thư mục snapshot")?;
        }
        Ok(Self { base_path })
    }

    /// Lưu trạng thái core vào file snapshot
    pub fn save_snapshot(&self, state: &ExchangeState, seq_id: u64) -> Result<PathBuf> {
        let filename = format!("snapshot_{}.bin", seq_id);
        let path = self.base_path.join(filename);
        
        let file = File::create(&path).context("Không thể tạo file snapshot")?;
        let writer = BufWriter::new(file);
        
        bincode::serialize_into(writer, state).context("Serialize snapshot thất bại")?;
        
        Ok(path)
    }

    /// Tải snapshot với chỉ mục được chỉ định
    pub fn load_snapshot(&self, seq_id: u64) -> Result<ExchangeState> {
        let filename = format!("snapshot_{}.bin", seq_id);
        let path = self.base_path.join(filename);
        
        let file = File::open(&path).context("Không thể mở file snapshot")?;
        let reader = BufReader::new(file);
        
        let state: ExchangeState = bincode::deserialize_from(reader).context("Deserialize snapshot thất bại")?;
        
        Ok(state)
    }

    /// Lấy chỉ mục snapshot mới nhất
    pub fn get_latest_seq_id(&self) -> Result<Option<u64>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.base_path)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("snapshot_") && name.ends_with(".bin") {
                if let Ok(id) = name["snapshot_".len()..name.len() - 4].parse::<u64>() {
                    ids.push(id);
                }
            }
        }
        
        ids.sort_unstable();
        Ok(ids.last().copied())
    }
}
