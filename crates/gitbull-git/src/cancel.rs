//! Stopping the background work of a repository at once.

use std::sync::{Arc, Mutex};

type Callback = Box<dyn FnOnce() + Send>;

/// Stops the work registered with it, such as running Git processes, when
/// cancelled. Clones share the same state.
#[derive(Clone, Default)]
pub struct CancelToken {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    cancelled: bool,
    next: u64,
    callbacks: Vec<(u64, Callback)>,
}

/// Identifies a callback of a [`CancelToken`], to forget it once the work
/// it stops has finished.
#[derive(Debug, PartialEq, Eq)]
pub struct Registration(u64);

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }

    /// Runs every registered callback; later ones run as they register.
    pub fn cancel(&self) {
        let callbacks = {
            let mut inner = self.lock();
            inner.cancelled = true;
            std::mem::take(&mut inner.callbacks)
        };
        // Outside the lock, so that a callback may use the token.
        for (_, stop) in callbacks {
            stop();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// Runs `stop` on cancellation, or at once if already cancelled.
    pub fn on_cancel(&self, stop: impl FnOnce() + Send + 'static) -> Registration {
        let mut inner = self.lock();
        let id = inner.next;
        inner.next += 1;
        if inner.cancelled {
            drop(inner);
            stop();
        } else {
            inner.callbacks.push((id, Box::new(stop)));
        }
        Registration(id)
    }

    /// Drops a callback whose work has finished.
    pub fn forget(&self, registration: Registration) {
        self.lock()
            .callbacks
            .retain(|(id, _)| *id != registration.0);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counter() -> (Arc<AtomicUsize>, impl Fn() -> Box<dyn FnOnce() + Send>) {
        let count = Arc::new(AtomicUsize::new(0));
        let shared = Arc::clone(&count);
        let make = move || -> Box<dyn FnOnce() + Send> {
            let count = Arc::clone(&shared);
            Box::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            })
        };
        (count, make)
    }

    #[test]
    fn cancelling_runs_every_callback_once() {
        let (count, make) = counter();
        let token = CancelToken::new();
        token.on_cancel(make());
        token.clone().on_cancel(make());
        assert!(!token.is_cancelled());
        token.cancel();
        token.cancel();
        assert!(token.is_cancelled());
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_callback_registered_after_cancelling_runs_at_once() {
        let (count, make) = counter();
        let token = CancelToken::new();
        token.cancel();
        token.on_cancel(make());
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_forgotten_callback_does_not_run() {
        let (count, make) = counter();
        let token = CancelToken::new();
        let registration = token.on_cancel(make());
        token.on_cancel(make());
        token.forget(registration);
        token.cancel();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_callback_may_use_the_token() {
        let token = CancelToken::new();
        let inner = token.clone();
        token.on_cancel(move || assert!(inner.is_cancelled()));
        token.cancel();
    }
}
