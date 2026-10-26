use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};

use super::InjectError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Image,
    Private,
    Mapped,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base: u64,
    pub size: u64,
    pub kind: RegionKind,
    pub readable: bool,
    pub writable: bool,
}

/// Raw access to another process's address space.
pub trait ProcessMemory {
    /// Reads up to `len` bytes. May return fewer on a partial read; `None` when
    /// nothing at all is readable (scans rely on this being cheap and non-fatal).
    fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>>;
    fn write(&self, addr: u64, data: &[u8]) -> Result<(), InjectError>;
    /// Committed regions of the address space.
    fn regions(&self) -> Vec<MemoryRegion>;
    /// Base address of the main executable module.
    fn main_module_base(&self) -> Option<u64>;
}

/// Plausible user-mode pointer (rejects nulls, small ints, kernel addresses).
pub fn is_user_ptr(v: u64) -> bool {
    v > 0x0100_0000 && v < 0x8000_0000_0000
}

pub fn read_exact(mem: &dyn ProcessMemory, addr: u64, len: usize) -> Option<Vec<u8>> {
    mem.read(addr, len).filter(|b| b.len() == len)
}

pub fn read_u8(mem: &dyn ProcessMemory, addr: u64) -> Option<u8> {
    read_exact(mem, addr, 1).map(|b| b[0])
}

pub fn read_u32(mem: &dyn ProcessMemory, addr: u64) -> Option<u32> {
    read_exact(mem, addr, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn read_u64(mem: &dyn ProcessMemory, addr: u64) -> Option<u64> {
    read_exact(mem, addr, 8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
}

/// Bytes read per `ReadProcessMemory` call while scanning. Regions can be
/// multiple GiB; chunking bounds our own memory use.
const SCAN_CHUNK: usize = 64 * 1024 * 1024;

/// Finds every occurrence of `needle` (at `align`-byte alignment) in `region`,
/// reading it in chunks. `on_hit(addr, chunk, pos)` gets the absolute address,
/// the chunk that contains the match and the match offset inside it (so callers
/// can look at nearby bytes without another syscall). Unreadable chunks are
/// skipped. Returns `Break` as soon as `on_hit` breaks or `cancel` is set.
pub fn scan_region(
    mem: &dyn ProcessMemory,
    region: &MemoryRegion,
    needle: &[u8],
    align: u64,
    cancel: &AtomicBool,
    mut on_hit: impl FnMut(u64, &[u8], usize) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let overlap = needle.len().saturating_sub(1);
    let size = region.size as usize;
    let mut offset = 0usize;
    while offset < size {
        if cancel.load(Ordering::Relaxed) {
            return ControlFlow::Break(());
        }
        let want = (SCAN_CHUNK + overlap).min(size - offset);
        let last_chunk = offset + SCAN_CHUNK >= size;
        if let Some(data) = mem.read(region.base + offset as u64, want) {
            for pos in memchr::memmem::find_iter(&data, needle) {
                // Matches in the overlap belong to the next chunk (no double counting).
                if pos >= SCAN_CHUNK && !last_chunk {
                    break;
                }
                let addr = region.base + (offset + pos) as u64;
                if align > 1 && addr % align != 0 {
                    continue;
                }
                on_hit(addr, &data, pos)?;
            }
        }
        offset += SCAN_CHUNK;
    }
    ControlFlow::Continue(())
}
