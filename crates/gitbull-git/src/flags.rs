//! Arguments that ADR 0006 requires on particular commands, in addition to
//! the global rules that [`crate::Git`] applies to every invocation.

/// On every command that shows a diff: no external tools, no text
/// conversion, output that configuration cannot reshape, and no diffs from
/// inside submodules.
pub const DIFF: &[&str] = &[
    "--no-ext-diff",
    "--no-textconv",
    "--no-color",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--submodule=short",
    "--ignore-submodules=dirty",
];

/// The working-copy status: every untracked file listed, and Git kept out
/// of submodules.
pub const STATUS: &[&str] = &[
    "status",
    "--porcelain=v2",
    "-z",
    "--untracked-files=all",
    "--ignore-submodules=dirty",
];

/// The summary of a working copy for the home tab: as [`STATUS`], with the
/// branch, and without walking into a folder whose content is all
/// untracked, which Git then reports once.
pub const SUMMARY: &[&str] = &[
    "status",
    "--porcelain=v2",
    "-z",
    "--branch",
    "--untracked-files=normal",
    "--ignore-submodules=dirty",
];

/// On every blame: no text conversion, and no ignore file that the
/// repository names. The user's own ignore files are passed after these.
pub const BLAME: &[&str] = &["--incremental", "--no-textconv", "--no-ignore-revs-file"];
