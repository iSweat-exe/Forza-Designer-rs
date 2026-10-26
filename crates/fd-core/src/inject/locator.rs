//! Finding the live vinyl group (`CLiveryGroup`) in the game's memory.
//!
//! 1. **Sphere scan** (fast, primary): look in writable heap for a `u16`
//!    equal to a layer count at `group + count_offset`, then require the layer
//!    table to look like a *fresh, untouched* template (every sampled layer
//!    scores 5/5, and >= 85 % of the whole table does).
//! 2. **RTTI scan** (slow, fallback): find the MSVC RTTI chain for the class
//!    name, derive its vtable(s) and look for heap objects starting with it.
//!    Identity is by C++ type, so already-painted templates are found too.
//!
//! Both are strict on purpose: writing through a wrong table corrupts the game.
//!
//! Performance: memory is read in chunks by several threads at once, and every
//! plausible template size is searched in **one pass** over the heap (rather
//! than one full pass per size). Candidate *validation* stays sequential and in
//! address order, so results and early exit behave like a single-threaded scan.

use std::ops::ControlFlow;
use std::sync::atomic::AtomicBool;

use super::layer::{loose_valid_layer, score_layer};
use super::layout::GameProfile;
use super::memory::{
    chunk_plan, find_owned, is_user_ptr, read_chunk, read_exact, read_u32, read_u64, read_u8,
    scan_batches, ChunkSpec, MemoryRegion, ProcessMemory, RegionKind, ScanEnd,
};
use super::InjectError;

/// Share of a fresh template's layers that must score 5/5.
const FULL_TABLE_THRESHOLD: f64 = 0.85;
/// Share of layers that must pass the loose check for an RTTI-found group.
const RTTI_TABLE_THRESHOLD: f64 = 0.95;
const MAX_CANDIDATES: usize = 200_000;
const SAMPLE_LAYERS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    pub group_addr: u64,
    pub table_addr: u64,
}

/// A located group together with its layer count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub at: Located,
    pub layers: usize,
}

/// What a scan reports while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanEvent {
    /// Chunks scanned so far, total chunks, candidates examined so far.
    Progress {
        scanned: u32,
        total: u32,
        hits: u32,
    },
    Phase(RttiPhase),
}

/// Phases of the RTTI scan, surfaced so the UI can explain the long wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RttiPhase {
    ClassName,
    TypeDescriptorRefs,
    VtableRefs,
    HeapScan,
}

fn read_table_ptr(mem: &dyn ProcessMemory, table_addr: u64, index: usize) -> u64 {
    read_u64(mem, table_addr + index as u64 * 8).unwrap_or(0)
}

fn count_where(
    mem: &dyn ProcessMemory,
    table: u64,
    count: usize,
    ok: impl Fn(u64) -> bool,
) -> usize {
    (0..count)
        .filter(|&k| ok(read_table_ptr(mem, table, k)))
        .count()
}

fn heap_regions(mem: &dyn ProcessMemory, private_only: bool) -> Vec<MemoryRegion> {
    let mut regions: Vec<_> = mem
        .regions()
        .into_iter()
        .filter(|r| {
            r.readable
                && r.writable
                && if private_only {
                    r.kind == RegionKind::Private
                } else {
                    r.kind != RegionKind::Image
                }
        })
        .collect();
    regions.sort_by_key(|r| std::cmp::Reverse(r.size));
    regions
}

/// A possible group found while scanning, before any validation.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    at: Located,
    /// Index into the list of wanted layer counts.
    size_index: usize,
}

/// 64 Kibit membership set over `u16` values.
struct CountSet(Box<[u64; 1024]>);

impl CountSet {
    fn new(counts: &[usize]) -> Self {
        let mut bits = Box::new([0u64; 1024]);
        for &c in counts {
            if let Ok(v) = u16::try_from(c) {
                bits[usize::from(v >> 6)] |= 1 << (v & 63);
            }
        }
        Self(bits)
    }

    fn contains(&self, v: u16) -> bool {
        self.0[usize::from(v >> 6)] >> (v & 63) & 1 == 1
    }
}

/// Sphere-fingerprint scan for a fresh template whose layer count is any of
/// `sizes`. `sizes` is in order of preference: a confirmed match for `sizes[0]`
/// stops the scan immediately, otherwise the earliest-listed size that was
/// confirmed by the end of the scan wins.
pub fn locate_by_sphere_scan(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    sizes: &[usize],
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Option<Found>, InjectError> {
    let layout = &profile.layout;
    let wanted = CountSet::new(sizes);
    let chunks = chunk_plan(&heap_regions(mem, false), 1);
    let total = chunks.len() as u32;

    let worker = |spec: &ChunkSpec| -> Vec<Candidate> {
        let Some(data) = read_chunk(mem, spec) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let last = data.len().saturating_sub(1).min(spec.own_len);
        for pos in 0..last {
            let v = u16::from_le_bytes([data[pos], data[pos + 1]]);
            if !wanted.contains(v) {
                continue;
            }
            let addr = spec.addr + pos as u64;
            let Some(group_addr) = addr
                .checked_sub(layout.count_offset)
                .filter(|g| *g >= spec.region_base)
            else {
                continue;
            };
            // Cheap pre-filter without a syscall: the table pointer is almost
            // always inside the chunk we already hold.
            let table_addr = (pos + layout.table_offset as usize)
                .checked_sub(layout.count_offset as usize)
                .and_then(|p| data.get(p..p + 8))
                .and_then(|b| b.try_into().ok())
                .map(u64::from_le_bytes)
                .or_else(|| read_u64(mem, group_addr + layout.table_offset))
                .unwrap_or(0);
            // The table is an array of pointers: it must be pointer-aligned.
            if !is_user_ptr(table_addr) || table_addr % 8 != 0 {
                continue;
            }
            if let Some(size_index) = sizes.iter().position(|&s| s == usize::from(v)) {
                out.push(Candidate {
                    at: Located {
                        group_addr,
                        table_addr,
                    },
                    size_index,
                });
            }
        }
        out
    };

    let mut confirmed: Vec<Option<Located>> = vec![None; sizes.len()];
    let mut examined = 0usize;
    let end = scan_batches(&chunks, cancel, worker, |candidates, done| {
        let mut flow = ControlFlow::Continue(());
        for c in candidates {
            examined += 1;
            if examined > MAX_CANDIDATES {
                flow = ControlFlow::Break(());
                break;
            }
            if confirmed[c.size_index].is_some() {
                continue;
            }
            if validate_fresh(mem, profile, c.at.table_addr, sizes[c.size_index]) {
                confirmed[c.size_index] = Some(c.at);
                if c.size_index == 0 {
                    flow = ControlFlow::Break(()); // the exact size: nothing can beat it
                    break;
                }
            }
        }
        on_event(ScanEvent::Progress {
            scanned: done as u32,
            total,
            hits: examined as u32,
        });
        flow
    });
    if end == ScanEnd::Cancelled {
        return Err(InjectError::Cancelled);
    }
    Ok(confirmed
        .iter()
        .zip(sizes)
        .find_map(|(at, &layers)| at.map(|at| Found { at, layers })))
}

/// 16/16 sampled layers perfect, then >= 85 % of the whole table perfect.
fn validate_fresh(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    table: u64,
    count: usize,
) -> bool {
    let layout = &profile.layout;
    let strict = |ptr: u64| score_layer(mem, layout, ptr) >= 5;
    (0..count.min(SAMPLE_LAYERS)).all(|k| strict(read_table_ptr(mem, table, k)))
        && count_where(mem, table, count, strict) as f64 >= count as f64 * FULL_TABLE_THRESHOLD
}

/// All occurrences of `needle` in the game image's data (non-code) pages,
/// ascending. Falls back to code pages too if the data pages have none (e.g. a
/// packed executable).
fn find_in_images(
    mem: &dyn ProcessMemory,
    needle: &[u8],
    align: u64,
    stop_after: Option<usize>,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Vec<u64>, InjectError> {
    for include_code in [false, true] {
        let regions: Vec<_> = mem
            .regions()
            .into_iter()
            .filter(|r| {
                r.kind == RegionKind::Image && r.readable && (include_code || !r.executable)
            })
            .collect();
        if regions.is_empty() {
            continue;
        }
        let chunks = chunk_plan(&regions, needle.len().saturating_sub(1));
        let total = chunks.len() as u32;
        let mut hits: Vec<u64> = Vec::new();
        let end = scan_batches(
            &chunks,
            cancel,
            |spec| {
                read_chunk(mem, spec)
                    .map(|d| {
                        find_owned(spec, &d, needle, align)
                            .into_iter()
                            .map(|(a, _)| a)
                            .collect()
                    })
                    .unwrap_or_default()
            },
            |found: Vec<u64>, done| {
                hits.extend(found);
                on_event(ScanEvent::Progress {
                    scanned: done as u32,
                    total,
                    hits: hits.len() as u32,
                });
                if stop_after.is_some_and(|n| hits.len() >= n) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        );
        if end == ScanEnd::Cancelled {
            return Err(InjectError::Cancelled);
        }
        if !hits.is_empty() {
            hits.sort_unstable();
            return Ok(hits);
        }
    }
    Ok(Vec::new())
}

/// MSVC RTTI walk: class-name string -> TypeDescriptor -> CompleteObjectLocator
/// (`u32` RVA reference preceded by signature 1) -> vtable (`COL ptr` is stored at
/// `vtable[-1]`).
fn find_vtables(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    module_base: u64,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Vec<u64>, InjectError> {
    on_event(ScanEvent::Phase(RttiPhase::ClassName));
    let names = find_in_images(mem, profile.rtti_class_name, 1, Some(1), cancel, on_event)?;
    let Some(&name_addr) = names.first() else {
        return Ok(Vec::new());
    };
    // The type name string starts 0x10 bytes into the TypeDescriptor.
    let descriptor = name_addr.saturating_sub(0x10);
    let Some(rva) = descriptor
        .checked_sub(module_base)
        .and_then(|r| u32::try_from(r).ok())
    else {
        return Ok(Vec::new());
    };

    on_event(ScanEvent::Phase(RttiPhase::TypeDescriptorRefs));
    let mut cols: Vec<u64> = find_in_images(mem, &rva.to_le_bytes(), 4, None, cancel, on_event)?
        .into_iter()
        .filter_map(|hit| hit.checked_sub(0x0C))
        .filter(|&col| read_u8(mem, col) == Some(1))
        .collect();
    cols.sort_unstable();
    cols.dedup();
    if cols.is_empty() {
        return Ok(Vec::new());
    }

    on_event(ScanEvent::Phase(RttiPhase::VtableRefs));
    let mut vtables = Vec::new();
    for col in cols {
        for hit in find_in_images(mem, &col.to_le_bytes(), 8, None, cancel, on_event)? {
            vtables.push(hit + 8);
        }
    }
    vtables.sort_unstable();
    vtables.dedup();
    Ok(vtables)
}

/// RTTI-based scan for a group with any of `sizes` layers (same preference
/// rules as [`locate_by_sphere_scan`]). Accepts already-painted templates (loose
/// layer check), requiring 95 % of layers to be sane.
pub fn locate_by_rtti(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    sizes: &[usize],
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Option<Found>, InjectError> {
    let layout = &profile.layout;
    let Some(module_base) = mem.main_module_base() else {
        return Ok(None);
    };
    // The vtable chain is the same for every size: resolve it once.
    let vtables = find_vtables(mem, profile, module_base, cancel, on_event)?;
    if vtables.is_empty() {
        return Ok(None);
    }

    on_event(ScanEvent::Phase(RttiPhase::HeapScan));
    let chunks = chunk_plan(&heap_regions(mem, true), 7);
    let total = chunks.len() as u32;

    let worker = |spec: &ChunkSpec| -> Vec<Candidate> {
        let Some(data) = read_chunk(mem, spec) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for vtable in &vtables {
            for (group_addr, pos) in find_owned(spec, &data, &vtable.to_le_bytes(), 8) {
                let count_at = pos + layout.count_offset as usize;
                let raw = data
                    .get(count_at..count_at + 4)
                    .and_then(|b| b.try_into().ok())
                    .map(u32::from_le_bytes)
                    .or_else(|| read_u32(mem, group_addr + layout.count_offset));
                let Some(raw) = raw else { continue };
                let Some(size_index) = sizes.iter().position(|&s| s == (raw & 0xFFFF) as usize)
                else {
                    continue;
                };
                let table_addr = read_u64(mem, group_addr + layout.table_offset).unwrap_or(0);
                if is_user_ptr(table_addr) {
                    out.push(Candidate {
                        at: Located {
                            group_addr,
                            table_addr,
                        },
                        size_index,
                    });
                }
            }
        }
        out
    };

    let mut confirmed: Vec<Option<Located>> = vec![None; sizes.len()];
    let mut examined = 0usize;
    let end = scan_batches(&chunks, cancel, worker, |candidates, done| {
        let mut flow = ControlFlow::Continue(());
        for c in candidates {
            examined += 1;
            if confirmed[c.size_index].is_some() {
                continue;
            }
            if validate_loose(mem, profile, c.at.table_addr, sizes[c.size_index]) {
                confirmed[c.size_index] = Some(c.at);
                if c.size_index == 0 {
                    flow = ControlFlow::Break(());
                    break;
                }
            }
        }
        on_event(ScanEvent::Progress {
            scanned: done as u32,
            total,
            hits: examined as u32,
        });
        flow
    });
    if end == ScanEnd::Cancelled {
        return Err(InjectError::Cancelled);
    }
    Ok(confirmed
        .iter()
        .zip(sizes)
        .find_map(|(at, &layers)| at.map(|at| Found { at, layers })))
}

fn validate_loose(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    table: u64,
    count: usize,
) -> bool {
    let layout = &profile.layout;
    let sane = |ptr: u64| loose_valid_layer(mem, layout, ptr);
    (0..count.min(SAMPLE_LAYERS)).all(|k| sane(read_table_ptr(mem, table, k)))
        && count_where(mem, table, count, sane) as f64 >= count as f64 * RTTI_TABLE_THRESHOLD
}

/// Reads the whole layer-pointer table in one go (falls back to per-pointer reads).
pub fn read_layer_table(mem: &dyn ProcessMemory, table_addr: u64, count: usize) -> Vec<u64> {
    match read_exact(mem, table_addr, count * 8) {
        Some(bytes) => bytes
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
            .collect(),
        None => (0..count)
            .map(|k| read_table_ptr(mem, table_addr, k))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_set_membership() {
        let set = CountSet::new(&[10, 500, 3000, 70_000]);
        assert!(set.contains(10) && set.contains(500) && set.contains(3000));
        assert!(!set.contains(11) && !set.contains(0));
        // 70_000 does not fit a u16 and must simply be ignored.
        assert!(!set.contains((70_000u32 & 0xFFFF) as u16));
    }
}
