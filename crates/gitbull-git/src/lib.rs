//! Git access for git-bull.
//!
//! Runs the installed Git executable, parses its output and returns typed
//! data. Browsing applies ADR 0006's protections; explicit user writes honour
//! ordinary Git configuration through [`Git::write`] (ADR 0007).

pub mod ai_diff;
pub mod backend;
pub mod bases;
pub mod batch_check;
pub mod blame;
pub mod blob;
pub mod cancel;
pub mod changes;
pub mod commit_graph;
pub mod commits;
pub mod compare;
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
pub mod index;
pub mod invoke;
#[cfg(windows)]
mod job;
pub mod locate;
pub mod log;
pub mod merged;
pub mod new_ref;
pub mod object_id;
pub mod path;
pub mod process;
pub mod records;
pub mod ref_name;
pub mod refs;
pub mod refusal;
pub mod repository;
pub mod search;
pub mod shallow;
pub mod stashes;
pub mod status;
pub mod summary;
pub mod switch;
pub mod uncommitted;
pub mod version;
pub mod working_copy;
pub mod worktrees;
pub mod write;

pub use backend::{Backend, CliBackend};
pub use error::Error;
pub use invoke::{ConfigOverride, Git};
pub use process::{Canceller, Process};
pub use write::{WriteHooks, WriteInvocation};
