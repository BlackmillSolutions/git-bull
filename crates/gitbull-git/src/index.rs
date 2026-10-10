//! Staging and unstaging whole files (spec `staging`; spec `git-integration`,
//! requirement "Index operations").
//!
//! Both operations change the index only, and only for the paths they are
//! given. The paths go to Git on its standard input, each ended by a NUL, so
//! that their number does not depend on the length a command line may have,
//! and every invocation reads them literally. They run through the explicit
//! write invocation (ADR 0007), so that the clean filters of the repository
//! run as they do for `git add`.

use std::path::Path;

use crate::cancel::CancelToken;
use crate::invoke::Git;
use crate::path::RepoPath;
use crate::refusal::WriteFailure;
use crate::write::WriteHooks;

/// Reads the paths from standard input, each ended by a NUL.
const PATHS_FROM_INPUT: [&str; 2] = ["--pathspec-from-file=-", "--pathspec-file-nul"];

/// Stages `paths` as they are in the working copy at `repo`: a modification,
/// an addition, a deletion, or the commit a submodule has checked out.
///
/// Without a path nothing runs. A failure keeps Git's exit status and message
/// and is never read as a refusal: a filter prints what it likes.
pub fn stage(
    git: &Git,
    repo: &Path,
    paths: &[RepoPath],
    cancel: &CancelToken,
) -> Result<(), WriteFailure> {
    run(git, repo, "add", &[], paths, cancel)
}

/// Unstages `paths` in the repository at `repo`: the index holds each as the
/// last commit has it, or not at all when the last commit does not have it or
/// there is no commit. The working copy and the state of a merge stay as they
/// are. A staged rename is unstaged by both of its paths.
///
/// Without a path nothing runs, and this is a guard: `git reset` without a
/// path resets the whole index and ends a merge that is under way.
pub fn unstage(
    git: &Git,
    repo: &Path,
    paths: &[RepoPath],
    cancel: &CancelToken,
) -> Result<(), WriteFailure> {
    run(git, repo, "reset", &["-q"], paths, cancel)
}

fn run(
    git: &Git,
    repo: &Path,
    command: &str,
    options: &[&str],
    paths: &[RepoPath],
    cancel: &CancelToken,
) -> Result<(), WriteFailure> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
    }
    let args = [command]
        .into_iter()
        .chain(options.iter().copied())
        .chain(PATHS_FROM_INPUT);
    git.write(WriteHooks::Run)
        .run(repo, args, Some(&input), cancel)
        // Not `WriteFailure::from_error`, which reads the refusals of a
        // checkout out of Git's error output.
        .map_err(WriteFailure::Failed)
}
