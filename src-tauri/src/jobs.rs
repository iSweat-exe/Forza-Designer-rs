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

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn only_one_generation_at_a_time() {
        let jobs = Jobs::default();
        assert!(jobs.begin_generation().is_some());
        assert!(jobs.begin_generation().is_none());
        jobs.end_generation();
        assert!(jobs.begin_generation().is_some());
    }

    #[test]
    fn stop_reaches_the_running_generation() {
        let jobs = Jobs::default();
        let control = jobs.begin_generation().unwrap();
        jobs.generation().unwrap().request_stop();
        assert!(control.is_stopped());
    }

    #[test]
    fn no_generation_to_control_when_idle() {
        assert!(Jobs::default().generation().is_none());
    }

    #[test]
    fn only_one_injection_at_a_time_and_cancel_is_shared() {
        let jobs = Jobs::default();
        let flag = jobs.begin_injection().unwrap();
        assert!(jobs.begin_injection().is_none());
        jobs.injection().unwrap().store(true, Ordering::Relaxed);
        assert!(flag.load(Ordering::Relaxed));
        jobs.end_injection();
        assert!(jobs.injection().is_none());
        assert!(
            !jobs.begin_injection().unwrap().load(Ordering::Relaxed),
            "fresh flag per job"
        );
    }

    #[test]
    fn a_poisoned_lock_does_not_wedge_the_app() {
        let jobs = std::sync::Arc::new(Jobs::default());
        let j = jobs.clone();
        let _ = std::thread::spawn(move || {
            let _guard = j.generation.lock().unwrap();
            panic!("boom");
        })
        .join();
        assert!(jobs.begin_generation().is_some());
    }
}
