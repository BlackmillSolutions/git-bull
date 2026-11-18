//! The words of the write actions that discard, restore and clear, and of
//! their dialogs (spec `discard`).

use fluent_bundle::FluentArgs;
use gitbull_core::session::{Action, ActionDialog, JournalWhy};
use gitbull_git::discard::DiscardRefusal;
use gitbull_git::journal::{Left, PathRefusal};

use crate::i18n::{Msg, Translations};

fn with(texts: &Translations, msg: Msg, key: &str, value: impl Into<String>) -> String {
    let mut args = FluentArgs::new();
    args.set(key, value.into());
    texts.text_with(msg, Some(&args))
}

/// What a path refusal means, such as "it lies inside the Git folder".
pub(crate) fn path_reason(texts: &Translations, why: PathRefusal) -> String {
    texts.text(match why {
        PathRefusal::Odd => Msg::PathOdd,
        PathRefusal::Absolute => Msg::PathAbsolute,
        PathRefusal::DotGit => Msg::PathDotGit,
        PathRefusal::NestedRepository => Msg::PathNested,
        PathRefusal::FolderAtPath => Msg::PathFolder,
        PathRefusal::ThroughLink => Msg::PathLink,
        PathRefusal::ParentNotFolder => Msg::PathParentFile,
    })
}

/// What a write action that discards, restores or clears is called, such as
/// "Discarding 3 files". Empty for any other action.
pub(crate) fn action_text(texts: &Translations, action: &Action) -> String {
    match action {
        Action::Discard { files } => {
            let mut args = FluentArgs::new();
            args.set("count", *files as i64);
            texts.text_with(Msg::ActionDiscard, Some(&args))
        }
        Action::DiscardPart { path } => with(texts, Msg::ActionDiscardPart, "path", path.clone()),
        Action::Restore { again: false } => texts.text(Msg::ActionRestore),
        Action::Restore { again: true } => texts.text(Msg::ActionDiscardAgain),
        Action::PutBack => texts.text(Msg::ActionPutBack),
        Action::ClearDiscards => texts.text(Msg::ActionClearDiscards),
        _ => String::new(),
    }
}

/// The title of the dialog of a discard, restore or clear that failed. Empty
/// for any other action.
pub(crate) fn failed_title(texts: &Translations, action: &Action) -> String {
    match action {
        Action::Discard { .. } => texts.text(Msg::DiscardFailedTitle),
        Action::DiscardPart { path } => {
            with(texts, Msg::DiscardPartFailedTitle, "path", path.clone())
        }
        Action::Restore { again: false } => texts.text(Msg::RestoreFailedTitle),
        Action::Restore { again: true } => texts.text(Msg::DiscardAgainFailedTitle),
        Action::PutBack => texts.text(Msg::PutBackFailedTitle),
        Action::ClearDiscards => texts.text(Msg::ClearDiscardsFailedTitle),
        _ => String::new(),
    }
}

/// What a dialog of these actions says: its title, a sentence, the lines
/// below it, and whether they may be copied. `None` for any other dialog.
pub(crate) fn dialog_texts(
    texts: &Translations,
    dialog: &ActionDialog,
) -> Option<(String, String, String, bool)> {
    Some(match dialog {
        ActionDialog::DiscardRefused { why, path } => {
            let file = path.clone().unwrap_or_default();
            match why {
                DiscardRefusal::FilesChanged => plain(texts, Msg::DiscardRefusedFilesChanged),
                DiscardRefusal::PathBack(path) => (
                    texts.text(Msg::DiscardRefusedTitle),
                    with(texts, Msg::DiscardRefusedPathBack, "path", path.to_string()),
                    String::new(),
                    false,
                ),
                DiscardRefusal::Journal(message) => (
                    texts.text(Msg::DiscardRefusedTitle),
                    with(
                        texts,
                        Msg::DiscardRefusedJournal,
                        "message",
                        message.clone(),
                    ),
                    String::new(),
                    false,
                ),
                DiscardRefusal::DiffOutOfDate => (
                    with(texts, Msg::DiffOutOfDateTitle, "path", file),
                    texts.text(Msg::DiscardRefusedDiffBody),
                    String::new(),
                    false,
                ),
                DiscardRefusal::EndOfFileSplit => (
                    with(texts, Msg::DiscardRefusedSplitTitle, "path", file),
                    texts.text(Msg::DiscardRefusedSplitBody),
                    String::new(),
                    false,
                ),
                DiscardRefusal::LineEndings => (
                    with(texts, Msg::DiscardRefusedLineEndingsTitle, "path", file),
                    texts.text(Msg::DiscardRefusedLineEndingsBody),
                    String::new(),
                    false,
                ),
                DiscardRefusal::NotTracked => plain(texts, Msg::DiscardRefusedNotTracked),
                DiscardRefusal::Path(why) => (
                    texts.text(Msg::DiscardRefusedTitle),
                    with(
                        texts,
                        Msg::DiscardRefusedPath,
                        "reason",
                        path_reason(texts, *why),
                    ),
                    String::new(),
                    false,
                ),
            }
        }
        ActionDialog::DiscardLeftOut { files } => (
            texts.text(Msg::DiscardLeftOutTitle),
            texts.text(Msg::DiscardLeftOutBody),
            files
                .iter()
                .map(|(path, why)| format!("{path}: {}", path_reason(texts, *why)))
                .collect::<Vec<_>>()
                .join("\n"),
            false,
        ),
        ActionDialog::DiscardStuck { message, files } => (
            texts.text(Msg::DiscardStuckTitle),
            texts.text(Msg::DiscardStuckBody),
            std::iter::once(message.clone())
                .chain(files.iter().cloned())
                .collect::<Vec<_>>()
                .join("\n"),
            true,
        ),
        ActionDialog::JournalRefused { why } => journal_texts(texts, why),
        _ => return None,
    })
}

/// A dialog that is a title and one sentence.
fn plain(texts: &Translations, body: Msg) -> (String, String, String, bool) {
    (
        texts.text(Msg::DiscardRefusedTitle),
        texts.text(body),
        String::new(),
        false,
    )
}

fn journal_texts(texts: &Translations, why: &JournalWhy) -> (String, String, String, bool) {
    let title = texts.text(Msg::JournalRefusedTitle);
    let listed = |body: Msg, lines: String| (title.clone(), texts.text(body), lines, false);
    match why {
        JournalWhy::Gone => (
            title.clone(),
            texts.text(Msg::JournalGone),
            String::new(),
            false,
        ),
        JournalWhy::Changed(_) => (
            title.clone(),
            texts.text(Msg::JournalChanged),
            String::new(),
            false,
        ),
        JournalWhy::Busy => (
            title.clone(),
            texts.text(Msg::JournalBusy),
            String::new(),
            false,
        ),
        JournalWhy::NoLocks(message) => (
            title.clone(),
            with(texts, Msg::JournalNoLocks, "message", message.clone()),
            String::new(),
            false,
        ),
        JournalWhy::GitRuns(pid) => {
            let mut args = FluentArgs::new();
            args.set("pid", i64::from(*pid));
            (
                title.clone(),
                texts.text_with(Msg::JournalGitRuns, Some(&args)),
                String::new(),
                false,
            )
        }
        JournalWhy::Blocked { path, why } => {
            let mut args = FluentArgs::new();
            args.set("path", path.clone());
            args.set("reason", path_reason(texts, *why));
            (
                title.clone(),
                texts.text_with(Msg::JournalBlocked, Some(&args)),
                String::new(),
                false,
            )
        }
        JournalWhy::LeftAlone(files) => listed(
            Msg::JournalLeftAloneBody,
            files
                .iter()
                .map(|(path, left)| {
                    let reason = match left {
                        Left::SlotMissing => Msg::SlotMissing,
                        Left::Foreign => Msg::SlotForeign,
                    };
                    format!("{path}: {}", texts.text(reason))
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        JournalWhy::PathsTaken(files) => listed(Msg::JournalPathsTakenBody, files.join("\n")),
    }
}
