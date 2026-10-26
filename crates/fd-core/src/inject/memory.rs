use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;

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
    /// Code pages. RTTI data never lives there, so RTTI scans skip them.
    pub executable: bool,
}

/// Raw access to another process's address space.
///
/// `Sync` because scans read different chunks from several threads at once.
pub trait ProcessMemory: Sync {
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
/// multiple GiB; chunking bounds memory use and gives the threads work units.
pub const SCAN_CHUNK: usize = 16 * 1024 * 1024;

/// One unit of scanning work: a slice of a region.
///
/// `read_len` includes an overlap so patterns straddling the boundary are
/// seen; only matches starting in the first `own_len` bytes belong to this
/// chunk (the next chunk owns the rest), so nothing is reported twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkSpec {
    pub region_base: u64,
    pub addr: u64,
    pub read_len: usize,
    pub own_len: usize,
}

/// Splits `regions` (in the given order) into chunks, `overlap` bytes of which
/// are re-read at each boundary.
pub fn chunk_plan(regions: &[MemoryRegion], overlap: usize) -> Vec<ChunkSpec> {
    let mut out = Vec::new();
    for r in regions {
        let size = r.size as usize;
        let mut offset = 0usize;
        while offset < size {
            let own_len = SCAN_CHUNK.min(size - offset);
            out.push(ChunkSpec {
                region_base: r.base,
                addr: r.base + offset as u64,
                read_len: (own_len + overlap).min(size - offset),
                own_len,
            });
            offset += SCAN_CHUNK;
        }
    }
    out
}

/// Reads the chunk (may be shorter than `read_len`; `None` if unreadable).
pub fn read_chunk(mem: &dyn ProcessMemory, spec: &ChunkSpec) -> Option<Vec<u8>> {
    mem.read(spec.addr, spec.read_len).filter(|d| !d.is_empty())
}

/// Matches of `needle` that this chunk owns, as `(absolute address, offset in data)`.
pub fn find_owned(spec: &ChunkSpec, data: &[u8], needle: &[u8], align: u64) -> Vec<(u64, usize)> {
    memchr::memmem::find_iter(data, needle)
        .take_while(|&pos| pos < spec.own_len)
        .map(|pos| (spec.addr + pos as u64, pos))
        .filter(|&(addr, _)| align <= 1 || addr % align == 0)
        .collect()
}

/// Runs `work` over `chunks` several at a time (one rayon batch per call to
/// `on_batch`), handing results back **in chunk order** so callers can validate
/// sequentially and stop early. `on_batch(results, chunks_done)` may `Break`.
///
/// Returns `Err(())`-like cancellation as `Break` with `cancelled == true` via
/// the second tuple field.
pub fn scan_batches<T: Send>(
    chunks: &[ChunkSpec],
    cancel: &AtomicBool,
    work: impl Fn(&ChunkSpec) -> Vec<T> + Sync,
    mut on_batch: impl FnMut(Vec<T>, usize) -> ControlFlow<()>,
) -> ScanEnd {
    let batch = rayon::current_num_threads().clamp(1, 8);
    let mut done = 0usize;
    for group in chunks.chunks(batch) {
        if cancel.load(Ordering::Relaxed) {
            return ScanEnd::Cancelled;
        }
        let results: Vec<Vec<T>> = group.par_iter().map(&work).collect();
        done += group.len();
        if cancel.load(Ordering::Relaxed) {
            return ScanEnd::Cancelled;
        }
        if on_batch(results.into_iter().flatten().collect(), done).is_break() {
            return ScanEnd::Stopped;
        }
    }
    ScanEnd::Completed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanEnd {
    Completed,
    /// The callback asked to stop (a match was accepted).
    Stopped,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inject::mock::MockProcess;

    fn region(base: u64, size: u64) -> MemoryRegion {
        MemoryRegion {
            base,
            size,
            kind: RegionKind::Private,
            readable: true,
            writable: true,
            executable: false,
        }
    }

    #[test]
    fn chunk_plan_covers_every_byte_once() {
        let plan = chunk_plan(&[region(0x1000, (SCAN_CHUNK * 2 + 123) as u64)], 7);
        assert_eq!(plan.len(), 3);
        assert_eq!(
            plan.iter().map(|c| c.own_len).sum::<usize>(),
            SCAN_CHUNK * 2 + 123
        );
        assert_eq!(plan[0].read_len, SCAN_CHUNK + 7);
        assert_eq!(
            plan[2].read_len, 123,
            "last chunk has nothing to overlap into"
        );
        assert_eq!(plan[1].addr, 0x1000 + SCAN_CHUNK as u64);
    }

    #[test]
    fn a_match_straddling_a_chunk_boundary_is_found_exactly_once() {
        let size = SCAN_CHUNK + 64;
        let p = MockProcess::new();
        p.add(0x10_0000, size, RegionKind::Private, true);
        let boundary = 0x10_0000 + SCAN_CHUNK as u64;
        p.poke(boundary - 2, &[1, 2, 3, 4]); // spans the boundary
        let needle = [1u8, 2, 3, 4];
        let plan = chunk_plan(&p.regions(), needle.len() - 1);
        let hits: Vec<u64> = plan
            .iter()
            .flat_map(|c| {
                read_chunk(&p, c)
                    .map(|d| find_owned(c, &d, &needle, 1))
                    .unwrap_or_default()
            })
            .map(|(a, _)| a)
            .collect();
        assert_eq!(hits, vec![boundary - 2]);
    }

    #[test]
    fn alignment_filters_hits() {
        let spec = ChunkSpec {
            region_base: 0,
            addr: 0x1000,
            read_len: 16,
            own_len: 16,
        };
        let mut data = vec![0u8; 16];
        data[1] = 9; // address 0x1001: unaligned
        data[8] = 9; // address 0x1008: aligned
        assert_eq!(find_owned(&spec, &data, &[9], 8), vec![(0x1008, 8)]);
    }

    #[test]
    fn batches_arrive_in_chunk_order_and_can_stop_early() {
        let chunks: Vec<ChunkSpec> = (0..20)
            .map(|i| ChunkSpec {
                region_base: 0,
                addr: i * 100,
                read_len: 1,
                own_len: 1,
            })
            .collect();
        let mut seen = Vec::new();
        let end = scan_batches(
            &chunks,
            &AtomicBool::new(false),
            |c| vec![c.addr],
            |items, _| {
                seen.extend(items);
                if seen.len() >= 8 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        );
        assert_eq!(end, ScanEnd::Stopped);
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
        assert!(seen.len() < 20);
    }

    #[test]
    fn cancellation_stops_scanning_before_any_work() {
        let chunks = [ChunkSpec {
            region_base: 0,
            addr: 0,
            read_len: 1,
            own_len: 1,
        }];
        let end = scan_batches(
            &chunks,
            &AtomicBool::new(true),
            |_| vec![1],
            |_, _| ControlFlow::Continue(()),
        );
        assert_eq!(end, ScanEnd::Cancelled);
    }
}
