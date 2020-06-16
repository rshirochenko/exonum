pub use crate::vm::error::VMLogicError;
pub use crate::vm::memory::MemoryLike;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub enum ReturnData {
    /// Method returned some value or data.
    Value(Vec<u8>),

    /// The return value of the method should be taken from the return value of another method
    /// identified through receipt index.
    ReceiptIndex(u64),

    /// Method hasn't returned any data or promise.
    None,
}

pub struct VMLogic<'a> {
    /// Pointer to the guest memory.
    memory: &'a mut dyn MemoryLike,
    /// What method returns.
    return_data: ReturnData,
    /// Registers
    registers: HashMap<u64, Vec<u8>>,
}

pub type Result<T> = ::std::result::Result<T, VMLogicError>;

impl<'a> VMLogic<'a> {
    pub fn new(
        memory: &'a mut dyn MemoryLike,
    ) -> Self {
        Self {
            memory,
            return_data: ReturnData::None,
            registers: HashMap::new(),
        }
    }

    fn try_fit_mem(&mut self, offset: u64, len: u64) -> Result<()> {
        if self.memory.fits_memory(offset, len) {
            Ok(())
        } else {
            Err(VMLogicError::HostError.into())
        }
    }

    fn memory_get_vec(&mut self, offset: u64, len: u64) -> Result<Vec<u8>> {
        self.try_fit_mem(offset, len)?;
        let mut buf = vec![0; len as usize];
        self.memory.read_memory(offset, &mut buf);
        Ok(buf)
    }

    fn memory_set_slice(&mut self, offset: u64, buf: &[u8]) -> Result<()> {
        self.try_fit_mem(offset, buf.len() as _)?;
        self.memory.write_memory(offset, buf);
        Ok(())
    }

    fn internal_read_register(&mut self, register_id: u64) -> Result<Vec<u8>> {
        if let Some(data) = self.registers.get(&register_id) {
            Ok(data.clone())
        } else {
            Err(VMLogicError::HostError.into())
        }
    }

    pub fn read_register(&mut self, register_id: u64, ptr: u64) -> Result<()> {
        let data = self.internal_read_register(register_id)?;
        self.memory_set_slice(ptr, &data)
    }

    pub fn register_len(&mut self, register_id: u64) -> Result<u64> {
        Ok(self.registers.get(&register_id).map(|r| r.len() as _).unwrap_or(std::u64::MAX))
    }

    pub fn outcome(self) -> ReturnData {
        self.return_data
    }
}
