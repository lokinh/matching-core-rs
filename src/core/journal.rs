use crate::api::OrderCommand;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, BufWriter, BufReader};
use std::path::Path;
use anyhow::Result;
use rkyv::Deserialize;

/// Triển khai nhật ký ghi trước hiệu năng cao (WAL) - Sử dụng serialize không sao chép rkyv
pub struct Journaler {
    writer: BufWriter<File>,
}

impl Journaler {
    /// Tạo hoặc mở file nhật ký
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        
        Ok(Self {
            writer: BufWriter::with_capacity(64 * 1024, file), // Bộ đệm 64KB
        })
    }

    /// Ghi lệnh vào nhật ký (sử dụng rkyv, nhanh hơn bincode 2.5 lần)
    pub fn write_command(&mut self, cmd: &OrderCommand) -> Result<()> {
        // Serialize rkyv
        let bytes = rkyv::to_bytes::<_, 256>(cmd)
            .map_err(|e| anyhow::anyhow!("rkyv serialize thất bại: {}", e))?;
        
        // Ghi tiền tố độ dài (u32) + dữ liệu
        let len = bytes.len() as u32;
        self.writer.write_all(&len.to_le_bytes())?;
        self.writer.write_all(&bytes)?;
        
        // Ghi hàng loạt vào đĩa (được BufWriter điều khiển)
        self.writer.flush()?;
        
        Ok(())
    }

    /// Đọc từ file nhật ký và phát lại tất cả lệnh
    pub fn read_commands<P: AsRef<Path>>(path: P) -> Result<Vec<OrderCommand>> {
        if !path.as_ref().exists() {
            return Ok(Vec::new());
        }

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut commands = Vec::new();

        loop {
            let mut len_buf = [0u8; 4];
            if reader.read_exact(&mut len_buf).is_err() {
                break; // Đến cuối file
            }
            
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data)?;
            
            // Deserialize rkyv (có kiểm tra)
            let archived = rkyv::check_archived_root::<OrderCommand>(&data)
                .map_err(|e| anyhow::anyhow!("rkyv kiểm tra dữ liệu thất bại: {}", e))?;
            
            let cmd: OrderCommand = archived.deserialize(&mut rkyv::Infallible)
                .map_err(|_| anyhow::anyhow!("rkyv deserialize thất bại"))?;
            
            commands.push(cmd);
        }

        Ok(commands)
    }
}
