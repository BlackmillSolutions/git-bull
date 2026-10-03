//! Git access for git-bull.
//!
//! Runs the installed Git executable, parses its output and returns typed
//! data. Every invocation applies the rules of ADR 0006.

pub mod backend;
pub mod batch_check;
pub mod blame;
pub mod blob;
pub mod cancel;
pub mod changes;
pub mod commit_graph;
pub mod config;
pub mod content;
pub mod diff;
pub mod error;
pub mod facts;
pub mod file_history;
pub mod filters;
pub mod flags;
pub mod head;
pub mod history;
pub mod invoke;
pub mod locate;
pub mod log;
pub mod object_id;
pub mod path;
pub mod process;
pub mod records;
pub mod refs;
pub mod repository;
pub mod search;
pub mod shallow;
pub mod stashes;
pub mod status;
pub mod summary;
pub mod version;
pub mod working_copy;
pub mod worktrees;

pub use backend::{Backend, CliBackend};
pub use error::Error;
pub use invoke::{ConfigOverride, Git};
pub use process::{Canceller, Process};
