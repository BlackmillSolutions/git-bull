//! Core logic of git-bull.
//!
//! Holds the state of each open repository and accepts actions from the UI.
//! Depends on `gitbull-git` only through its backend trait and never on the
//! UI framework.

pub mod opening;
