//! Forza Horizon vinyl injection.
//!
//! Strategy (shared by the FH4/5/6 "Forge" engine lineage): find the live
//! `CLiveryGroup` in the game's heap, follow its layer table (an array of
//! pointers to independently heap-allocated layer structs) and overwrite each
//! layer's position / scale / rotation / colour / shape id. Writes propagate to
//! the renderer immediately; no commit step is needed.
//!
//! Everything above the raw read/write syscalls goes through [`ProcessMemory`],
//! so the locator and writer are unit-tested against an in-memory fake process.

mod injector;
mod layer;
mod layout;
mod locator;
mod memory;
#[cfg(test)]
mod mock;
#[cfg(windows)]
mod windows;

use std::sync::atomic::AtomicBool;

pub use injector::{inject, InjectEvent, InjectOptions, InjectReport, LocateMethod};
pub use layout::{GameProfile, Layout, FH6};
pub use memory::{MemoryRegion, ProcessMemory, RegionKind};

/// All games we can inject into. Add FH5/FH4/FH3/... here as they are validated.
pub fn supported_games() -> &'static [&'static GameProfile] {
    &[&FH6]
}

pub fn game_by_key(key: &str) -> Option<&'static GameProfile> {
    supported_games()
        .iter()
        .copied()
        .find(|g| g.key.eq_ignore_ascii_case(key.trim()))
}

#[derive(Debug, thiserror::Error)]
pub enum InjectError {
    #[error(
        "{game} is not running, or this app has fewer privileges than the game. \
         If the game is open, run both the game and this app as Administrator (looked for: {names})"
    )]
    NotRunning { game: &'static str, names: String },
    #[error(
        "could not open the game process (pid {pid}): {reason}. Run this app as Administrator."
    )]
    Attach { pid: u32, reason: String },
    #[error("injection is only available on Windows")]
    UnsupportedPlatform,
    #[error("no confident vinyl group match. Open the vinyl editor with a fresh, unmodified sphere template (500/1500/3000 layers) and try again")]
    NoMatch,
    #[error("the template has {slots} layer slots but the document has {shapes} shapes; load a larger template")]
    TooManyShapes { slots: usize, shapes: usize },
    #[error("there are no shapes to inject")]
    NoShapes,
    #[error("cancelled")]
    Cancelled,
    #[error("memory access failed at {addr:#x}: {reason}")]
    Memory { addr: u64, reason: String },
}

/// Opens the running game and injects `shapes`. Windows only.
pub fn attach_and_inject(
    game: &'static GameProfile,
    shapes: &[crate::shapegen::Shape],
    options: &InjectOptions,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(InjectEvent),
) -> Result<InjectReport, InjectError> {
    #[cfg(windows)]
    {
        let proc = windows::GameProcess::attach(game)?;
        on_event(InjectEvent::Attached {
            pid: proc.pid(),
            build: proc.game_build(),
        });
        let mut report = inject(&proc, game, shapes, options, cancel, on_event)?;
        report.game_build = proc.game_build();
        Ok(report)
    }
    #[cfg(not(windows))]
    {
        let _ = (game, shapes, options, cancel, on_event);
        Err(InjectError::UnsupportedPlatform)
    }
}

/// Is the game currently running? (Always `false` off Windows.)
pub fn is_game_running(game: &GameProfile) -> bool {
    #[cfg(windows)]
    {
        windows::find_process_id(game.process_names).is_some()
    }
    #[cfg(not(windows))]
    {
        let _ = game;
        false
    }
}
