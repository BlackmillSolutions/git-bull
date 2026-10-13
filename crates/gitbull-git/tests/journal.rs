//! The journal of discards with real Git and real files (spec `discard`,
//! requirement "The journal of discards"). One test owns process-wide
//! configuration before any worker starts, like `tests/patch.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gitbull_git::Git;
use gitbull_git::journal::{
    Entry, FileKind, FileRecord, Held, Journal, Kind, Listed, Manifest, PathRefusal, Running,
    State, checked,
};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::path::RepoPath;
use gitbull_testkit::TestRepo;

/// Set in the process that holds the lock for the test of the lock.
const HOLD: &str = "GITBULL_JOURNAL_HOLD";

#[test]
fn the_journal_preserves_what_it_is_given() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("gitconfig");
    fs::write(&global, "[maintenance]\n auto = false\n[gc]\n auto = 0\n").unwrap();
    // SAFETY: this integration binary has one test that sets these before any
    // thread of its own starts; the helper below is a separate process.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let executable = locate_git(None, Os::current(), &SystemProbe).unwrap();
    let empty_hooks = root.path().join("empty-hooks");
    fs::create_dir(&empty_hooks).unwrap();
    let git = Git::new(executable, empty_hooks);

    the_folder_is_where_git_says(&git);
    listing_creates_nothing(&git);
    a_manifest_is_read_back_as_written();
    entries_list_newest_first_and_ids_differ(&git);
    what_is_not_written_by_git_bull_is_unreadable(&git);
    the_manifest_of_a_removed_entry_goes_last(&git);
    paths_are_checked();
    the_lock_is_taken_by_one_process_at_a_time(&git);
}

fn repository() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Journal Test");
    repo.config("user.email", "journal@example.com");
    repo.write("f.txt", "one\n");
    repo.commit("Initial");
    repo
}

fn journal_of(git: &Git, folder: &Path) -> Journal {
    Journal::locate(git, folder).unwrap()
}

fn record(slot: u32, kind: FileKind, held: Held, path: &[u8]) -> FileRecord {
    FileRecord {
        slot,
        kind,
        held,
        path: RepoPath::new(path),
    }
}

fn manifest() -> Manifest {
    Manifest {
        changed: 1_760_270_000,
        size: 123_456,
        kind: Kind::Files,
        state: State::Pending,
        git: Some(4242),
        files: vec![
            record(0, FileKind::Untracked, Held::File, b"notes.txt"),
            record(1, FileKind::Tracked, Held::None, b"dir/gone.rs"),
            record(2, FileKind::Untracked, Held::Link, b"a link"),
        ],
        idents: Vec::new(),
    }
}

fn the_folder_is_where_git_says(git: &Git) {
    let repo = repository();
    let main = journal_of(git, repo.path());
    assert_eq!(main.dir(), repo.path().join(".git/gitbull/discards"));

    // A subfolder of the worktree gives the same folder, reached over `..`.
    fs::create_dir(repo.path().join("sub")).unwrap();
    let from_sub = journal_of(git, &repo.path().join("sub"));
    let git_folder = |journal: &Journal| {
        fs::canonicalize(journal.dir().parent().unwrap().parent().unwrap()).unwrap()
    };
    assert_eq!(git_folder(&from_sub), git_folder(&main));

    // A linked worktree has its own, inside the folder of that worktree.
    let linked_root = tempfile::tempdir().unwrap();
    let linked = linked_root.path().join("linked");
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        linked.to_str().unwrap(),
    ]);
    let own = journal_of(git, &linked);
    assert!(
        own.dir().to_string_lossy().contains("worktrees"),
        "{:?}",
        own.dir()
    );
    // Git reports real paths: on macOS `/private/var` for `/var`.
    let git_folder = repo.path().join(".git").canonicalize().unwrap();
    assert!(own.dir().starts_with(&git_folder), "{:?}", own.dir());
    assert_ne!(own.dir(), main.dir());

    // A `.git` file that points elsewhere.
    let separate = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let status = Command::new("git")
        .args(["init", "-q", "--separate-git-dir"])
        .arg(separate.path().join("g"))
        .arg(work.path())
        .status()
        .unwrap();
    assert!(status.success());
    let elsewhere = journal_of(git, work.path());
    assert!(elsewhere.dir().starts_with(separate.path().join("g")));

    // A submodule's Git folder lies in the superproject's.
    let upstream = repository();
    let outer = repository();
    outer.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        "-q",
        &upstream.url(),
        "lib",
    ]);
    let inner = journal_of(git, &outer.path().join("lib"));
    assert!(
        inner.dir().starts_with(outer.path().join(".git/modules")),
        "{:?}",
        inner.dir()
    );
}

fn listing_creates_nothing(git: &Git) {
    let repo = repository();
    let journal = journal_of(git, repo.path());
    assert!(journal.list().unwrap().is_empty());
    assert!(!repo.path().join(".git/gitbull").exists());
}

fn a_manifest_is_read_back_as_written() {
    let written = manifest();
    let text = written.to_text();
    assert!(text.starts_with("gitbull-discard 1\n"));
    assert_eq!(Manifest::parse(&text).unwrap(), written);

    // A path with a newline, a space, and bytes that are not UTF-8.
    let odd = Manifest {
        files: vec![
            record(0, FileKind::Untracked, Held::File, b"line\nbreak.txt"),
            record(1, FileKind::Tracked, Held::File, b"with space.txt"),
            record(2, FileKind::Untracked, Held::File, b"bytes\xff\xfe.bin"),
        ],
        git: None,
        ..manifest()
    };
    let text = odd.to_text();
    assert_eq!(text.lines().count(), odd.to_text().lines().count());
    assert!(!text.contains("line\nbreak"));
    assert_eq!(Manifest::parse(&text).unwrap(), odd);

    // The states and the kinds survive.
    for state in [
        State::Pending,
        State::Discarded,
        State::Restoring,
        State::Restored,
        State::Redoing,
    ] {
        for kind in [Kind::Files, Kind::Group, Kind::Hunk, Kind::Lines] {
            let m = Manifest {
                state,
                kind,
                ..manifest()
            };
            assert_eq!(Manifest::parse(&m.to_text()).unwrap(), m);
        }
    }

    // What cannot be read.
    assert!(Manifest::parse("gitbull-discard 2\n").is_err());
    assert!(Manifest::parse("nonsense\n").is_err());
    let bad_slot = written.to_text().replace("file 0 ", "file x ");
    assert!(Manifest::parse(&bad_slot).is_err());
}

fn made(journal: &Journal, state: State, file: &str) -> Entry {
    let m = Manifest {
        state,
        files: vec![record(0, FileKind::Untracked, Held::File, file.as_bytes())],
        ..manifest()
    };
    journal.create(&m).unwrap()
}

fn entries_list_newest_first_and_ids_differ(git: &Git) {
    let repo = repository();
    let journal = journal_of(git, repo.path());
    // Several entries in the same millisecond: the counter tells them apart.
    let first = made(&journal, State::Discarded, "a");
    let second = made(&journal, State::Discarded, "b");
    let third = made(&journal, State::Discarded, "c");
    let ids = [first.id(), second.id(), third.id()];
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        3,
        "{ids:?}"
    );
    let listed = journal.list().unwrap();
    let names: Vec<_> = listed
        .iter()
        .map(|item| match item {
            Listed::Entry { entry, .. } => entry.id().to_owned(),
            Listed::Unreadable { folder, .. } => panic!("{folder:?}"),
        })
        .collect();
    assert_eq!(names, [third.id(), second.id(), first.id()]);
    // The manifest of an entry is the one that was written.
    let Listed::Entry { entry, .. } = &listed[0] else {
        unreachable!()
    };
    assert_eq!(entry.manifest().files[0].path, RepoPath::new("c"));
}

fn what_is_not_written_by_git_bull_is_unreadable(git: &Git) {
    let repo = repository();
    let journal = journal_of(git, repo.path());
    let good = made(&journal, State::Discarded, "good");

    let unreadable = |journal: &Journal| -> Vec<String> {
        journal
            .list()
            .unwrap()
            .into_iter()
            .filter_map(|item| match item {
                Listed::Unreadable { folder, .. } => {
                    Some(folder.file_name().unwrap().to_string_lossy().into_owned())
                }
                Listed::Entry { .. } => None,
            })
            .collect()
    };
    assert!(unreadable(&journal).is_empty());

    // An unknown version.
    let version = made(&journal, State::Discarded, "version");
    let manifest_path = version.folder().join("manifest");
    let text = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        text.replacen("gitbull-discard 1", "gitbull-discard 9", 1),
    )
    .unwrap();

    // A slot number that is not digits.
    let digits = made(&journal, State::Discarded, "digits");
    let manifest_path = digits.folder().join("manifest");
    let text = fs::read_to_string(&manifest_path).unwrap();
    fs::write(&manifest_path, text.replace("file 0 ", "file ../x ")).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let elsewhere = tempfile::tempdir().unwrap();
        // The manifest is a link.
        let linked = made(&journal, State::Discarded, "linked");
        let manifest_path = linked.folder().join("manifest");
        fs::rename(&manifest_path, elsewhere.path().join("m")).unwrap();
        symlink(elsewhere.path().join("m"), &manifest_path).unwrap();
        // A slot is a link where the manifest says a file.
        let slot = made(&journal, State::Discarded, "slot");
        fs::write(slot.folder().join("0"), "x").unwrap();
        fs::remove_file(slot.folder().join("0")).unwrap();
        symlink(elsewhere.path(), slot.folder().join("0")).unwrap();
        // An entry that is a link to another folder.
        let target = elsewhere.path().join("entry");
        fs::create_dir(&target).unwrap();
        symlink(&target, journal.dir().join("9999999999999-0")).unwrap();
    }

    let names = unreadable(&journal);
    assert!(
        names.contains(&version.id().to_owned()) && names.contains(&digits.id().to_owned()),
        "{names:?}"
    );
    #[cfg(unix)]
    assert_eq!(names.len(), 5, "{names:?}");
    // The good entry is still listed, and reading removed nothing.
    let listed = journal.list().unwrap();
    assert!(
        listed
            .iter()
            .any(|item| matches!(item, Listed::Entry { entry, .. } if entry.id() == good.id()))
    );
    assert!(version.folder().exists() && digits.folder().exists());

    // `discards` itself a link: nothing in it is followed.
    #[cfg(unix)]
    {
        let other = repository();
        let other_journal = journal_of(git, other.path());
        fs::create_dir_all(other.path().join(".git/gitbull")).unwrap();
        let real = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(real.path(), other_journal.dir()).unwrap();
        assert!(other_journal.list().is_err());
        assert!(other_journal.create(&manifest()).is_err());
    }

    // A link planted at `.lock` or at `manifest.new` is not written through.
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let repo = repository();
        let journal = journal_of(git, repo.path());
        let entry = made(&journal, State::Discarded, "planted");
        let outside = tempfile::tempdir().unwrap();
        let precious = outside.path().join("notes.txt");
        fs::write(&precious, "keep me").unwrap();
        symlink(&precious, entry.folder().join("manifest.new")).unwrap();
        journal.write_manifest(&entry, &manifest()).unwrap();
        assert_eq!(fs::read_to_string(&precious).unwrap(), "keep me");
        assert!(
            !fs::symlink_metadata(entry.folder().join("manifest"))
                .unwrap()
                .file_type()
                .is_symlink()
        );

        symlink(&precious, journal.dir().join(".lock")).unwrap();
        assert!(journal.lock("x", Duration::from_millis(50)).is_err());
        assert_eq!(fs::read_to_string(&precious).unwrap(), "keep me");
    }
}

fn the_manifest_of_a_removed_entry_goes_last(git: &Git) {
    let repo = repository();
    let journal = journal_of(git, repo.path());
    let entry = made(&journal, State::Discarded, "x");
    fs::write(entry.folder().join("0"), "bytes").unwrap();
    fs::write(entry.folder().join("0.part"), "half").unwrap();
    let mut seen = Vec::new();
    journal
        .remove_entry_with(&entry, &mut |step| seen.push(step.to_owned()))
        .unwrap();
    assert!(!entry.folder().exists());
    assert_eq!(
        seen.last().map(String::as_str),
        Some("manifest"),
        "{seen:?}"
    );
    assert!(seen.iter().any(|name| name == "0"));
}

fn paths_are_checked() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path();
    fs::create_dir_all(root.join("dir")).unwrap();
    fs::write(root.join("dir/file"), "x").unwrap();
    let refusal = |path: &[u8]| checked(root, &RepoPath::new(path)).unwrap_err();

    assert_eq!(refusal(b""), PathRefusal::Odd);
    assert_eq!(refusal(b"/etc/passwd"), PathRefusal::Absolute);
    assert_eq!(refusal(b"../x"), PathRefusal::Odd);
    assert_eq!(refusal(b"dir/../x"), PathRefusal::Odd);
    assert_eq!(refusal(b"./x"), PathRefusal::Odd);
    assert_eq!(refusal(b"a//b"), PathRefusal::Odd);
    assert_eq!(refusal(b".git/config"), PathRefusal::DotGit);
    assert_eq!(refusal(b"x/.GIT/config"), PathRefusal::DotGit);
    assert_eq!(refusal(b"x/.git."), PathRefusal::DotGit);
    assert_eq!(refusal(b"GIT~1/config"), PathRefusal::DotGit);
    // Git's way to list a repository of its own.
    assert_eq!(refusal(b"nested/"), PathRefusal::NestedRepository);
    // A folder where the path ends.
    assert_eq!(refusal(b"dir"), PathRefusal::FolderAtPath);

    // Files, and what does not exist, are fine.
    assert_eq!(
        checked(root, &RepoPath::new("dir/file")).unwrap(),
        root.join("dir/file")
    );
    assert_eq!(
        checked(root, &RepoPath::new("not/there/yet")).unwrap(),
        root.join("not/there/yet")
    );
    assert_eq!(
        checked(root, &RepoPath::new("odd name")).unwrap(),
        root.join("odd name")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), "s").unwrap();
        // A parent that is a link, also one that leaves the working copy.
        symlink(outside.path(), root.join("out")).unwrap();
        assert_eq!(refusal(b"out/secret"), PathRefusal::ThroughLink);
        symlink(root.join("dir"), root.join("in")).unwrap();
        assert_eq!(refusal(b"in/file"), PathRefusal::ThroughLink);
        // A link as the last component is accepted: it is moved as the link.
        assert_eq!(
            checked(root, &RepoPath::new("out")).unwrap(),
            root.join("out")
        );
        symlink("dir/file", root.join("link")).unwrap();
        assert_eq!(
            checked(root, &RepoPath::new("link")).unwrap(),
            root.join("link")
        );
        // Bytes that are not UTF-8 are fine on Unix.
        assert!(checked(root, &RepoPath::new(b"odd\xff".to_vec())).is_ok());
    }
}

/// The helper process: takes the lock, says so, and holds it until it ends.
#[test]
#[ignore = "started by the test of the lock"]
fn hold_the_lock() {
    let Ok(folder) = std::env::var(HOLD) else {
        return;
    };
    let journal = Journal::at(PathBuf::from(folder));
    let guard = journal.lock("0-0", Duration::from_secs(5)).unwrap();
    println!("held");
    std::thread::sleep(Duration::from_secs(60));
    drop(guard);
}

fn wait_for_line(child: &mut std::process::Child, wanted: &str) {
    use std::io::{BufRead, BufReader};
    let out = child.stdout.take().unwrap();
    for line in BufReader::new(out).lines() {
        if line.unwrap().trim() == wanted {
            return;
        }
    }
    panic!("the helper never said {wanted}");
}

fn the_lock_is_taken_by_one_process_at_a_time(git: &Git) {
    let repo = repository();
    let journal = journal_of(git, repo.path());
    fs::create_dir_all(journal.dir()).unwrap();

    // An entry that is not finished, and the lock held by another process.
    let unfinished = made(&journal, State::Pending, "unfinished");
    let mut helper = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "hold_the_lock", "--nocapture"])
        .env(HOLD, journal.dir())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    wait_for_line(&mut helper, "held");

    // A second taker waits and gives up.
    let started = Instant::now();
    let refused = journal.lock(unfinished.id(), Duration::from_millis(400));
    assert!(refused.is_err(), "the lock was taken twice");
    assert!(started.elapsed() >= Duration::from_millis(350));

    // Listing takes no lock, and tells that somebody works: the entry the
    // lock names is running, every other unfinished one is interrupted.
    let listed = journal.list().unwrap();
    let Listed::Entry { running, .. } = &listed[0] else {
        panic!("{listed:?}")
    };
    assert_eq!(
        *running,
        Running::Interrupted,
        "the lock names another entry"
    );

    // The helper is killed: the lock is free at once.
    helper.kill().unwrap();
    helper.wait().unwrap();
    let again = Instant::now();
    let guard = journal
        .lock(unfinished.id(), Duration::from_secs(5))
        .unwrap();
    assert!(
        again.elapsed() < Duration::from_secs(2),
        "the lock was not free at once"
    );

    // Held by this process and naming the entry: running.
    let listed = journal.list().unwrap();
    let Listed::Entry { running, .. } = &listed[0] else {
        panic!("{listed:?}")
    };
    assert_eq!(*running, Running::Running);
    drop(guard);
    let listed = journal.list().unwrap();
    let Listed::Entry { running, .. } = &listed[0] else {
        panic!("{listed:?}")
    };
    assert_eq!(*running, Running::Interrupted);
}
