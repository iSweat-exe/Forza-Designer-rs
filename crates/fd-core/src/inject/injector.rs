//! Orchestration: locate the group, then write every shape into its layer.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use super::layer::{layer_values, loose_valid_layer, score_layer};
use super::layout::{GameProfile, Layout};
use super::locator::{self, Located, ScanEvent};
use super::memory::{is_user_ptr, ProcessMemory};
use super::InjectError;
use crate::shapegen::Shape;

/// Template sizes people commonly save, tried after the exact shape count
/// (a 1500-layer template can host a 500-shape document).
const COMMON_TEMPLATE_SIZES: [usize; 8] = [500, 1500, 3000, 1000, 100, 50, 20, 10];

#[derive(Debug, Clone, Default)]
pub struct InjectOptions {
    /// Offset/constant overrides (contents of `.fd6_offsets.json`), if any.
    pub offset_overrides: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocateMethod {
    SphereScan,
    Rtti,
}

/// Progress notifications, structured so the UI can localise them.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InjectEvent {
    Attached {
        pid: u32,
        build: Option<String>,
    },
    /// Looking for a template with this many layers.
    Scanning {
        layers: usize,
    },
    ScanProgress {
        scanned: u32,
        total: u32,
        hits: u32,
    },
    /// The fast scan missed everywhere; the slow RTTI scan starts (minutes).
    RttiFallback,
    RttiPhase {
        phase: u8,
    },
    Located {
        slots: usize,
        method: LocateMethod,
    },
    Writing {
        written: usize,
        total: usize,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct InjectReport {
    pub slots: usize,
    pub total: usize,
    pub written: usize,
    /// Layers that failed revalidation or a write and were left untouched.
    pub skipped: usize,
    pub bytes: usize,
    pub method: LocateMethod,
    /// Shapes written, per type name.
    pub type_counts: BTreeMap<&'static str, usize>,
    pub game_build: Option<String>,
    pub validated_build: Option<&'static str>,
    /// Keys applied from the offsets override file.
    pub overrides_applied: Vec<&'static str>,
}

fn forward_scan(e: ScanEvent, on_event: &mut dyn FnMut(InjectEvent)) {
    on_event(match e {
        ScanEvent::Progress {
            scanned,
            total,
            hits,
        } => InjectEvent::ScanProgress {
            scanned,
            total,
            hits,
        },
        ScanEvent::Phase(p) => InjectEvent::RttiPhase { phase: p as u8 + 1 },
    });
}

/// Templates sizes to try, exact count first.
fn candidate_sizes(shapes: usize) -> Vec<usize> {
    let mut sizes = vec![shapes];
    sizes.extend(COMMON_TEMPLATE_SIZES.into_iter().filter(|&c| c > shapes));
    sizes
}

pub fn inject(
    mem: &dyn ProcessMemory,
    game: &GameProfile,
    shapes: &[Shape],
    options: &InjectOptions,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(InjectEvent),
) -> Result<InjectReport, InjectError> {
    if shapes.is_empty() {
        return Err(InjectError::NoShapes);
    }
    // Apply local offset overrides on top of the built-in layout.
    let (layout, overrides_applied) = match &options.offset_overrides {
        Some(json) => game.layout.with_overrides(game.key, json),
        None => (game.layout.clone(), Vec::new()),
    };
    let mut profile = game.clone();
    profile.layout = layout;

    let (located, slots, method) = find_group(mem, &profile, shapes.len(), cancel, on_event)?;
    if shapes.len() > slots {
        return Err(InjectError::TooManyShapes {
            slots,
            shapes: shapes.len(),
        });
    }
    on_event(InjectEvent::Located { slots, method });

    let layers = locator::read_layer_table(mem, located.table_addr, slots);
    let mut report = write_shapes(
        mem,
        &profile.layout,
        shapes,
        &layers,
        slots,
        method,
        cancel,
        on_event,
    )?;
    report.validated_build = game.validated_build;
    report.overrides_applied = overrides_applied;
    Ok(report)
}

fn find_group(
    mem: &dyn ProcessMemory,
    profile: &GameProfile,
    shapes: usize,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(InjectEvent),
) -> Result<(Located, usize, LocateMethod), InjectError> {
    let sizes = candidate_sizes(shapes);

    // Phase 1: fast sphere scan, one pass over the heap for every plausible
    // template size, BEFORE any slow fallback (a tiny JSON must not pay a
    // multi-minute RTTI scan while a 1500-layer template sits right there).
    on_event(InjectEvent::Scanning { layers: sizes[0] });
    if let Some(f) = locator::locate_by_sphere_scan(mem, profile, &sizes, cancel, &mut |e| {
        forward_scan(e, on_event)
    })? {
        return Ok((f.at, f.layers, LocateMethod::SphereScan));
    }

    // Phase 2: RTTI (works on already-painted templates; slow).
    on_event(InjectEvent::RttiFallback);
    if let Some(f) = locator::locate_by_rtti(mem, profile, &sizes, cancel, &mut |e| {
        forward_scan(e, on_event)
    })? {
        return Ok((f.at, f.layers, LocateMethod::Rtti));
    }
    Err(InjectError::NoMatch)
}

#[allow(clippy::too_many_arguments)]
fn write_shapes(
    mem: &dyn ProcessMemory,
    layout: &Layout,
    shapes: &[Shape],
    layers: &[u64],
    slots: usize,
    method: LocateMethod,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(InjectEvent),
) -> Result<InjectReport, InjectError> {
    let total = shapes.len();
    let (mut written, mut skipped, mut bytes) = (0usize, 0usize, 0usize);
    let mut type_counts: BTreeMap<&'static str, usize> = BTreeMap::new();

    for (shape, &ptr) in shapes.iter().zip(layers) {
        if cancel.load(Ordering::Relaxed) {
            return Err(InjectError::Cancelled);
        }
        // Revalidate right before writing: never write through a layer that
        // stopped looking like one (freed/moved by the game, or a near-miss).
        // A group found by C++ type (RTTI) may hold arbitrary painted layers, so
        // it only gets the loose check; a fingerprint match must be 5/5.
        let sane = is_user_ptr(ptr)
            && match method {
                LocateMethod::SphereScan => score_layer(mem, layout, ptr) >= 5,
                LocateMethod::Rtti => loose_valid_layer(mem, layout, ptr),
            };
        if !sane {
            skipped += 1;
        } else {
            match write_layer(mem, layout, shape, ptr) {
                Ok(n) => {
                    bytes += n;
                    written += 1;
                    *type_counts.entry(shape.kind().name()).or_default() += 1;
                }
                Err(_) => skipped += 1,
            }
        }
        on_event(InjectEvent::Writing { written, total });
    }

    Ok(InjectReport {
        slots,
        total,
        written,
        skipped,
        bytes,
        method,
        type_counts,
        game_build: None,
        validated_build: None,
        overrides_applied: Vec::new(),
    })
}

fn write_layer(
    mem: &dyn ProcessMemory,
    layout: &Layout,
    shape: &Shape,
    ptr: u64,
) -> Result<usize, InjectError> {
    let v = layer_values(shape, layout);
    let f2 = |[a, b]: [f32; 2]| -> Vec<u8> { [a.to_le_bytes(), b.to_le_bytes()].concat() };
    mem.write(ptr + layout.position_offset, &f2(v.position))?;
    mem.write(ptr + layout.scale_offset, &f2(v.scale))?;
    mem.write(ptr + layout.rotation_offset, &v.rotation.to_le_bytes())?;
    if layout.mask_offset == layout.color_offset + 4 {
        // Colour and mask are adjacent: one syscall instead of two.
        let [r, g, b, a] = v.color;
        mem.write(ptr + layout.color_offset, &[r, g, b, a, 0])?;
    } else {
        mem.write(ptr + layout.color_offset, &v.color)?;
        mem.write(ptr + layout.mask_offset, &[0])?;
    }
    mem.write(ptr + layout.shape_id_offset, &[v.shape_id])?;
    Ok(8 + 8 + 4 + 4 + 1 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inject::memory::RegionKind;
    use crate::inject::mock::MockProcess;
    use crate::inject::FH6;

    const HEAP: u64 = 0x2000_0000;
    const LAYER_SIZE: u64 = 0x100;

    /// Lays out one livery group with `n` layers inside the heap block, starting at
    /// `at` (relative to HEAP). Returns the group address.
    fn build_group(p: &MockProcess, at: u64, n: usize, fresh: bool, vtable: u64) -> u64 {
        let group = HEAP + at;
        let table = group + 0x200;
        let layers = table + (n as u64) * 8 + 0x40;
        p.poke(group + 0x5A, &(n as u16).to_le_bytes());
        p.poke_u64(group + 0x78, table);
        if vtable != 0 {
            p.poke_u64(group, vtable);
        }
        for i in 0..n {
            let l = layers + i as u64 * LAYER_SIZE;
            p.poke_u64(table + i as u64 * 8, l);
            // Fresh sphere: centred, scale 0.5, ellipse id, mask 0.
            p.poke_f32s(l + 0x18, &[10.0 + i as f32, -10.0]);
            p.poke_f32s(l + 0x28, &if fresh { [0.5, 0.5] } else { [100.0, 100.0] });
            p.poke(l + 0x74, &[255, 255, 255, 255]);
            p.poke(l + 0x78, &[0]);
            p.poke(l + 0x7A, &[if fresh { 102 } else { 7 }]);
        }
        group
    }

    fn layer_addr(p: &MockProcess, group: u64, i: u64) -> u64 {
        let table = u64::from_le_bytes(p.peek(group + 0x78, 8).try_into().unwrap());
        u64::from_le_bytes(p.peek(table + i * 8, 8).try_into().unwrap())
    }

    fn world() -> MockProcess {
        let p = MockProcess::new().with_module_base(0x1_4000_0000);
        p.add(HEAP, 0x20_000, RegionKind::Private, true);
        p
    }

    fn ellipse(i: usize) -> Shape {
        Shape::RotatedEllipse {
            x: 100.0 + i as f32,
            y: 50.0,
            rx: 63.0,
            ry: 31.5,
            angle: 90.0,
            color: [10, 20, 30, 128],
        }
    }

    fn run(p: &MockProcess, shapes: &[Shape]) -> Result<InjectReport, InjectError> {
        inject(
            p,
            &FH6,
            shapes,
            &InjectOptions::default(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
    }

    #[test]
    fn injects_into_a_fresh_template_larger_than_the_document() {
        let p = world();
        let group = build_group(&p, 0x100, 10, true, 0);
        let shapes: Vec<_> = (0..4).map(ellipse).collect();
        let report = run(&p, &shapes).unwrap();
        assert_eq!((report.written, report.skipped, report.slots), (4, 0, 10));
        assert_eq!(report.method, LocateMethod::SphereScan);
        let l1 = layer_addr(&p, group, 1);
        assert_eq!(p.peek_f32s(l1 + 0x18, 2), vec![101.0, -50.0]);
        assert_eq!(p.peek_f32s(l1 + 0x28, 2), vec![1.0, 0.5]);
        assert_eq!(p.peek_f32s(l1 + 0x50, 1), vec![270.0]);
        assert_eq!(p.peek(l1 + 0x74, 4), vec![10, 20, 30, 255]);
        assert_eq!(p.peek(l1 + 0x7A, 1), vec![102]);
        // Unused layers keep their original values.
        assert_eq!(
            p.peek_f32s(layer_addr(&p, group, 7) + 0x18, 2),
            vec![17.0, -10.0]
        );
    }

    #[test]
    fn ignores_decoy_groups_whose_table_is_garbage() {
        let p = world();
        // Decoy with the right count but a table full of null layer pointers.
        let decoy = HEAP + 0x8000;
        p.poke(decoy + 0x5A, &10u16.to_le_bytes());
        p.poke_u64(decoy + 0x78, HEAP + 0x9000);
        let real = build_group(&p, 0x10000, 10, true, 0);
        let report = run(&p, &[ellipse(0)]).unwrap();
        assert_eq!(report.written, 1);
        let first = layer_addr(&p, real, 0);
        assert_eq!(p.peek_f32s(first + 0x18, 2), vec![100.0, -50.0]);
    }

    #[test]
    fn reports_no_match_when_nothing_looks_like_a_template() {
        let p = world();
        assert!(matches!(run(&p, &[ellipse(0)]), Err(InjectError::NoMatch)));
    }

    #[test]
    fn empty_document_is_rejected() {
        assert!(matches!(run(&world(), &[]), Err(InjectError::NoShapes)));
    }

    #[test]
    fn cancel_flag_aborts_the_scan() {
        let p = world();
        build_group(&p, 0x100, 10, true, 0);
        let cancel = AtomicBool::new(true);
        let r = inject(
            &p,
            &FH6,
            &[ellipse(0)],
            &InjectOptions::default(),
            &cancel,
            &mut |_| {},
        );
        assert!(matches!(r, Err(InjectError::Cancelled)));
    }

    #[test]
    fn rtti_fallback_finds_and_repaints_a_painted_template() {
        let module = 0x1_4000_0000u64;
        let p = MockProcess::new().with_module_base(module);
        p.add(module, 0x4000, RegionKind::Image, false);
        p.add(HEAP, 0x20_000, RegionKind::Private, true);
        // TypeDescriptor at +0x1000 with the class name at +0x10.
        p.poke(module + 0x1010, b".?AVCLiveryGroup@@\0");
        // CompleteObjectLocator at +0x2000: signature 1, TypeDescriptor RVA at +0x0C.
        p.poke(module + 0x2000, &1u32.to_le_bytes());
        p.poke(module + 0x200C, &0x1000u32.to_le_bytes());
        // vtable[-1] holds the COL pointer.
        p.poke_u64(module + 0x3000, module + 0x2000);
        let vtable = module + 0x3008;
        // Painted: scale 100 and shape id 7 fail the strict fingerprint.
        let group = build_group(&p, 0x100, 10, false, vtable);

        let mut events = Vec::new();
        let report = inject(
            &p,
            &FH6,
            &[ellipse(0), ellipse(1)],
            &InjectOptions::default(),
            &AtomicBool::new(false),
            &mut |e| events.push(e),
        )
        .unwrap();
        assert_eq!(report.method, LocateMethod::Rtti);
        assert_eq!(report.written, 2);
        assert!(events
            .iter()
            .any(|e| matches!(e, InjectEvent::RttiFallback)));
        let l0 = layer_addr(&p, group, 0);
        assert_eq!(p.peek(l0 + 0x7A, 1), vec![102]);
    }

    #[test]
    fn layers_that_fail_revalidation_are_skipped_not_written() {
        let p = world();
        let group = build_group(&p, 0x100, 20, true, 0);
        // The first 16 layers must all be perfect to be located at all; a bad
        // layer further down still leaves 19/20 (>= 85 %) so the group is found,
        // but that layer must never be written through.
        let bad = layer_addr(&p, group, 18);
        p.poke(bad + 0x78, &[9]);
        let shapes: Vec<_> = (0..20).map(ellipse).collect();
        let report = run(&p, &shapes).unwrap();
        assert_eq!((report.written, report.skipped), (19, 1));
        assert_eq!(p.peek_f32s(bad + 0x18, 2), vec![28.0, -10.0]);
    }

    #[test]
    fn a_bad_layer_among_the_first_sixteen_rejects_the_group() {
        let p = world();
        let group = build_group(&p, 0x100, 20, true, 0);
        p.poke(layer_addr(&p, group, 3) + 0x78, &[9]);
        assert!(matches!(run(&p, &[ellipse(0)]), Err(InjectError::NoMatch)));
    }

    #[test]
    fn prefers_the_exact_template_size_over_a_bigger_one() {
        let p = world();
        build_group(&p, 0x100, 20, true, 0);
        build_group(&p, 0x8000, 10, true, 0);
        let shapes: Vec<_> = (0..10).map(ellipse).collect();
        assert_eq!(run(&p, &shapes).unwrap().slots, 10);
    }

    #[test]
    fn without_an_exact_size_the_earliest_listed_template_wins() {
        // sizes tried for 5 shapes: [5, 500, 1500, 3000, 1000, 100, 50, 20, 10]
        let p = world();
        build_group(&p, 0x100, 10, true, 0);
        build_group(&p, 0x8000, 20, true, 0);
        let shapes: Vec<_> = (0..5).map(ellipse).collect();
        assert_eq!(run(&p, &shapes).unwrap().slots, 20);
    }

    #[test]
    fn every_template_size_is_searched_in_a_single_heap_pass() {
        let p = world();
        assert!(matches!(run(&p, &[ellipse(0)]), Err(InjectError::NoMatch)));
        // 9 candidate sizes used to mean 9 full passes over the heap.
        assert!(
            p.bytes_read_from(HEAP) < 2 * 0x20_000,
            "{}",
            p.bytes_read_from(HEAP)
        );
    }

    #[test]
    fn finds_a_group_that_starts_in_the_previous_scan_chunk() {
        let chunk = crate::inject::memory::SCAN_CHUNK as u64;
        let p = MockProcess::new().with_module_base(0x1_4000_0000);
        p.add(HEAP, chunk as usize + 0x20_000, RegionKind::Private, true);
        // The group (8-aligned) starts in chunk 0 but its count field lands in chunk 1.
        let group = build_group(&p, chunk - 88, 10, true, 0);
        let report = run(&p, &[ellipse(0)]).unwrap();
        assert_eq!(report.written, 1);
        assert_eq!(
            p.peek_f32s(layer_addr(&p, group, 0) + 0x18, 2),
            vec![100.0, -50.0]
        );
    }

    #[test]
    fn writing_clears_a_set_mask_flag() {
        let p = world();
        let group = build_group(&p, 0x100, 10, true, 0);
        let l0 = layer_addr(&p, group, 0);
        p.poke(l0 + 0x78, &[1]); // still a plausible fresh layer
        run(&p, &[ellipse(0)]).unwrap();
        assert_eq!(p.peek(l0 + 0x74, 5), vec![10, 20, 30, 255, 0]);
    }

    #[test]
    fn rtti_scan_never_reads_code_pages_when_data_pages_suffice() {
        let module = 0x1_4000_0000u64;
        let code = 0x2_0000_0000u64;
        let p = MockProcess::new().with_module_base(module);
        p.add(module, 0x4000, RegionKind::Image, false);
        p.add_code(code, 0x10_000);
        p.add(HEAP, 0x20_000, RegionKind::Private, true);
        p.poke(module + 0x1010, b".?AVCLiveryGroup@@\0");
        p.poke(module + 0x2000, &1u32.to_le_bytes());
        p.poke(module + 0x200C, &0x1000u32.to_le_bytes());
        p.poke_u64(module + 0x3000, module + 0x2000);
        build_group(&p, 0x100, 10, false, module + 0x3008);
        let report = run(&p, &[ellipse(0)]).unwrap();
        assert_eq!(report.method, LocateMethod::Rtti);
        assert_eq!(p.bytes_read_from(code), 0);
    }

    #[test]
    fn rtti_falls_back_to_code_pages_when_the_data_pages_have_no_class_name() {
        let module = 0x1_4000_0000u64;
        let p = MockProcess::new().with_module_base(module);
        // Everything (as in a packed executable) sits in one executable image block.
        p.add_code(module, 0x4000);
        p.add(HEAP, 0x20_000, RegionKind::Private, true);
        p.poke(module + 0x1010, b".?AVCLiveryGroup@@\0");
        p.poke(module + 0x2000, &1u32.to_le_bytes());
        p.poke(module + 0x200C, &0x1000u32.to_le_bytes());
        p.poke_u64(module + 0x3000, module + 0x2000);
        build_group(&p, 0x100, 10, false, module + 0x3008);
        assert_eq!(run(&p, &[ellipse(0)]).unwrap().method, LocateMethod::Rtti);
    }

    /// Throughput of the sphere scan over a large, mostly-noisy heap.
    /// `cargo test --release -p fd-core scan_throughput -- --ignored --nocapture`
    #[test]
    #[ignore = "benchmark"]
    fn scan_throughput() {
        let size = 256 * 1024 * 1024usize;
        let p = MockProcess::new().with_module_base(0x1_4000_0000);
        p.add(HEAP, size, RegionKind::Private, true);
        // Sprinkle values equal to common layer counts so the pre-filter has work to do.
        let noise = 500u16.to_le_bytes();
        for i in (0..size - 16).step_by(4096) {
            p.poke(HEAP + i as u64, &noise);
        }
        let started = std::time::Instant::now();
        let r = run(&p, &[ellipse(0)]);
        let secs = started.elapsed().as_secs_f64();
        println!(
            "{} MiB scanned in {secs:.2}s = {:.0} MiB/s ({r:?})",
            size >> 20,
            (size >> 20) as f64 / secs
        );
    }
}
