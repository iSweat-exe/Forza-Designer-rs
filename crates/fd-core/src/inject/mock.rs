//! In-memory fake game process for tests.

use std::cell::RefCell;

use super::memory::{MemoryRegion, ProcessMemory, RegionKind};
use super::InjectError;

struct Block {
    base: u64,
    data: Vec<u8>,
    kind: RegionKind,
    writable: bool,
}

#[derive(Default)]
pub struct MockProcess {
    blocks: RefCell<Vec<Block>>,
    module_base: Option<u64>,
}

impl MockProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_module_base(mut self, base: u64) -> Self {
        self.module_base = Some(base);
        self
    }

    pub fn add(&self, base: u64, size: usize, kind: RegionKind, writable: bool) {
        self.blocks.borrow_mut().push(Block {
            base,
            data: vec![0; size],
            kind,
            writable,
        });
    }

    pub fn poke(&self, addr: u64, bytes: &[u8]) {
        let mut blocks = self.blocks.borrow_mut();
        let b = blocks
            .iter_mut()
            .find(|b| addr >= b.base && addr + bytes.len() as u64 <= b.base + b.data.len() as u64)
            .expect("poke outside any block");
        let off = (addr - b.base) as usize;
        b.data[off..off + bytes.len()].copy_from_slice(bytes);
    }

    pub fn peek(&self, addr: u64, len: usize) -> Vec<u8> {
        self.read(addr, len).expect("peek outside any block")
    }

    pub fn poke_u64(&self, addr: u64, v: u64) {
        self.poke(addr, &v.to_le_bytes());
    }

    pub fn poke_f32s(&self, addr: u64, vals: &[f32]) {
        let bytes: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
        self.poke(addr, &bytes);
    }

    pub fn peek_f32s(&self, addr: u64, n: usize) -> Vec<f32> {
        self.peek(addr, n * 4)
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

impl ProcessMemory for MockProcess {
    fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
        let blocks = self.blocks.borrow();
        let b = blocks
            .iter()
            .find(|b| addr >= b.base && addr < b.base + b.data.len() as u64)?;
        let off = (addr - b.base) as usize;
        let end = (off + len).min(b.data.len());
        Some(b.data[off..end].to_vec())
    }

    fn write(&self, addr: u64, data: &[u8]) -> Result<(), InjectError> {
        let mut blocks = self.blocks.borrow_mut();
        let b = blocks
            .iter_mut()
            .find(|b| {
                addr >= b.base
                    && addr + data.len() as u64 <= b.base + b.data.len() as u64
                    && b.writable
            })
            .ok_or(InjectError::Memory {
                addr,
                reason: "not writable".into(),
            })?;
        let off = (addr - b.base) as usize;
        b.data[off..off + data.len()].copy_from_slice(data);
        Ok(())
    }

    fn regions(&self) -> Vec<MemoryRegion> {
        self.blocks
            .borrow()
            .iter()
            .map(|b| MemoryRegion {
                base: b.base,
                size: b.data.len() as u64,
                kind: b.kind,
                readable: true,
                writable: b.writable,
            })
            .collect()
    }

    fn main_module_base(&self) -> Option<u64> {
        self.module_base
    }
}
