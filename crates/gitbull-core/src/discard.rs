//! What the File status view offers to discard (spec `discard`, requirements
//! "Discarding whole files" and "Discarding hunks and lines").
//!
//! Only the Unstaged and the Untracked group offer anything: the changes of
//! the Staged group are in the index, and discarding them is unstaging them
//! and then discarding what comes back.

use gitbull_git::changes::ChangeKind;
use gitbull_git::discard::Class;
use gitbull_git::status::{Group, StatusEntry, StatusKind};

use crate::file_status::Parts;

/// Why a file cannot be discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
    /// The file is in conflict: resolving it is another change.
    Conflict,
    /// The entry is a submodule, which has a repository of its own.
    Submodule,
    /// The file is renamed or copied in the working copy: half of it is no
    /// discard Git knows.
    Renamed,
    /// The file was added with `git add --intent-to-add`: its content is only
    /// in the working copy.
    IntentToAdd,
    /// The entry is a folder with a repository of its own.
    NestedRepository,
}

/// What a file of the status offers to discard as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardOffer {
    /// It can be discarded; Git writes a tracked file again from the index,
    /// an untracked file is only moved away.
    Offered(Class),
    Unavailable(Unavailable),
}

/// What `entry` of `group` offers to discard as a whole; `None` where nothing
/// is offered, as for every file of the Staged group.
pub fn discard_offer(group: Group, entry: &StatusEntry) -> Option<DiscardOffer> {
    if group == Group::Staged {
        return None;
    }
    if entry.submodule {
        return Some(DiscardOffer::Unavailable(Unavailable::Submodule));
    }
    Some(match entry.kind {
        StatusKind::Conflicted => DiscardOffer::Unavailable(Unavailable::Conflict),
        StatusKind::Untracked if entry.path.to_string().ends_with('/') => {
            DiscardOffer::Unavailable(Unavailable::NestedRepository)
        }
        StatusKind::Untracked => DiscardOffer::Offered(Class::Untracked),
        StatusKind::Changed(ChangeKind::Renamed | ChangeKind::Copied) => {
            DiscardOffer::Unavailable(Unavailable::Renamed)
        }
        // A file that is only in the working copy but known to the index.
        StatusKind::Changed(ChangeKind::Added) => {
            DiscardOffer::Unavailable(Unavailable::IntentToAdd)
        }
        StatusKind::Changed(
            ChangeKind::Modified | ChangeKind::Deleted | ChangeKind::TypeChanged,
        ) => DiscardOffer::Offered(Class::Tracked),
    })
}

/// What a diff offers to discard in parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartOffer {
    Offered,
    /// As where staging in parts is unavailable, for the same reason.
    Unavailable(Parts),
}

/// What the diff shown of `entry` in `group` offers to discard in parts, given
/// whether it can be staged in parts (`parts`); `None` where nothing is
/// offered: for the Staged group, for an untracked file, and for a diff that
/// shows no lines.
pub fn part_offer(group: Group, entry: &StatusEntry, parts: Option<Parts>) -> Option<PartOffer> {
    if group != Group::Unstaged {
        return None;
    }
    match discard_offer(group, entry)? {
        DiscardOffer::Offered(Class::Tracked) => {}
        _ => return None,
    }
    Some(match parts? {
        Parts::Available => PartOffer::Offered,
        other => PartOffer::Unavailable(other),
    })
}

#[cfg(test)]
mod tests {
    use gitbull_git::path::RepoPath;

    use super::*;

    fn entry(kind: StatusKind, path: &str) -> StatusEntry {
        StatusEntry {
            kind,
            path: RepoPath::new(path),
            old_path: None,
            submodule: false,
        }
    }

    fn changed(kind: ChangeKind) -> StatusEntry {
        entry(StatusKind::Changed(kind), "f.txt")
    }

    #[test]
    fn modified_deleted_type_changed_and_untracked_files_are_offered() {
        for kind in [
            ChangeKind::Modified,
            ChangeKind::Deleted,
            ChangeKind::TypeChanged,
        ] {
            assert_eq!(
                discard_offer(Group::Unstaged, &changed(kind)),
                Some(DiscardOffer::Offered(Class::Tracked)),
                "{kind:?}"
            );
        }
        assert_eq!(
            discard_offer(Group::Untracked, &entry(StatusKind::Untracked, "new.txt")),
            Some(DiscardOffer::Offered(Class::Untracked))
        );
    }

    #[test]
    fn what_cannot_be_discarded_says_why() {
        let unstaged = |entry: &StatusEntry| discard_offer(Group::Unstaged, entry);
        assert_eq!(
            unstaged(&entry(StatusKind::Conflicted, "f.txt")),
            Some(DiscardOffer::Unavailable(Unavailable::Conflict))
        );
        let mut submodule = changed(ChangeKind::Modified);
        submodule.submodule = true;
        assert_eq!(
            unstaged(&submodule),
            Some(DiscardOffer::Unavailable(Unavailable::Submodule))
        );
        for kind in [ChangeKind::Renamed, ChangeKind::Copied] {
            assert_eq!(
                unstaged(&changed(kind)),
                Some(DiscardOffer::Unavailable(Unavailable::Renamed))
            );
        }
        assert_eq!(
            unstaged(&changed(ChangeKind::Added)),
            Some(DiscardOffer::Unavailable(Unavailable::IntentToAdd))
        );
        assert_eq!(
            discard_offer(Group::Untracked, &entry(StatusKind::Untracked, "lib/")),
            Some(DiscardOffer::Unavailable(Unavailable::NestedRepository))
        );
    }

    #[test]
    fn the_staged_group_offers_nothing() {
        for kind in [ChangeKind::Modified, ChangeKind::Added, ChangeKind::Deleted] {
            assert_eq!(discard_offer(Group::Staged, &changed(kind)), None);
        }
        assert_eq!(
            part_offer(
                Group::Staged,
                &changed(ChangeKind::Modified),
                Some(Parts::Available)
            ),
            None
        );
    }

    #[test]
    fn parts_are_offered_for_a_tracked_file_where_staging_in_parts_is() {
        let file = changed(ChangeKind::Modified);
        assert_eq!(
            part_offer(Group::Unstaged, &file, Some(Parts::Available)),
            Some(PartOffer::Offered)
        );
        for parts in [
            Parts::Conflicted,
            Parts::Truncated,
            Parts::Filtered,
            Parts::WholeOnly,
            Parts::Reading,
        ] {
            assert_eq!(
                part_offer(Group::Unstaged, &file, Some(parts)),
                Some(PartOffer::Unavailable(parts))
            );
        }
        // A diff without lines offers nothing.
        assert_eq!(part_offer(Group::Unstaged, &file, None), None);
    }

    #[test]
    fn parts_are_not_offered_for_an_untracked_file_or_where_a_file_is_not_offered() {
        let new = entry(StatusKind::Untracked, "new.txt");
        assert_eq!(
            part_offer(Group::Untracked, &new, Some(Parts::Available)),
            None
        );
        for kind in [ChangeKind::Added, ChangeKind::Renamed] {
            assert_eq!(
                part_offer(Group::Unstaged, &changed(kind), Some(Parts::Available)),
                None
            );
        }
    }
}
