use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use fd_core::shapegen::Control;

/// The (at most one) running generation and injection, so the UI can stop them.
#[derive(Default)]
pub struct Jobs {
    generation: Mutex<Option<Control>>,
    injection: Mutex<Option<Arc<AtomicBool>>>,
}

/// A poisoned lock only means another command panicked; the data is still valid.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Jobs {
    /// Registers a generation; `None` if one is already running.
    pub fn begin_generation(&self) -> Option<Control> {
        let mut slot = lock(&self.generation);
        if slot.is_some() {
            return None;
        }
        let control = Control::default();
        *slot = Some(control.clone());
        Some(control)
    }

    pub fn end_generation(&self) {
        *lock(&self.generation) = None;
    }

    pub fn generation(&self) -> Option<Control> {
        lock(&self.generation).clone()
    }

    pub fn begin_injection(&self) -> Option<Arc<AtomicBool>> {
        let mut slot = lock(&self.injection);
        if slot.is_some() {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        *slot = Some(Arc::clone(&flag));
        Some(flag)
    }

    pub fn end_injection(&self) {
        *lock(&self.injection) = None;
    }

    pub fn injection(&self) -> Option<Arc<AtomicBool>> {
        lock(&self.injection).clone()
    }
}
