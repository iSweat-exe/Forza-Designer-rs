//! Finding the live vinyl group (`CLiveryGroup`) in the game's memory.
//!
//! 1. **Sphere scan** (fast, primary): look in writable heap for a `u16`
//!    equal to the layer count at `group + count_offset`, then require the layer
//!    table to look like a *fresh, untouched* template (every sampled layer
//!    scores 5/5, and >= 85 % of the whole table does).
//! 2. **RTTI scan** (slow, fallback): find the MSVC RTTI chain for the class
//!    name, derive its vtable(s) and look for heap objects starting with it.
//!    Identity is by C++ type, so already-painted templates are found too.
//!
//! Both are strict on purpose: writing through a wrong table corrupts the game.

use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};

use super::layer::{loose_valid_layer, score_layer};
use super::layout::GameProfile;
use super::memory::{
    is_user_ptr, read_exact, read_u32, read_u64, read_u8, scan_region, MemoryRegion, ProcessMemory,
    RegionKind,
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

/// What a scan reports while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanEvent {
    /// Regions scanned so far, total regions, candidates so far.
    Progress {
        scanned: u32,
        total: u32,
        hits: u32,
    },
    Phase(RttiPhase),
}

fn read_table_ptr(mem: &dyn ProcessMemory, table_addr: u64, index: usize) -> u64 {
    read_u64(mem, table_addr + index as u64 * 8).unwrap_or(0)
}

fn count_strict(mem: &dyn ProcessMemory, profile: &GameProfile, table: u64, count: usize) -> usize {
    (0..count)
        .filter(|&k| score_layer(mem, &profile.layout, read_table_ptr(mem, table, k)) >= 5)
        .count()
}

fn count_loose(mem: &dyn ProcessMemory, profile: &GameProfile, table: u64, count: usize) -> usize {
    (0..count)
        .filter(|&k| loose_valid_layer(mem, &profile.layout, read_table_ptr(mem, table, k)))
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

/// Sphere-fingerprint scan for a group with exactly `count` layers.
pub fn locate_by_sphere_scan(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    count: usize,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Option<Located>, InjectError> {
    let layout = &profile.layout;
    let needle = (count as u16).to_le_bytes();
    let regions = heap_regions(mem, false);
    let total = regions.len() as u32;
    let mut plausible = 0usize;
    let mut perfect: Vec<Located> = Vec::new();
    let mut confirmed: Option<Located> = None;
    let sample = count.min(SAMPLE_LAYERS);

    for (i, region) in regions.iter().enumerate() {
        let flow = scan_region(mem, region, &needle, 1, cancel, |addr, chunk, pos| {
            let Some(group_addr) = addr
                .checked_sub(layout.count_offset)
                .filter(|g| *g >= region.base)
            else {
                return ControlFlow::Continue(());
            };
            // Cheap pre-filter without a syscall: the table pointer is almost
            // always inside the chunk we already hold.
            let tbl_pos =
                (pos + layout.table_offset as usize).checked_sub(layout.count_offset as usize);
            let table_addr = tbl_pos
                .and_then(|p| chunk.get(p..p + 8))
                .and_then(|b| b.try_into().ok())
                .map(u64::from_le_bytes)
                .or_else(|| read_u64(mem, group_addr + layout.table_offset))
                .unwrap_or(0);
            if !is_user_ptr(table_addr) {
                return ControlFlow::Continue(());
            }
            plausible += 1;
            if plausible > MAX_CANDIDATES {
                return ControlFlow::Break(());
            }
            let all_strict = (0..sample)
                .all(|k| score_layer(mem, layout, read_table_ptr(mem, table_addr, k)) >= 5);
            if !all_strict {
                return ControlFlow::Continue(());
            }
            let candidate = Located {
                group_addr,
                table_addr,
            };
            let valid = count_strict(mem, profile, table_addr, count);
            if valid as f64 >= count as f64 * FULL_TABLE_THRESHOLD {
                confirmed = Some(candidate); // first confident match wins; stop scanning
                return ControlFlow::Break(());
            }
            perfect.push(candidate);
            ControlFlow::Continue(())
        });
        on_event(ScanEvent::Progress {
            scanned: i as u32 + 1,
            total,
            hits: perfect.len() as u32 + u32::from(confirmed.is_some()),
        });
        if cancel.load(Ordering::Relaxed) {
            return Err(InjectError::Cancelled);
        }
        if flow.is_break() {
            break;
        }
    }
    Ok(confirmed.or_else(|| best_by_full_table(mem, profile, perfect, count)))
}

/// Among candidates, the one whose whole table validates best (must clear the threshold).
fn best_by_full_table(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    c: Vec<Located>,
    count: usize,
) -> Option<Located> {
    c.into_iter()
        .map(|l| (count_strict(mem, profile, l.table_addr, count), l))
        .max_by_key(|(valid, _)| *valid)
        .filter(|(valid, _)| *valid as f64 >= count as f64 * FULL_TABLE_THRESHOLD)
        .map(|(_, l)| l)
}

/// Phases of the RTTI scan, surfaced so the UI can explain the long wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RttiPhase {
    ClassName,
    TypeDescriptorRefs,
    VtableRefs,
    HeapScan,
}

/// All occurrences of `needle` inside readable image regions.
fn find_in_images(
    mem: &dyn ProcessMemory,
    needle: &[u8],
    align: u64,
    stop_after: Option<usize>,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Vec<u64>, InjectError> {
    let regions: Vec<_> = mem
        .regions()
        .into_iter()
        .filter(|r| r.kind == RegionKind::Image && r.readable)
        .collect();
    let total = regions.len().max(1) as u32;
    let mut hits = Vec::new();
    for (i, region) in regions.iter().enumerate() {
        let flow = scan_region(mem, region, needle, align, cancel, |addr, _, _| {
            hits.push(addr);
            if stop_after.is_some_and(|n| hits.len() >= n) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        on_event(ScanEvent::Progress {
            scanned: i as u32 + 1,
            total,
            hits: hits.len() as u32,
        });
        if cancel.load(Ordering::Relaxed) {
            return Err(InjectError::Cancelled);
        }
        if flow.is_break() {
            break;
        }
    }
    Ok(hits)
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

/// RTTI-based scan for a group with `count` layers. Accepts already-painted
/// templates (loose layer check), requiring 95 % of layers to be sane.
pub fn locate_by_rtti(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    count: usize,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Option<Located>, InjectError> {
    let layout = &profile.layout;
    let Some(module_base) = mem.main_module_base() else {
        return Ok(None);
    };
    let vtables = find_vtables(mem, profile, module_base, cancel, on_event)?;
    if vtables.is_empty() {
        return Ok(None);
    }

    on_event(ScanEvent::Phase(RttiPhase::HeapScan));
    let regions = heap_regions(mem, true);
    let total = regions.len() as u32;
    let mut candidates: Vec<Located> = Vec::new();
    let mut confirmed: Option<Located> = None;
    let sample = count.min(SAMPLE_LAYERS);

    for (i, region) in regions.iter().enumerate() {
        'vtables: for vtable in &vtables {
            let flow = scan_region(
                mem,
                region,
                &vtable.to_le_bytes(),
                8,
                cancel,
                |group_addr, _, _| {
                    let Some(raw) = read_u32(mem, group_addr + layout.count_offset) else {
                        return ControlFlow::Continue(());
                    };
                    if (raw & 0xFFFF) as usize != count {
                        return ControlFlow::Continue(());
                    }
                    let table_addr = read_u64(mem, group_addr + layout.table_offset).unwrap_or(0);
                    if !is_user_ptr(table_addr)
                        || candidates.iter().any(|c| c.group_addr == group_addr)
                    {
                        return ControlFlow::Continue(());
                    }
                    let candidate = Located {
                        group_addr,
                        table_addr,
                    };
                    let sampled_ok = (0..sample).all(|k| {
                        loose_valid_layer(mem, layout, read_table_ptr(mem, table_addr, k))
                    });
                    if sampled_ok
                        && count_loose(mem, profile, table_addr, count) as f64
                            >= count as f64 * RTTI_TABLE_THRESHOLD
                    {
                        confirmed = Some(candidate);
                        return ControlFlow::Break(());
                    }
                    candidates.push(candidate);
                    ControlFlow::Continue(())
                },
            );
            if flow.is_break() {
                break 'vtables;
            }
        }
        on_event(ScanEvent::Progress {
            scanned: i as u32 + 1,
            total,
            hits: candidates.len() as u32 + u32::from(confirmed.is_some()),
        });
        if cancel.load(Ordering::Relaxed) {
            return Err(InjectError::Cancelled);
        }
        if confirmed.is_some() {
            break;
        }
    }
    if confirmed.is_some() {
        return Ok(confirmed);
    }

    Ok(candidates
        .into_iter()
        .map(|c| (count_loose(mem, profile, c.table_addr, count), c))
        .max_by_key(|(valid, _)| *valid)
        .filter(|(valid, _)| *valid as f64 >= count as f64 * RTTI_TABLE_THRESHOLD)
        .map(|(_, c)| c))
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
