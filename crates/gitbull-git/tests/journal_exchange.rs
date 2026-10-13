//! The exchange, the resuming and the retention of the journal (spec
//! `discard`, requirements "Restoring", "An interrupted discard or restore" and
//! "Retention of the journal"; design decisions 6 and 8). Plain files in
//! temporary folders: no Git is needed here.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gitbull_git::journal::{
    Entry, ExchangeError, Failpoints, FileId, FileKind, FileRecord, Held, Ident, Interrupt,
    Journal, Kind, Left, Limits, Listed, Manifest, NoFailpoints, Side, State,
};
use gitbull_git::path::RepoPath;

/// Ends an operation after the step with this name, as if the process had.
struct EndAfter(&'static str);

impl Failpoints for EndAfter {
    fn after(&self, step: &str) -> Result<(), Interrupt> {
        if step == self.0 {
            Err(Interrupt)
        } else {
            Ok(())
        }
    }
}

fn record(slot: u32, held: Held, path: &str) -> FileRecord {
    FileRecord {
        slot,
        kind: FileKind::Untracked,
        held,
        path: RepoPath::new(path),
    }
}

fn manifest() -> Manifest {
    Manifest {
        changed: 1_760_270_000,
        size: 1,
        kind: Kind::Files,
        state: State::Discarded,
        git: None,
        files: Vec::new(),
        idents: Vec::new(),
    }
}

/// A working copy and a journal in a folder of its own.
struct Place {
    work: tempfile::TempDir,
    journal: Journal,
    _journal_dir: tempfile::TempDir,
}

impl Place {
    fn new() -> Place {
        let work = tempfile::tempdir().unwrap();
        let journal_dir = tempfile::tempdir().unwrap();
        let journal = Journal::at(journal_dir.path().join("gitbull/discards"));
        Place {
            work,
            journal,
            _journal_dir: journal_dir,
        }
    }

    fn root(&self) -> &Path {
        self.work.path()
    }

    fn path(&self, name: &str) -> PathBuf {
        self.work.path().join(name)
    }

    fn lock(&self) -> gitbull_git::journal::LockGuard {
        self.journal.lock("0-0", Duration::from_secs(2)).unwrap()
    }

    /// An entry that has the files `files` in the state `state`.
    fn entry(&self, state: State, files: &[(&str, Held)]) -> Entry {
        let records = files
            .iter()
            .enumerate()
            .map(|(slot, (name, held))| record(slot as u32, *held, name))
            .collect();
        self.journal
            .create(&Manifest {
                state,
                files: records,
                ..manifest()
            })
            .unwrap()
    }
}

fn text_of(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn reload(place: &Place, entry: &Entry) -> Entry {
    place
        .journal
        .list()
        .unwrap()
        .into_iter()
        .find_map(|item| match item {
            Listed::Entry { entry: e, .. } if e.id() == entry.id() => Some(e),
            _ => None,
        })
        .expect("the entry is listed")
}

#[test]
fn the_exchange_swaps_the_slots_and_the_paths() {
    let place = Place::new();
    let entry = place.entry(
        State::Discarded,
        &[
            ("kept.txt", Held::File),
            ("deleted.txt", Held::File),
            ("empty.txt", Held::None),
        ],
    );
    // kept.txt: the user has a newer file; deleted.txt: nothing at the path;
    // empty.txt: a file at the path and nothing in the slot.
    fs::write(place.path("kept.txt"), "now").unwrap();
    fs::write(entry.slot(0), "discarded").unwrap();
    fs::write(entry.slot(1), "was there").unwrap();
    fs::write(place.path("empty.txt"), "created later").unwrap();
    let lock = place.lock();

    let end = place
        .journal
        .exchange(&lock, &entry, place.root(), State::Discarded, &NoFailpoints)
        .unwrap();
    assert!(end.complete && end.left.is_empty());
    assert_eq!(end.entry.manifest().state, State::Restored);
    assert_eq!(
        text_of(&place.path("kept.txt")).as_deref(),
        Some("discarded")
    );
    assert_eq!(text_of(&entry.slot(0)).as_deref(), Some("now"));
    assert_eq!(
        text_of(&place.path("deleted.txt")).as_deref(),
        Some("was there")
    );
    assert!(!entry.slot(1).exists());
    assert!(!place.path("empty.txt").exists());
    assert_eq!(text_of(&entry.slot(2)).as_deref(), Some("created later"));
    // What the slots hold is told by the manifest again.
    let held: Vec<_> = reload(&place, &entry)
        .manifest()
        .files
        .iter()
        .map(|file| file.held)
        .collect();
    assert_eq!(held, [Held::File, Held::None, Held::File]);

    // Twice over, it is the start again.
    let back = place
        .journal
        .exchange(
            &lock,
            &end.entry,
            place.root(),
            State::Restored,
            &NoFailpoints,
        )
        .unwrap();
    assert_eq!(back.entry.manifest().state, State::Discarded);
    assert_eq!(text_of(&place.path("kept.txt")).as_deref(), Some("now"));
    assert_eq!(text_of(&entry.slot(0)).as_deref(), Some("discarded"));
    assert_eq!(text_of(&place.path("deleted.txt")), None);
    assert_eq!(text_of(&entry.slot(1)).as_deref(), Some("was there"));
    assert_eq!(
        text_of(&place.path("empty.txt")).as_deref(),
        Some("created later")
    );
    assert!(!entry.slot(2).exists());
}

#[test]
fn the_exchange_keeps_mode_times_and_links() {
    let place = Place::new();
    let entry = place.entry(
        State::Discarded,
        &[("a.sh", Held::File), ("link", Held::Link)],
    );
    fs::write(entry.slot(0), "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        fs::set_permissions(entry.slot(0), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("elsewhere", entry.slot(1)).unwrap();
    }
    #[cfg(not(unix))]
    fs::write(entry.slot(1), "x").unwrap();
    let before = fs::metadata(entry.slot(0)).unwrap();
    let lock = place.lock();
    place
        .journal
        .exchange(&lock, &entry, place.root(), State::Discarded, &NoFailpoints)
        .unwrap();
    let after = fs::metadata(place.path("a.sh")).unwrap();
    assert_eq!(before.permissions(), after.permissions());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    #[cfg(unix)]
    assert_eq!(
        fs::read_link(place.path("link")).unwrap(),
        PathBuf::from("elsewhere")
    );
}

#[test]
fn the_exchange_makes_missing_folders_and_refuses_a_folder_in_the_way() {
    let place = Place::new();
    let entry = place.entry(
        State::Discarded,
        &[
            ("deep/er/file.txt", Held::File),
            ("blocked.txt", Held::File),
        ],
    );
    fs::write(entry.slot(0), "a").unwrap();
    fs::write(entry.slot(1), "b").unwrap();
    let lock = place.lock();

    // A folder at the path of the second refuses the whole entry.
    fs::create_dir(place.path("blocked.txt")).unwrap();
    let refused = place
        .journal
        .exchange(&lock, &entry, place.root(), State::Discarded, &NoFailpoints)
        .unwrap_err();
    assert!(
        matches!(refused, ExchangeError::Blocked { .. }),
        "{refused:?}"
    );
    assert!(!place.path("deep").exists(), "nothing was moved");
    assert!(entry.slot(0).exists() && entry.slot(1).exists());
    assert_eq!(reload(&place, &entry).manifest().state, State::Discarded);

    // Without the folder the parents are made.
    fs::remove_dir(place.path("blocked.txt")).unwrap();
    place
        .journal
        .exchange(&lock, &entry, place.root(), State::Discarded, &NoFailpoints)
        .unwrap();
    assert_eq!(
        text_of(&place.path("deep/er/file.txt")).as_deref(),
        Some("a")
    );
}

#[test]
fn a_missing_slot_is_left_out_and_the_expected_state_is_checked() {
    let place = Place::new();
    let entry = place.entry(
        State::Discarded,
        &[("gone.txt", Held::File), ("there.txt", Held::File)],
    );
    fs::write(entry.slot(1), "kept").unwrap();
    let lock = place.lock();

    // Another state than the request expects changes nothing.
    let wrong = place
        .journal
        .exchange(&lock, &entry, place.root(), State::Restored, &NoFailpoints)
        .unwrap_err();
    assert!(
        matches!(
            wrong,
            ExchangeError::State {
                actual: State::Discarded
            }
        ),
        "{wrong:?}"
    );
    assert!(!place.path("there.txt").exists());

    let end = place
        .journal
        .exchange(&lock, &entry, place.root(), State::Discarded, &NoFailpoints)
        .unwrap();
    assert_eq!(end.left, [(RepoPath::new("gone.txt"), Left::SlotMissing)]);
    assert_eq!(text_of(&place.path("there.txt")).as_deref(), Some("kept"));
}

/// An exchange of two files that ended after `step`.
fn ended_after(step: &'static str) -> (Place, Entry) {
    let place = Place::new();
    let entry = place.entry(
        State::Discarded,
        &[("one.txt", Held::File), ("two.txt", Held::File)],
    );
    // one.txt has a newer file, two.txt has none.
    fs::write(place.path("one.txt"), "one now").unwrap();
    fs::write(entry.slot(0), "one discarded").unwrap();
    // A different size from the file of slot 0: where the clock is coarse, as on
    // Windows, files written together agree in their times, and the identity of
    // a file is its size and its times.
    fs::write(entry.slot(1), "two were discarded").unwrap();
    let lock = place.lock();
    let ended = place.journal.exchange(
        &lock,
        &entry,
        place.root(),
        State::Discarded,
        &EndAfter(step),
    );
    assert!(
        matches!(ended, Err(ExchangeError::Interrupted)),
        "{ended:?}"
    );
    drop(lock);
    (place, entry)
}

#[test]
fn a_process_that_ends_after_any_step_leaves_an_entry_that_resumes_without_loss() {
    for step in ["manifest", "0:1", "0:2", "0:3", "1:1", "1:2", "1:3"] {
        let (place, entry) = ended_after(step);
        let stopped = reload(&place, &entry);
        assert_eq!(stopped.manifest().state, State::Restoring, "{step}");
        // Every byte is somewhere: at a path, in a slot or in a `.tmp`.
        let mut seen = Vec::new();
        for name in ["one.txt", "two.txt"] {
            seen.extend(text_of(&place.path(name)));
        }
        for file in ["0", "1", "0.tmp", "1.tmp"] {
            seen.extend(text_of(&entry.folder().join(file)));
        }
        seen.sort();
        assert_eq!(
            seen,
            ["one discarded", "one now", "two were discarded"],
            "after {step}"
        );

        let lock = place.lock();
        let end = place
            .journal
            .resume(&lock, &stopped, place.root(), &NoFailpoints)
            .unwrap();
        assert!(end.complete && end.left.is_empty(), "{step}: {end:?}");
        assert_eq!(end.entry.manifest().state, State::Restored, "{step}");
        let one = text_of(&place.path("one.txt"));
        assert_eq!(one.as_deref(), Some("one discarded"), "{step}");
        assert_eq!(
            text_of(&entry.slot(0)).as_deref(),
            Some("one now"),
            "{step}"
        );
        let two = text_of(&place.path("two.txt"));
        assert_eq!(two.as_deref(), Some("two were discarded"), "{step}");
        assert!(!entry.slot(1).exists(), "{step}");
        assert!(!entry.folder().join("0.tmp").exists(), "{step}");
        // Resuming a finished exchange changes nothing.
        let again = place
            .journal
            .resume(&lock, &end.entry, place.root(), &NoFailpoints)
            .unwrap();
        assert_eq!(again.entry.manifest().state, State::Restored, "{step}");
        let one = text_of(&place.path("one.txt"));
        assert_eq!(one.as_deref(), Some("one discarded"), "{step}");
    }
}

#[test]
fn done_is_told_from_not_started_by_identity() {
    // A file of which the slot was empty and the path held a file: after the
    // exchange the path is empty and the slot holds the file, and before it,
    // the reverse. Resuming must not undo a finished exchange.
    let place = Place::new();
    let lock = place.lock();
    let entry = place.entry(State::Discarded, &[("late.txt", Held::None)]);
    fs::write(place.path("late.txt"), "created later").unwrap();
    let ended = place.journal.exchange(
        &lock,
        &entry,
        place.root(),
        State::Discarded,
        &EndAfter("0:3"),
    );
    assert!(matches!(ended, Err(ExchangeError::Interrupted)));
    let end = place
        .journal
        .resume(&lock, &reload(&place, &entry), place.root(), &NoFailpoints)
        .unwrap();
    assert_eq!(text_of(&place.path("late.txt")), None);
    assert_eq!(text_of(&entry.slot(0)).as_deref(), Some("created later"));
    assert_eq!(end.entry.manifest().state, State::Restored);

    // The reverse: a slot that held a file and a path that had none.
    let entry = place.entry(State::Discarded, &[("back.txt", Held::File)]);
    fs::write(entry.slot(0), "slot").unwrap();
    let ended = place.journal.exchange(
        &lock,
        &entry,
        place.root(),
        State::Discarded,
        &EndAfter("0:3"),
    );
    assert!(matches!(ended, Err(ExchangeError::Interrupted)));
    let end = place
        .journal
        .resume(&lock, &reload(&place, &entry), place.root(), &NoFailpoints)
        .unwrap();
    assert_eq!(text_of(&place.path("back.txt")).as_deref(), Some("slot"));
    assert!(!entry.slot(0).exists());
    assert_eq!(end.entry.manifest().state, State::Restored);
}

#[test]
fn a_file_of_another_identity_is_left_alone_when_resuming() {
    // The process ended after step 1; then a program wrote another file at the
    // path. The slot holds the discarded file and `.tmp` the user's old one:
    // step 2 would put the slot's file over the new one, step 3 would put
    // `.tmp` over the slot. Neither may happen.
    let place = Place::new();
    let entry = place.entry(State::Discarded, &[("x.txt", Held::File)]);
    fs::write(place.path("x.txt"), "old now").unwrap();
    fs::write(entry.slot(0), "discarded").unwrap();
    let lock = place.lock();
    let ended = place.journal.exchange(
        &lock,
        &entry,
        place.root(),
        State::Discarded,
        &EndAfter("0:1"),
    );
    assert!(matches!(ended, Err(ExchangeError::Interrupted)));
    fs::write(place.path("x.txt"), "written by a program").unwrap();

    let end = place
        .journal
        .resume(&lock, &reload(&place, &entry), place.root(), &NoFailpoints)
        .unwrap();
    assert!(!end.complete);
    assert_eq!(end.left, [(RepoPath::new("x.txt"), Left::Foreign)]);
    assert_eq!(
        text_of(&place.path("x.txt")).as_deref(),
        Some("written by a program")
    );
    assert_eq!(text_of(&entry.slot(0)).as_deref(), Some("discarded"));
    assert_eq!(
        text_of(&entry.folder().join("0.tmp")).as_deref(),
        Some("old now")
    );
    assert_eq!(reload(&place, &entry).manifest().state, State::Restoring);
}

#[test]
fn no_step_renames_onto_a_name_that_exists() {
    let place = Place::new();
    let entry = place.entry(State::Discarded, &[("x.txt", Held::File)]);
    fs::write(place.path("x.txt"), "now").unwrap();
    fs::write(entry.slot(0), "discarded").unwrap();
    // A `.tmp` that is there from somewhere else.
    fs::write(entry.folder().join("0.tmp"), "somebody's").unwrap();
    let lock = place.lock();
    let refused =
        place
            .journal
            .exchange(&lock, &entry, place.root(), State::Discarded, &NoFailpoints);
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(text_of(&place.path("x.txt")).as_deref(), Some("now"));
    assert_eq!(
        text_of(&entry.folder().join("0.tmp")).as_deref(),
        Some("somebody's")
    );
}

fn dated(place: &Place, name: &str, changed: u64, size: u64, state: State) -> Entry {
    let manifest = Manifest {
        changed,
        size,
        state,
        files: vec![record(0, Held::File, name)],
        ..manifest()
    };
    let entry = place.journal.create(&manifest).unwrap();
    fs::write(entry.slot(0), name).unwrap();
    entry
}

fn names(place: &Place) -> Vec<String> {
    let mut found: Vec<String> = place
        .journal
        .list()
        .unwrap()
        .into_iter()
        .filter_map(|item| match item {
            Listed::Entry { entry, .. } => Some(
                entry.manifest().files[0]
                    .path
                    .to_string_lossy()
                    .into_owned(),
            ),
            Listed::Unreadable { .. } => None,
        })
        .collect();
    found.sort();
    found
}

const DAY: u64 = 24 * 60 * 60;
const NOW: u64 = 1_800_000_000;

#[test]
fn pruning_removes_what_is_old_then_what_is_oldest_beyond_the_size() {
    let place = Place::new();
    let limits = Limits {
        max_age: Duration::from_secs(30 * DAY),
        max_size: 1_000,
    };
    let lock = place.lock();
    dated(&place, "old", NOW - 31 * DAY, 10, State::Discarded);
    dated(
        &place,
        "old-interrupted",
        NOW - 40 * DAY,
        10,
        State::Pending,
    );
    dated(&place, "a", NOW - 3 * DAY, 400, State::Discarded);
    dated(&place, "b", NOW - 2 * DAY, 400, State::Discarded);
    let newest = dated(&place, "new", NOW, 400, State::Discarded);

    let removed = place
        .journal
        .prune(&lock, Some(newest.id()), NOW, &limits)
        .unwrap();
    // "old" by age; then the oldest while the sizes (10 + 400 * 3) are above
    // 1,000: "a" goes, and what is left is within the limit.
    assert_eq!(removed.len(), 2, "{removed:?}");
    assert_eq!(names(&place), ["b", "new", "old-interrupted"]);
}

#[test]
fn pruning_never_removes_the_new_entry_or_an_interrupted_one() {
    let place = Place::new();
    let limits = Limits {
        max_age: Duration::from_secs(30 * DAY),
        max_size: 100,
    };
    let lock = place.lock();
    dated(&place, "interrupted", NOW - 90 * DAY, 500, State::Redoing);
    let newest = dated(&place, "huge", NOW, 5_000, State::Discarded);
    place
        .journal
        .prune(&lock, Some(newest.id()), NOW, &limits)
        .unwrap();
    assert_eq!(names(&place), ["huge", "interrupted"]);
}

#[test]
fn clear_removes_every_entry_and_says_how_many_were_interrupted() {
    let place = Place::new();
    dated(&place, "done", NOW, 1, State::Discarded);
    dated(&place, "half", NOW, 1, State::Pending);
    let broken = dated(&place, "broken", NOW, 1, State::Discarded);
    fs::write(broken.folder().join("manifest"), "not a manifest").unwrap();
    #[cfg(unix)]
    let elsewhere = {
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(elsewhere.path().join("keep.txt"), "keep").unwrap();
        std::os::unix::fs::symlink(
            elsewhere.path(),
            place.journal.dir().join("9999999999999-1"),
        )
        .unwrap();
        elsewhere
    };
    let lock = place.lock();
    let cleared = place.journal.clear(&lock).unwrap();
    assert_eq!(cleared.interrupted, 1);
    #[cfg(unix)]
    {
        assert_eq!(cleared.removed, 4);
        // A link is removed, not followed.
        assert!(elsewhere.path().join("keep.txt").exists());
    }
    assert!(place.journal.list().unwrap().is_empty());
}

#[test]
fn the_identity_of_a_renamed_file_stays_and_the_manifest_keeps_it() {
    let place = Place::new();
    fs::write(place.path("a"), "x").unwrap();
    let before = FileId::of(&fs::symlink_metadata(place.path("a")).unwrap());
    fs::rename(place.path("a"), place.path("b")).unwrap();
    let after = FileId::of(&fs::symlink_metadata(place.path("b")).unwrap());
    assert_eq!(before, after);
    let m = Manifest {
        idents: vec![Ident {
            slot: 0,
            side: Side::Path,
            id: Some(before),
        }],
        ..manifest()
    };
    assert_eq!(Manifest::parse(&m.to_text()).unwrap(), m);
}
