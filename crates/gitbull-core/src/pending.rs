//! Background work whose answer only the latest request wants.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use gitbull_git::cancel::CancelToken;

use crate::session::{catch, take};
use crate::workspace::{Failure, Notify};

/// Work in the background for one request: dropping the receiver drops its
/// answer, cancelling stops its Git process.
pub(crate) struct Pending<T> {
    cancel: CancelToken,
    result: Option<Receiver<Result<T, Failure>>>,
}

impl<T: Send + 'static> Pending<T> {
    pub(crate) fn none() -> Pending<T> {
        Pending {
            cancel: CancelToken::new(),
            result: None,
        }
    }

    /// Stops the previous work and starts `work` on a worker thread.
    pub(crate) fn start(
        &mut self,
        notify: &Notify,
        work: impl FnOnce(&CancelToken) -> Result<T, gitbull_git::Error> + Send + 'static,
    ) {
        self.stop();
        self.cancel = CancelToken::new();
        let (cancel, notify) = (self.cancel.clone(), Arc::clone(notify));
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            if sender.send(catch(|| work(&cancel))).is_ok() {
                notify();
            }
        });
        self.result = Some(receiver);
    }

    pub(crate) fn stop(&mut self) {
        self.cancel.cancel();
        self.result = None;
    }

    /// Whether work was started and has not answered yet.
    pub(crate) fn is_running(&self) -> bool {
        self.result.is_some()
    }

    pub(crate) fn take(&mut self) -> Option<Result<T, Failure>> {
        take(&mut self.result)
    }
}

impl<T> Drop for Pending<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
