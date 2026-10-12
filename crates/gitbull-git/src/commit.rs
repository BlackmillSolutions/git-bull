//! Creating a commit, and what the commit form reads (spec `commit`; spec
//! `git-integration`, requirements "Creating a commit", "Staging everything
//! before a commit" and "Reads for committing").
//!
//! The commit is Git's own `git commit`, run as an explicit write (ADR 0007),
//! so that hooks, filters and signing apply as the user configured them. The
//! message goes to Git on its standard input.

use std::path::Path;

use crate::cancel::CancelToken;
use crate::error::Error;
use crate::index;
use crate::invoke::{ConfigOverride, Git};
use crate::path::RepoPath;
use crate::refusal::WriteFailure;
use crate::status::{StatusKind, status};
use crate::write::WriteHooks;

/// What a commit is made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitRequest {
    /// The message as the user wrote it; Git cleans it of surrounding empty
    /// lines and trailing blanks, and of nothing else.
    pub message: String,
    /// Replace the last commit instead of adding one.
    pub amend: bool,
    /// [`WriteHooks::SkipCommitHooks`] for "Commit without hooks".
    pub hooks: WriteHooks,
    /// Stage every change of the working copy first: "Stage all changes and
    /// commit".
    pub stage_all: bool,
}

/// How a commit ended, and whether it took place, which the outcome alone
/// does not tell (ADR 0008).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitEnd {
    pub committed: bool,
    pub outcome: CommitOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitOutcome {
    /// Git ended successfully.
    Done,
    /// The commit was stopped by its token.
    Stopped,
    /// git-bull started no commit.
    Refused(CommitRefusal),
    /// What Git and its hooks printed: the error output, then the standard
    /// output, where Git says why nothing was committed.
    Failed(String),
}

/// Why git-bull itself made no commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitRefusal {
    /// A file is in conflict, so that not every change can be staged.
    Conflicts,
    /// A rebase, or a series of patches, is in progress: a commit there loses
    /// the author of the commit being replayed. Or a merge is, and every
    /// change was to be staged for it, which is not how a merge is concluded.
    InProgress(Operation),
    /// An amend while an operation is in progress would replace a commit
    /// that is not the user's last one, or one the operation still needs.
    AmendDuring(Operation),
}

/// An operation of Git that was begun and not ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Merge,
    /// Also between the commits of a series of picks.
    CherryPick,
    /// Also between the commits of a series of reverts.
    Revert,
    Rebase,
    /// `git am` applies a series of patches.
    ApplyingPatches,
}

/// What the repository is in the middle of, for the commit form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitState {
    pub operation: Option<Operation>,
    /// The message Git prepared for the next commit, of a merge, a merge
    /// that squashes, a cherry-pick or a revert, without its comment lines.
    pub prepared: Option<String>,
}

/// The message is UTF-8 whatever `i18n.commitEncoding` says: Git would label
/// these bytes with the encoding configured, and every reader would decode
/// them wrong.
fn utf8() -> [ConfigOverride; 1] {
    [ConfigOverride {
        key: "i18n.commitEncoding".to_owned(),
        value: "UTF-8".to_owned(),
    }]
}

/// Creates a commit of the staged changes in the repository at `repo`, or
/// replaces the last commit.
///
/// `cancel` stops the commit, but not the staging of `stage_all`, which
/// holds the lock on the index: a stop that arrives meanwhile takes effect
/// before the commit starts.
pub fn commit(git: &Git, repo: &Path, request: &CommitRequest, cancel: &CancelToken) -> CommitEnd {
    let not_made = |outcome| CommitEnd {
        committed: false,
        outcome,
    };
    let failed = |error: Error| not_made(CommitOutcome::Failed(message_of(error, &[])));
    // Reads and the staging are not stopped.
    let steady = CancelToken::new();
    // The form closes these, but only as fresh as its last read. They are
    // read before anything is staged: a refusal leaves the index alone.
    match commit_state(git, repo) {
        Ok(state) => match state.operation {
            Some(operation @ (Operation::Rebase | Operation::ApplyingPatches)) => {
                return not_made(CommitOutcome::Refused(CommitRefusal::InProgress(operation)));
            }
            Some(operation) if request.amend => {
                return not_made(CommitOutcome::Refused(CommitRefusal::AmendDuring(
                    operation,
                )));
            }
            // A merge is concluded with what the user staged for it, not
            // with every file that lies in the working copy.
            Some(Operation::Merge) if request.stage_all => {
                return not_made(CommitOutcome::Refused(CommitRefusal::InProgress(
                    Operation::Merge,
                )));
            }
            _ => {}
        },
        Err(error) => return failed(error),
    }
    if request.stage_all {
        let status = match status(git, repo, &steady) {
            Ok(status) => status,
            Err(error) => return failed(error),
        };
        let listed = || status.unstaged.iter().chain(&status.untracked);
        if listed().any(|entry| entry.kind == StatusKind::Conflicted) {
            return not_made(CommitOutcome::Refused(CommitRefusal::Conflicts));
        }
        // A rename is staged by both of its paths; the new one alone leaves
        // the old one as an unstaged deletion.
        let paths: Vec<RepoPath> = listed()
            .flat_map(|entry| entry.old_path.iter().chain([&entry.path]).cloned())
            .collect();
        match index::stage(git, repo, &paths, &steady) {
            Ok(()) => {}
            Err(WriteFailure::Failed(error)) => return failed(error),
            Err(WriteFailure::Refused(refusal)) => {
                return not_made(CommitOutcome::Failed(format!("{refusal:?}")));
            }
        }
    }
    if cancel.is_cancelled() {
        return not_made(CommitOutcome::Stopped);
    }

    let before = head_commit(git, repo);
    let mut args = vec!["commit", "--cleanup=whitespace", "-F", "-"];
    if request.amend {
        args.push("--amend");
    }
    let (printed, result) = git.write(request.hooks).run_keeping_output(
        repo,
        &utf8(),
        args,
        request.message.as_bytes(),
        cancel,
    );
    let outcome = match result {
        Ok(()) => {
            // An amend can give the commit it replaced: HEAD is not asked.
            return CommitEnd {
                committed: true,
                outcome: CommitOutcome::Done,
            };
        }
        Err(Error::Cancelled) => CommitOutcome::Stopped,
        Err(error) => CommitOutcome::Failed(message_of(error, &printed)),
    };
    CommitEnd {
        committed: is_ours(git, repo, request, before.as_deref()),
        outcome,
    }
}

/// The text of a failure: Git's error output, then its standard output.
fn message_of(error: Error, printed: &[u8]) -> String {
    let mut text = match error {
        Error::CommandFailed { stderr, .. } => stderr,
        other => other.to_string(),
    };
    let printed = String::from_utf8_lossy(printed);
    let printed = printed.trim_end();
    if !printed.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(printed);
    }
    text.trim_end().to_owned()
}

/// The commit HEAD names, or `None` where there is none yet.
fn head_commit(git: &Git, repo: &Path) -> Option<String> {
    let output = git
        .run(repo, &[], ["rev-parse", "-q", "--verify", "HEAD"])
        .ok()?;
    let id = String::from_utf8(output).ok()?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_owned())
}

/// The parents of `commit`.
fn parents(git: &Git, repo: &Path, commit: &str) -> Option<Vec<String>> {
    let output = git
        .run(
            repo,
            &[],
            [
                "rev-list",
                "--parents",
                "-n",
                "1",
                "--end-of-options",
                commit,
            ],
        )
        .ok()?;
    let line = String::from_utf8(output).ok()?;
    Some(line.split_whitespace().skip(1).map(str::to_owned).collect())
}

/// Whether, after a failure or a stop, HEAD names the commit `request` asked
/// for: another commit than `before`, that follows it as ours would, with the
/// message sent. A commit that a hook or another program made on top of
/// `before` follows it too, so the message decides; a commit of ours whose
/// message a hook changed is then not recognised, which keeps what the user
/// wrote.
fn is_ours(git: &Git, repo: &Path, request: &CommitRequest, before: Option<&str>) -> bool {
    let Some(after) = head_commit(git, repo) else {
        return false;
    };
    if before == Some(after.as_str()) {
        return false;
    }
    let Some(new_parents) = parents(git, repo, &after) else {
        return false;
    };
    let follows = match before {
        None => new_parents.is_empty(),
        Some(before) if request.amend => parents(git, repo, before).as_ref() == Some(&new_parents),
        Some(before) => new_parents.first().map(String::as_str) == Some(before),
    };
    if !follows {
        return false;
    }
    let sent = git.run_with_input(
        repo,
        &[],
        ["stripspace"],
        request.message.clone().into_bytes(),
        &CancelToken::new(),
    );
    let stored = git.run(
        repo,
        &[],
        [
            "log",
            "-1",
            "--encoding=UTF-8",
            "--format=%B",
            "--end-of-options",
            &after,
        ],
    );
    match (sent, stored) {
        (Ok(sent), Ok(stored)) => trimmed(&sent) == trimmed(&stored),
        _ => false,
    }
}

fn trimmed(text: &[u8]) -> &[u8] {
    let end = text
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(0, |last| last + 1);
    &text[..end]
}

/// The message of the last commit, as UTF-8; `None` in a repository without
/// a commit.
pub fn last_message(git: &Git, repo: &Path) -> Result<Option<String>, Error> {
    if head_commit(git, repo).is_none() {
        return Ok(None);
    }
    let output = git.run(repo, &[], ["log", "-1", "--encoding=UTF-8", "--format=%B"])?;
    let text = String::from_utf8_lossy(&output);
    // `%B` ends the message with a line break of its own.
    Ok(Some(text.trim_end().to_owned()))
}

/// The files of the Git folder that tell an operation, and the messages Git
/// prepares; asked of Git in one call, in this order.
const STATE_FILES: [&str; 9] = [
    "rebase-apply/applying",
    "rebase-merge",
    "rebase-apply",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "sequencer/todo",
    "SQUASH_MSG",
    "MERGE_MSG",
];

/// Which operation is in progress and which message Git prepared for the
/// next commit.
pub fn commit_state(git: &Git, repo: &Path) -> Result<CommitState, Error> {
    // Asking Git finds the files of a linked worktree too.
    let args = ["rev-parse"]
        .into_iter()
        .chain(STATE_FILES.iter().flat_map(|file| ["--git-path", file]));
    let output = git.run(repo, &[], args)?;
    let command = "git rev-parse --git-path";
    let text = String::from_utf8(output).map_err(|error| Error::Parse {
        command: command.to_owned(),
        message: "the paths are not UTF-8".to_owned(),
        bytes: error.into_bytes(),
    })?;
    let paths: Vec<_> = text
        .lines()
        .map(|line| repo.join(line.trim_end_matches('\r')))
        .collect();
    if paths.len() != STATE_FILES.len() {
        return Err(Error::Parse {
            command: command.to_owned(),
            message: format!("{} paths for {} files", paths.len(), STATE_FILES.len()),
            bytes: text.into_bytes(),
        });
    }
    let exists = |index: usize| paths[index].exists();
    let operation = if exists(0) {
        Some(Operation::ApplyingPatches)
    } else if exists(1) || exists(2) {
        // A rebase can hold a pick of its own; it wins.
        Some(Operation::Rebase)
    } else if exists(3) {
        Some(Operation::Merge)
    } else if exists(4) {
        Some(Operation::CherryPick)
    } else if exists(5) {
        Some(Operation::Revert)
    } else {
        // Between the commits of a series only its list is left.
        match std::fs::read(&paths[6]) {
            Ok(todo) if todo.starts_with(b"revert") => Some(Operation::Revert),
            Ok(todo) if !todo.is_empty() => Some(Operation::CherryPick),
            _ => None,
        }
    };
    let prepared = match operation {
        Some(Operation::Rebase | Operation::ApplyingPatches) => None,
        _ => {
            // The message of a squash, then what a conflict added: Git joins
            // them the same way.
            let mut raw = Vec::new();
            for path in &paths[7..] {
                if let Ok(content) = std::fs::read(path) {
                    raw.extend_from_slice(&content);
                }
            }
            if raw.is_empty() {
                None
            } else {
                // Git knows the comment character of the configuration.
                let cleaned = git.run_with_input(
                    repo,
                    &[],
                    ["stripspace", "--strip-comments"],
                    raw,
                    &CancelToken::new(),
                )?;
                let cleaned = String::from_utf8_lossy(&cleaned).trim_end().to_owned();
                (!cleaned.is_empty()).then_some(cleaned)
            }
        }
    };
    Ok(CommitState {
        operation,
        prepared,
    })
}

/// Whether the last commit is part of a branch of a remote. Not to be asked
/// in a repository without a commit, where Git rejects the question.
pub fn on_remote_branch(git: &Git, repo: &Path) -> Result<bool, Error> {
    let output = git.run(
        repo,
        &[],
        [
            "for-each-ref",
            "--count=1",
            "--contains",
            "HEAD",
            "refs/remotes",
        ],
    )?;
    Ok(output.iter().any(|byte| !byte.is_ascii_whitespace()))
}
