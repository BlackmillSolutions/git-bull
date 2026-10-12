//! What the user writes in the commit form, and what the form offers (spec
//! `commit`). The draft belongs to the session of its tab, so that it
//! survives a change of the view; the rules here need no session.

use gitbull_git::commit::Operation;
use gitbull_git::status::{StatusKind, WorkingStatus};

/// Where a message came from that git-bull put into the fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    /// The last commit, for "Amend last commit".
    Amend,
    /// The message Git prepared for the next commit.
    Prepared,
}

/// The message of a commit as it is being written, with the two boxes of the
/// form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitDraft {
    summary: String,
    description: String,
    amend: bool,
    without_hooks: bool,
    /// The message git-bull filled in, and why: the fields are replaced or
    /// emptied again only while they still hold it unchanged.
    filled: Option<(Source, String)>,
    /// The prepared message that was filled in last. It is not filled in
    /// again, so that one the user changed or removed does not come back.
    prepared_seen: Option<String>,
}

/// The summary and the description of `message`: its first line, and the
/// rest after the empty line. A second line that is not empty is the first
/// of the description.
pub fn split(message: &str) -> (String, String) {
    let message = message.trim_end();
    match message.split_once('\n') {
        Some((summary, rest)) => (
            summary.trim_end().to_owned(),
            rest.strip_prefix('\n').unwrap_or(rest).to_owned(),
        ),
        None => (message.to_owned(), String::new()),
    }
}

impl CommitDraft {
    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn amend(&self) -> bool {
        self.amend
    }

    pub fn without_hooks(&self) -> bool {
        self.without_hooks
    }

    pub fn set_summary(&mut self, summary: String) {
        self.summary = summary;
    }

    pub fn set_description(&mut self, description: String) {
        self.description = description;
    }

    pub fn set_without_hooks(&mut self, without: bool) {
        self.without_hooks = without;
    }

    /// The message of the commit: the summary, and when there is a
    /// description, an empty line and the description.
    pub fn message(&self) -> String {
        // What was filled in and not touched is the message as it was:
        // splitting it and joining it again would put an empty line after a
        // first line that had none.
        if let Some((_, text)) = &self.filled
            && self.holds(text)
        {
            return text.trim_end().to_owned();
        }
        let summary = self.summary.trim_end();
        if self.description.trim().is_empty() {
            summary.to_owned()
        } else {
            format!("{summary}\n\n{}", self.description.trim_end())
        }
    }

    /// Whether the summary holds nothing but blanks.
    pub fn has_no_summary(&self) -> bool {
        self.summary.trim().is_empty()
    }

    /// Whether nothing is written.
    fn is_blank(&self) -> bool {
        self.has_no_summary() && self.description.trim().is_empty()
    }

    /// Whether the fields hold `message`, as it would be filled in.
    fn holds(&self, message: &str) -> bool {
        let (summary, description) = split(message);
        self.summary.trim_end() == summary && self.description.trim_end() == description
    }

    /// Whether the fields still hold what was filled in from `source`,
    /// unchanged.
    fn still_filled_from(&self, source: Source) -> bool {
        matches!(&self.filled, Some((from, text)) if *from == source && self.holds(text))
    }

    fn fill(&mut self, source: Source, message: &str) {
        // The fields hold lines that end as the form writes them.
        let message = &message.replace("\r\n", "\n");
        let (summary, description) = split(message);
        self.summary = summary;
        self.description = description;
        self.filled = Some((source, message.to_owned()));
    }

    fn empty_fields(&mut self) {
        self.summary.clear();
        self.description.clear();
        self.filled = None;
    }

    /// "Amend last commit" was checked. The message of the last commit
    /// follows with [`CommitDraft::last_message_read`].
    pub(crate) fn check_amend(&mut self) {
        self.amend = true;
    }

    /// "Amend last commit" was unchecked, by the user or because the state
    /// of the repository closes it: the fields are emptied when they still
    /// hold the message of the last commit unchanged.
    pub(crate) fn uncheck_amend(&mut self) {
        self.amend = false;
        if self.still_filled_from(Source::Amend) {
            self.empty_fields();
        } else if matches!(self.filled, Some((Source::Amend, _))) {
            self.filled = None;
        }
    }

    /// The message of the last commit was read while "Amend last commit" is
    /// checked. It fills fields that are empty at this moment, and replaces
    /// the message of another last commit that is still there unchanged.
    pub(crate) fn last_message_read(&mut self, message: Option<&str>) {
        if !self.amend {
            return;
        }
        let Some(message) = message else {
            return;
        };
        if self.is_blank() || self.still_filled_from(Source::Amend) {
            self.fill(Source::Amend, message);
        }
    }

    /// The message Git prepared for the next commit was read, or none.
    pub(crate) fn prepared_read(&mut self, prepared: Option<&str>) {
        if prepared == self.prepared_seen.as_deref() {
            return;
        }
        // Gone, or another one: what was filled in and not touched goes.
        if self.still_filled_from(Source::Prepared) {
            self.empty_fields();
        }
        match prepared {
            None => self.prepared_seen = None,
            Some(text) if self.is_blank() => {
                self.fill(Source::Prepared, text);
                self.prepared_seen = Some(text.to_owned());
            }
            // Not filled, so not seen: it is offered once the fields are
            // empty.
            Some(_) => {}
        }
    }

    /// A commit took place: the form is empty again. The prepared message of
    /// before stays seen until Git has removed it.
    pub(crate) fn committed(&mut self) {
        *self = CommitDraft {
            prepared_seen: self.prepared_seen.take(),
            ..CommitDraft::default()
        };
    }
}

/// What the button of the commit form does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitOffer {
    /// Commit the staged changes, of this many files.
    Commit {
        files: usize,
    },
    /// Conclude the merge with what is staged, which may be nothing.
    ConcludeMerge {
        files: usize,
    },
    /// Stage every change and commit.
    StageAllAndCommit,
    /// Replace the last commit.
    Amend,
    Unavailable(CommitUnavailable),
}

/// Why the button of the commit form does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitUnavailable {
    /// The status is not known yet.
    StatusUnknown,
    /// A write action runs in the tab.
    Busy,
    /// A rebase is in progress and has to be continued outside git-bull.
    Rebase,
    /// A series of patches is being applied with `git am`.
    ApplyingPatches,
    /// Files are in conflict.
    Conflicts,
    /// The summary is empty.
    NoSummary,
    /// Nothing is staged and nothing is changed.
    NothingToCommit,
}

/// Why "Amend last commit" cannot be checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmendUnavailable {
    /// The repository has no commit.
    NoCommit,
    /// An operation is in progress.
    InProgress(Operation),
}

/// What the form offers for `draft` in a repository whose status is `status`,
/// `None` while it is not known, where `operation` is in progress and a
/// write action runs or not.
pub fn offer(
    draft: &CommitDraft,
    status: Option<&WorkingStatus>,
    operation: Option<Operation>,
    busy: bool,
) -> CommitOffer {
    use CommitUnavailable as No;
    let Some(status) = status else {
        return CommitOffer::Unavailable(No::StatusUnknown);
    };
    if busy {
        return CommitOffer::Unavailable(No::Busy);
    }
    match operation {
        Some(Operation::Rebase) => return CommitOffer::Unavailable(No::Rebase),
        Some(Operation::ApplyingPatches) => return CommitOffer::Unavailable(No::ApplyingPatches),
        _ => {}
    }
    let conflict = status
        .unstaged
        .iter()
        .any(|entry| entry.kind == StatusKind::Conflicted);
    if conflict {
        return CommitOffer::Unavailable(No::Conflicts);
    }
    let files = status.staged.len();
    let offer = if draft.amend {
        CommitOffer::Amend
    } else if operation == Some(Operation::Merge) {
        // Never by staging everything: files that lie in the working copy
        // are staged first, by the user.
        CommitOffer::ConcludeMerge { files }
    } else if files > 0 {
        CommitOffer::Commit { files }
    } else if !status.unstaged.is_empty() || !status.untracked.is_empty() {
        CommitOffer::StageAllAndCommit
    } else {
        return CommitOffer::Unavailable(No::NothingToCommit);
    };
    if draft.has_no_summary() {
        return CommitOffer::Unavailable(No::NoSummary);
    }
    offer
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::changes::ChangeKind;
    use gitbull_git::status::StatusEntry;

    fn entry(kind: StatusKind, path: &str) -> StatusEntry {
        StatusEntry {
            kind,
            path: path.into(),
            old_path: None,
            submodule: false,
        }
    }

    fn modified(path: &str) -> StatusEntry {
        entry(StatusKind::Changed(ChangeKind::Modified), path)
    }

    fn written(summary: &str) -> CommitDraft {
        let mut draft = CommitDraft::default();
        draft.set_summary(summary.to_owned());
        draft
    }

    fn fields(draft: &CommitDraft) -> (&str, &str) {
        (draft.summary(), draft.description())
    }

    #[test]
    fn the_message_is_the_summary_and_the_description_after_an_empty_line() {
        let mut draft = written("Add the form  ");
        assert_eq!(draft.message(), "Add the form");
        draft.set_description("It sits below the list.\n\n".to_owned());
        assert_eq!(draft.message(), "Add the form\n\nIt sits below the list.");
        draft.set_description("  \n".to_owned());
        assert_eq!(draft.message(), "Add the form");
    }

    #[test]
    fn a_filled_message_that_is_not_touched_is_sent_as_it_was() {
        let mut draft = CommitDraft::default();
        draft.check_amend();
        // No empty line after the first: it must not gain one.
        draft.last_message_read(Some("Subject\nsecond line\n"));
        assert_eq!(fields(&draft), ("Subject", "second line"));
        assert_eq!(draft.message(), "Subject\nsecond line");
        // Touched, it is the summary and the description.
        draft.set_summary("Subject, edited".to_owned());
        assert_eq!(draft.message(), "Subject, edited\n\nsecond line");

        // Line breaks of Windows do not get into the fields.
        let mut draft = CommitDraft::default();
        draft.check_amend();
        draft.last_message_read(Some("Subject\r\n\r\nBody\r\nmore\r\n"));
        assert_eq!(fields(&draft), ("Subject", "Body\nmore"));
        // And it still counts as untouched.
        draft.uncheck_amend();
        assert_eq!(fields(&draft), ("", ""));
    }

    #[test]
    fn a_message_is_split_into_summary_and_description() {
        assert_eq!(split("One"), ("One".to_owned(), String::new()));
        assert_eq!(
            split("One\n\nTwo\n\nThree\n"),
            ("One".to_owned(), "Two\n\nThree".to_owned())
        );
        // A second line that is not empty is the first of the description.
        assert_eq!(
            split("One\nTwo\nThree"),
            ("One".to_owned(), "Two\nThree".to_owned())
        );
    }

    #[test]
    fn the_offer_follows_the_state_in_its_order() {
        use CommitOffer::Unavailable;
        use CommitUnavailable as No;
        let staged = WorkingStatus {
            staged: vec![modified("a"), modified("b")],
            unstaged: vec![modified("c")],
            ..WorkingStatus::default()
        };
        let unstaged = WorkingStatus {
            untracked: vec![entry(StatusKind::Untracked, "new")],
            ..WorkingStatus::default()
        };
        let conflict = WorkingStatus {
            staged: vec![modified("a")],
            unstaged: vec![entry(StatusKind::Conflicted, "c")],
            ..WorkingStatus::default()
        };
        let clean = WorkingStatus::default();
        let draft = written("Summary");

        assert_eq!(
            offer(&draft, None, None, false),
            Unavailable(No::StatusUnknown)
        );
        assert_eq!(
            offer(&draft, Some(&staged), None, true),
            Unavailable(No::Busy)
        );
        // A rebase and a series of patches come before a conflict.
        assert_eq!(
            offer(&draft, Some(&conflict), Some(Operation::Rebase), false),
            Unavailable(No::Rebase)
        );
        assert_eq!(
            offer(
                &draft,
                Some(&conflict),
                Some(Operation::ApplyingPatches),
                false
            ),
            Unavailable(No::ApplyingPatches)
        );
        assert_eq!(
            offer(&draft, Some(&conflict), Some(Operation::Merge), false),
            Unavailable(No::Conflicts)
        );
        assert_eq!(
            offer(&draft, Some(&staged), None, false),
            CommitOffer::Commit { files: 2 }
        );
        assert_eq!(
            offer(&draft, Some(&unstaged), None, false),
            CommitOffer::StageAllAndCommit
        );
        assert_eq!(
            offer(&draft, Some(&clean), None, false),
            Unavailable(No::NothingToCommit)
        );
        // A cherry-pick changes no offer: one that became empty has nothing.
        assert_eq!(
            offer(&draft, Some(&clean), Some(Operation::CherryPick), false),
            Unavailable(No::NothingToCommit)
        );
        assert_eq!(
            offer(&draft, Some(&staged), Some(Operation::CherryPick), false),
            CommitOffer::Commit { files: 2 }
        );
        // Without a summary an offer is none; a state that offers nothing
        // keeps its own reason.
        let blank = written("  ");
        assert_eq!(
            offer(&blank, Some(&staged), None, false),
            Unavailable(No::NoSummary)
        );
        assert_eq!(
            offer(&blank, Some(&clean), None, false),
            Unavailable(No::NothingToCommit)
        );
        assert_eq!(
            offer(&blank, Some(&conflict), None, false),
            Unavailable(No::Conflicts)
        );
    }

    #[test]
    fn a_merge_is_concluded_with_what_is_staged_and_never_by_staging_everything() {
        let merge = Some(Operation::Merge);
        let draft = written("Merge");
        let staged = WorkingStatus {
            staged: vec![modified("a")],
            ..WorkingStatus::default()
        };
        assert_eq!(
            offer(&draft, Some(&staged), merge, false),
            CommitOffer::ConcludeMerge { files: 1 }
        );
        assert_eq!(
            offer(&draft, Some(&WorkingStatus::default()), merge, false),
            CommitOffer::ConcludeMerge { files: 0 }
        );
        let beside = WorkingStatus {
            untracked: vec![entry(StatusKind::Untracked, "notes.txt")],
            ..WorkingStatus::default()
        };
        assert_eq!(
            offer(&draft, Some(&beside), merge, false),
            CommitOffer::ConcludeMerge { files: 0 }
        );
    }

    #[test]
    fn amend_is_offered_with_nothing_staged_and_takes_the_place_of_staging_everything() {
        let mut draft = written("Summary");
        draft.check_amend();
        assert_eq!(
            offer(&draft, Some(&WorkingStatus::default()), None, false),
            CommitOffer::Amend
        );
        let unstaged = WorkingStatus {
            unstaged: vec![modified("c")],
            ..WorkingStatus::default()
        };
        assert_eq!(
            offer(&draft, Some(&unstaged), None, false),
            CommitOffer::Amend
        );
    }

    #[test]
    fn checking_amend_fills_empty_fields_and_keeps_what_was_typed() {
        let mut draft = CommitDraft::default();
        draft.check_amend();
        draft.last_message_read(Some("Last\n\nIts body.\n"));
        assert_eq!(fields(&draft), ("Last", "Its body."));

        // Typed before the box was checked.
        let mut draft = written("Mine");
        draft.check_amend();
        draft.last_message_read(Some("Last"));
        assert_eq!(fields(&draft), ("Mine", ""));

        // Typed before the message arrived.
        let mut draft = CommitDraft::default();
        draft.check_amend();
        draft.set_description("Typed meanwhile".to_owned());
        draft.last_message_read(Some("Last"));
        assert_eq!(fields(&draft), ("", "Typed meanwhile"));

        // No commit, or the box was unchecked meanwhile.
        let mut draft = CommitDraft::default();
        draft.check_amend();
        draft.last_message_read(None);
        assert_eq!(fields(&draft), ("", ""));
        draft.uncheck_amend();
        draft.last_message_read(Some("Last"));
        assert_eq!(fields(&draft), ("", ""));
    }

    #[test]
    fn another_last_commit_replaces_a_filled_message_that_is_unchanged() {
        let mut draft = CommitDraft::default();
        draft.check_amend();
        draft.last_message_read(Some("First"));
        draft.last_message_read(Some("Second"));
        assert_eq!(fields(&draft), ("Second", ""));
        draft.set_summary("Second, edited".to_owned());
        draft.last_message_read(Some("Third"));
        assert_eq!(fields(&draft), ("Second, edited", ""));
    }

    #[test]
    fn unchecking_amend_empties_only_an_unchanged_message() {
        let mut draft = CommitDraft::default();
        draft.check_amend();
        draft.last_message_read(Some("Last\n\nBody"));
        draft.uncheck_amend();
        assert_eq!(fields(&draft), ("", ""));
        assert!(!draft.amend());

        draft.check_amend();
        draft.last_message_read(Some("Last\n\nBody"));
        draft.set_summary("Last, edited".to_owned());
        draft.uncheck_amend();
        assert_eq!(fields(&draft), ("Last, edited", "Body"));
    }

    #[test]
    fn a_prepared_message_fills_empty_fields_and_does_not_come_back() {
        let merge = Some("Merge branch 'topic'");
        let mut draft = CommitDraft::default();
        draft.prepared_read(merge);
        assert_eq!(fields(&draft), ("Merge branch 'topic'", ""));
        // The user empties the summary; the next reads leave it empty.
        draft.set_summary(String::new());
        draft.prepared_read(merge);
        draft.prepared_read(merge);
        assert_eq!(fields(&draft), ("", ""));
    }

    #[test]
    fn a_prepared_message_stays_while_git_has_it_whatever_the_operation() {
        // After a merge that squashes there is no operation, and the message
        // stays over further reads.
        let squash = Some("Squashed commit of the following:\n\ncommit 1");
        let mut draft = CommitDraft::default();
        draft.prepared_read(squash);
        draft.prepared_read(squash);
        assert_eq!(draft.summary(), "Squashed commit of the following:");
    }

    #[test]
    fn a_prepared_message_that_is_gone_takes_the_untouched_fields_with_it() {
        let mut draft = CommitDraft::default();
        draft.prepared_read(Some("Merge branch 'topic'"));
        draft.prepared_read(None);
        assert_eq!(fields(&draft), ("", ""));
        // The same merge begun again is offered again.
        draft.prepared_read(Some("Merge branch 'topic'"));
        assert_eq!(draft.summary(), "Merge branch 'topic'");
        // Edited, it stays when the merge is gone.
        draft.set_summary("Merge topic by hand".to_owned());
        draft.prepared_read(None);
        assert_eq!(draft.summary(), "Merge topic by hand");
        // Another one replaces an untouched one.
        let mut draft = CommitDraft::default();
        draft.prepared_read(Some("Merge branch 'a'"));
        draft.prepared_read(Some("Merge branch 'b'"));
        assert_eq!(draft.summary(), "Merge branch 'b'");
    }

    #[test]
    fn a_prepared_message_waits_for_fields_that_are_not_empty() {
        let mut draft = written("Mine");
        draft.prepared_read(Some("Merge branch 'topic'"));
        assert_eq!(draft.summary(), "Mine");
        // Not filled, so not seen: once the fields are empty, it is offered.
        draft.set_summary(String::new());
        draft.prepared_read(Some("Merge branch 'topic'"));
        assert_eq!(draft.summary(), "Merge branch 'topic'");
    }

    #[test]
    fn a_merge_that_begins_while_amend_is_checked_leaves_its_message() {
        let mut draft = CommitDraft::default();
        draft.check_amend();
        draft.last_message_read(Some("Last"));
        // The state closes the amend first, then the prepared message fills.
        draft.uncheck_amend();
        draft.prepared_read(Some("Merge branch 'topic'"));
        assert!(!draft.amend());
        assert_eq!(draft.summary(), "Merge branch 'topic'");
    }

    #[test]
    fn a_commit_empties_the_form_and_keeps_the_prepared_message_seen() {
        let mut draft = CommitDraft::default();
        draft.prepared_read(Some("Merge branch 'topic'"));
        draft.set_without_hooks(true);
        draft.committed();
        assert_eq!(fields(&draft), ("", ""));
        assert!(!draft.without_hooks() && !draft.amend());
        // Git has not removed the file yet when the state is read at once.
        draft.prepared_read(Some("Merge branch 'topic'"));
        assert_eq!(fields(&draft), ("", ""));
        draft.prepared_read(None);
        draft.prepared_read(Some("Merge branch 'topic'"));
        assert_eq!(draft.summary(), "Merge branch 'topic'");
    }
}
