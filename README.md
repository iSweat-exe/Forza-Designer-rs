# Forza Designer (Rust)

Turn any image into a Forza Horizon **vinyl group** and inject it into the running game.
A Rust + [Tauri v2](https://tauri.app) + React rewrite of
[Forza Designer 6](https://github.com/tokyubevoxelverse/ForzaDesigner6) (originally Python, MIT) — all credit for
the original idea, algorithm and research goes to its contributors and to
[bvzrays/forza-painter-fh6](https://github.com/bvzrays/forza-painter-fh6), from which the injection layout was learned.

| Status | Feature |
|---|---|
| ✅ | **Image → shapes** generation (greedy random search + hill climbing, edge-weighted, parallel) |
| ✅ | **Injection into Forza Horizon 6** (Windows): sphere-template scan, RTTI fallback, validated writes |
| ✅ | Python-compatible `fd6.shapes` JSON (existing files load; new files load in the Python app) |
| ✅ | Live preview, pause/stop, checkpoints, sticker (transparent) mode, FR/EN UI |
| 🔜 | FH5 / FH4 / FH3 (add a `GameProfile`), Assetto Corsa Competizione, GPU search, resume |

## Using it

**Generate** — pick an image, a quality profile and a shape count, press *Start*. The JSON is saved next to the
image (`<image>/<name>.json`, plus checkpoints every 100 shapes).

**Inject** (Windows only)
1. Start Forza Horizon 6 and open the *Vinyl Group* editor.
2. Load a **fresh, ungrouped sphere template** with at least as many layers as you have shapes (500 / 1500 / 3000).
3. In the *Inject* tab press *Inject*. Don't touch the game or the app while it writes.
4. If attaching fails, run **both the game and this app as Administrator**.

An already-painted template also works: the fast scan misses, then a slower type (RTTI) scan runs (2–5 min).

If a game patch moves a struct offset you can fix it without rebuilding: copy
write a [`.fd6_offsets.json`](#offset-overrides) next to the app.

### Offset overrides
`.fd6_offsets.json` (working directory or next to the executable), same format as the Python app:
```json
{ "fh6": { "layer_color_offset": "0x74", "scale_divisor_ellipse": 63.0 } }
```
Keys: `livery_count_offset`, `layer_table_offset`, `layer_position_offset`, `layer_scale_offset`,
`layer_rotation_offset`, `layer_color_offset`, `layer_mask_offset`, `layer_shape_id_offset`,
`scale_divisor_ellipse`, `scale_divisor_other`, `shape_id_ellipse`, `shape_id_other`.

> ⚠️ Memory injection into an online game may violate its terms of service. Use at your own risk, offline.

## Architecture

```
crates/fd-core     pure Rust, no GUI, fully unit-tested (also on Linux)
  shapegen/        preprocessing, shapes, edge-weighted scoring, parallel engine (rayon)
  inject/          ProcessMemory trait · locators · layer mapping · Win32 backend (windows-sys)
  document.rs      fd6.shapes JSON
src-tauri          thin Tauri v2 command layer (events streamed over Channels)
src                React + TypeScript UI (reducers in hooks/, i18n in i18n/)
```

The injector talks to the game only through the `ProcessMemory` trait, so the locator and writer are tested against
an in-memory fake process (fresh templates, decoys, painted templates found by RTTI, corrupted layers, cancel…).

### Performance
* **Shape search:** shapes are covered by analytic scanline spans, corrected against the exact per-pixel
  predicate (equivalence-tested), with O(1) transparent-overlap checks. Output is bit-identical to the naive
  version; single-thread throughput went from 3.9 to ~10 shapes/s on a 1000×900 image
  (`cargo run --release -p fd-core --example bench -- photo.png 150`).
* **Memory scan:** every template size is searched in **one** pass over the heap (not one per size), chunks are read
  by several threads, the RTTI chain is resolved once and skips code pages, colour+mask are written in one call.
  The scan reaches ~1.2 GiB/s on the in-memory fake process
  (`cargo test --release -p fd-core scan_throughput -- --ignored --nocapture`).

### Differences from the Python app
* The engine fits shapes **fully opaque** (`shape_alpha = 255`) because Forza renders layers opaque, so the preview
  matches the game. The Python app fitted at alpha 128 and forced 255 only at injection time.
* Triangles are not generated: the game's layer struct cannot express them.
* Search runs on all CPU cores with threads instead of a process pool (no shared-memory copies).
* GPU search and the resume feature are not ported yet.

## Development

```bash
npm install
npm run tauri dev          # app with hot reload
npm run tauri build        # installer (run on Windows for the .exe / NSIS bundle)

cargo test --workspace     # core + command tests
cargo clippy --workspace --all-targets -- -D warnings
npm run check              # typecheck + eslint + vitest
npm run version:check      # Cargo.toml / package.json / tauri.conf.json agree

# headless generation, no UI
cargo run --release -p fd-core --example generate -- photo.png balanced 1500 out.json
```
Linux needs `libwebkit2gtk-4.1-dev libgtk-3-dev` to build the Tauri crate; `fd-core` builds anywhere.

## License
MIT. Contains ideas and bundled quality profiles from Forza Designer 6 — see `LICENSE-ForzaDesigner6`.

## CI/CD
* `.github/workflows/ci.yml` — on every push/PR: frontend (version check, eslint, tsc, vitest, build), `cargo fmt`,
  clippy + tests on **Linux and Windows**, a Windows-target type-check of the injection backend, dependency audits
  (`cargo audit`, `npm audit`), and (on the default branch) a Windows build artifact.
* `.github/workflows/release.yml` — push a tag `vX.Y.Z` (it must match the version in `Cargo.toml`, `package.json`
  and `tauri.conf.json`; use `node scripts/version.mjs --set X.Y.Z`). Tests run first, then an NSIS installer
  (Windows) and AppImage/deb (Linux) are attached to a **draft** GitHub release.
* Dependabot keeps Cargo, npm and Actions dependencies current (grouped weekly PRs).
