//! Git access for git-bull.
//!
//! Runs the installed Git executable, parses its output and returns typed
//! data. Every invocation applies the rules of ADR 0006.

pub mod backend;
pub mod error;
pub mod filters;
pub mod flags;
pub mod invoke;
pub mod locate;
pub mod log;
pub mod process;
pub mod records;
pub mod repository;
pub mod version;

pub use backend::{Backend, CliBackend};
pub use error::Error;
pub use invoke::{ConfigOverride, Git};
pub use process::{Canceller, Process};
