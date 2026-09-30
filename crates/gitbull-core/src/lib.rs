//! Core logic of git-bull.
//!
//! Holds the state of each open repository and accepts actions from the UI.
//! Depends on `gitbull-git` only through its backend trait and never on the
//! UI framework.

pub mod badges;
pub mod content_cache;
pub mod git_setup;
pub mod graph;
pub mod opening;
pub mod session;
pub mod settings;
pub mod sidebar_tree;
pub mod store;
pub mod workspace;
