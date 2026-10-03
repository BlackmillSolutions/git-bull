//! Core logic of git-bull.
//!
//! Holds the state of each open repository and accepts actions from the UI.
//! Depends on `gitbull-git` only through its backend trait and never on the
//! UI framework.

pub mod badges;
pub mod base;
pub mod blame;
pub mod comparison;
pub mod content_cache;
pub mod details;
pub mod diff_document;
pub mod diff_pane;
pub mod file_history;
pub mod file_status;
pub mod file_tree;
pub mod git_setup;
pub mod graph;
pub mod highlight;
pub mod links;
pub mod opening;
pub mod overlaps;
pub mod overview;
pub mod panel;
mod pending;
pub mod repositories;
pub mod search;
pub mod seen;
pub mod session;
pub mod settings;
pub mod sidebar_tree;
pub mod state;
pub mod store;
pub mod workspace;
