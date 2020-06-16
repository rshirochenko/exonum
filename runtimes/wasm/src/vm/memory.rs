use wasmer_runtime::units::{Bytes, Pages};
use wasmer_runtime::wasm::MemoryDescriptor;
use wasmer_runtime::Memory;

use std::error::Error;

use crate::vm::error::VMError;

/// An abstraction over the memory of the smart contract.
pub trait MemoryLike {
    /// Returns whether the memory interval is completely inside the smart contract memory.
    fn fits_memory(&self, offset: u64, len: u64) -> bool;

    /// Reads the content of the given memory interval.
    ///
    /// # Panics
    ///
    /// If memory interval is outside the smart contract memory.
    fn read_memory(&self, offset: u64, buffer: &mut [u8]);

    /// Reads a single byte from the memory.
    ///
    /// # Panics
    ///
    /// If pointer is outside the smart contract memory.
    fn read_memory_u8(&self, offset: u64) -> u8;

    /// Writes the buffer into the smart contract memory.
    ///
    /// # Panics
    ///
    /// If `offset + buffer.len()` is outside the smart contract memory.
    fn write_memory(&mut self, offset: u64, buffer: &[u8]);
}

pub struct WasmerMemory(Memory);

impl WasmerMemory {
    pub fn new(initial_memory_pages: u32, max_memory_pages: u32) -> Result<Self, VMError> {
        Ok(WasmerMemory(
            Memory::new(
                MemoryDescriptor::new(
                    Pages(initial_memory_pages),
                    Some(Pages(max_memory_pages)),
                    false,
                )
                    .unwrap(),
            )
                .expect("TODO creating memory cannot fail"),
        ))
    }

    pub fn clone(&self) -> Memory {
        self.0.clone()
    }
}

impl MemoryLike for WasmerMemory {
    fn fits_memory(&self, offset: u64, len: u64) -> bool {
        match offset.checked_add(len) {
            None => false,
            Some(end) => self.0.size().bytes() >= Bytes(end as usize),
        }
    }

    fn read_memory(&self, offset: u64, buffer: &mut [u8]) {
        let offset = offset as usize;
        for (i, cell) in self.0.view()[offset..(offset + buffer.len())].iter().enumerate() {
            buffer[i] = cell.get();
        }
    }

    fn read_memory_u8(&self, offset: u64) -> u8 {
        self.0.view()[offset as usize].get()
    }

    fn write_memory(&mut self, offset: u64, buffer: &[u8]) {
        let offset = offset as usize;
        self.0.view()[offset..(offset + buffer.len())]
            .iter()
            .zip(buffer.iter())
            .for_each(|(cell, v)| cell.set(*v));
    }
}
