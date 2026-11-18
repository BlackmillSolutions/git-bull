//! Discarding hunks and lines with real Git (spec `discard`; design decision
//! 4). One test owns process-wide configuration before any worker starts, like
//! `tests/patch.rs`.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use gitbull_git::Git;
use gitbull_git::cancel::CancelToken;
use gitbull_git::diff::{Content, FileDiff, LineKind};
use gitbull_git::discard::{DiscardEnd, DiscardOutcome, DiscardRefusal, discard_part};
use gitbull_git::journal::{
    Entry, Failpoints, Interrupt, Journal, Kind, Listed, NoFailpoints, State,
};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::patch::{PartRequest, Selection, stage_part};
use gitbull_git::status::{Group, StatusEntry, status};
use gitbull_git::working_copy::working_diff;
use gitbull_testkit::TestRepo;

#[test]
fn part_discards_take_out_the_part_and_nothing_else() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("gitconfig");
    fs::write(&global, "[maintenance]\n auto = false\n[gc]\n auto = 0\n").unwrap();
    // SAFETY: this integration binary has one test and no threads have started.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let executable = locate_git(None, Os::current(), &SystemProbe).unwrap();
    let empty_hooks = root.path().join("empty-hooks");
    fs::create_dir(&empty_hooks).unwrap();
    let git = Git::new(executable, empty_hooks);

    a_hunk_is_discarded_and_the_copy_holds_the_file_as_it_was(&git);
    lines_are_discarded_and_removed_lines_come_back(&git);
    a_staged_hunk_stays_staged(&git);
    an_end_without_a_line_break_and_the_mode_are_kept(&git);
    whitespace_settings_of_the_user_do_not_touch_other_lines(&git);
    crlf_throughout_is_discarded_under_conversion(&git);
    crlf_throughout_is_discarded_under_an_eol_attribute(&git);
    mixed_endings_without_conversion_are_discarded_byte_for_byte(&git);
    conversion_that_would_rewrite_other_lines_is_refused(&git);
    conversions_by_attribute_that_rewrite_other_lines_are_refused(&git);
    a_diff_out_of_date_is_refused_without_an_entry(&git);
    a_file_that_changes_while_copied_is_refused(&git);
    a_file_edited_while_git_applies_keeps_both_versions(&git);
    an_untracked_file_is_refused(&git);
}

const BASE: &str = "l01\nl02\nl03\nl04\nl05\nl06\nl07\nl08\nl09\nl10\nl11\nl12\nl13\nl14\nl15\nl16\nl17\nl18\nl19\nl20\nl21\nl22\nl23\nl24\nl25\nl26\nl27\nl28\nl29\nl30\n";

/// `BASE` with `l02` replaced by `L02` and two lines added after `l20`: two
/// hunks.
fn two_hunks() -> String {
    BASE.replace("l02\n", "L02\n")
        .replace("l20\n", "l20\nadd A\nadd B\n")
}

fn committed(name: &str, content: &[u8]) -> TestRepo {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Discard Test");
    repo.config("user.email", "discard@example.com");
    repo.config("commit.gpgsign", "false");
    fs::write(repo.path().join(name), content).unwrap();
    repo.commit("Initial");
    repo
}

fn journal_of(git: &Git, repo: &TestRepo) -> Journal {
    Journal::locate(git, repo.path()).unwrap()
}

fn entries(git: &Git, repo: &TestRepo) -> Vec<Entry> {
    journal_of(git, repo)
        .list()
        .unwrap()
        .into_iter()
        .map(|item| match item {
            Listed::Entry { entry, .. } => entry,
            Listed::Unreadable { folder, why } => panic!("unreadable {folder:?}: {why}"),
        })
        .collect()
}

fn entry(git: &Git, repo: &TestRepo, group: Group, name: &str) -> StatusEntry {
    let status = status(git, repo.path(), &CancelToken::new()).unwrap();
    status
        .group(group)
        .iter()
        .find(|entry| entry.path.to_string_lossy() == name)
        .unwrap_or_else(|| panic!("no {name} in {group:?}: {status:?}"))
        .clone()
}

fn shown(git: &Git, repo: &TestRepo, group: Group, name: &str) -> Arc<FileDiff> {
    let entry = entry(git, repo, group, name);
    Arc::new(working_diff(git, repo.path(), group, &entry, None, &CancelToken::new()).unwrap())
}

/// The lines of `diff` named by their sign and text, such as `+add A`.
fn lines(diff: &FileDiff, wanted: &[&str]) -> Selection {
    let Content::Text(hunks) = &diff.content else {
        panic!("no text: {diff:?}");
    };
    let mut selection = Selection::new();
    for want in wanted {
        let (sign, text) = want.split_at(1);
        let kind = match sign {
            "+" => LineKind::Added,
            "-" => LineKind::Removed,
            other => panic!("{other} is no sign"),
        };
        let (hunk, line) = hunks
            .iter()
            .enumerate()
            .find_map(|(hunk, lines)| {
                let line = lines
                    .lines
                    .iter()
                    .position(|line| line.kind == kind && line.text == text)?;
                Some((hunk, line))
            })
            .unwrap_or_else(|| panic!("no line {want}"));
        selection = selection.line(hunk, line);
    }
    selection
}

fn discard(
    git: &Git,
    repo: &TestRepo,
    name: &str,
    shown: Arc<FileDiff>,
    selection: Selection,
    points: &dyn Failpoints,
) -> DiscardEnd {
    let entry = entry(git, repo, Group::Unstaged, name);
    let request = PartRequest { shown, selection };
    discard_part(
        git,
        repo.path(),
        &journal_of(git, repo),
        &entry,
        &request,
        points,
    )
}

/// Discards the hunk at `hunk` of the diff of `name` as it is.
fn discard_hunk(git: &Git, repo: &TestRepo, name: &str, hunk: usize) -> DiscardEnd {
    let diff = shown(git, repo, Group::Unstaged, name);
    discard(
        git,
        repo,
        name,
        diff,
        Selection::new().hunk(hunk),
        &NoFailpoints,
    )
}

/// Discards the lines `wanted` of the diff of `name` as it is.
fn discard_lines(git: &Git, repo: &TestRepo, name: &str, wanted: &[&str]) -> DiscardEnd {
    let diff = shown(git, repo, Group::Unstaged, name);
    let selection = lines(&diff, wanted);
    discard(git, repo, name, diff, selection, &NoFailpoints)
}

fn done(end: &DiscardEnd) {
    assert_eq!(end.outcome, DiscardOutcome::Done, "{end:?}");
}

fn refused(end: &DiscardEnd, why: DiscardRefusal) {
    assert_eq!(end.outcome, DiscardOutcome::Refused(why), "{end:?}");
    assert_eq!(end.entry, None);
}

fn bytes(repo: &TestRepo, name: &str) -> Vec<u8> {
    fs::read(repo.path().join(name)).unwrap()
}

fn text(repo: &TestRepo, name: &str) -> String {
    String::from_utf8(bytes(repo, name)).unwrap()
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

/// Restores the single entry, the way the interface does it.
fn restore(git: &Git, repo: &TestRepo, entry: &Entry) {
    let journal = journal_of(git, repo);
    let lock = journal.lock(entry.id(), Duration::from_secs(2)).unwrap();
    journal
        .exchange(
            &lock,
            entry,
            repo.path(),
            entry.manifest().state,
            &NoFailpoints,
        )
        .unwrap();
}

fn a_hunk_is_discarded_and_the_copy_holds_the_file_as_it_was(git: &Git) {
    let repo = committed("f.txt", BASE.as_bytes());
    fs::write(repo.path().join("f.txt"), two_hunks()).unwrap();
    let index_before = repo.git(&["rev-parse", ":f.txt"]);

    done(&discard_hunk(git, &repo, "f.txt", 0));

    assert_eq!(
        text(&repo, "f.txt"),
        BASE.replace("l20\n", "l20\nadd A\nadd B\n")
    );
    assert_eq!(repo.git(&["rev-parse", ":f.txt"]), index_before);
    let entry = entries(git, &repo).remove(0);
    let manifest = entry.manifest();
    assert_eq!(
        (manifest.kind, manifest.state),
        (Kind::Hunk, State::Discarded)
    );
    assert_eq!(read(&entry.slot(0)), two_hunks().as_bytes());

    // Restoring puts the file as it was before the discard back.
    restore(git, &repo, &entry);
    assert_eq!(text(&repo, "f.txt"), two_hunks());
}

fn lines_are_discarded_and_removed_lines_come_back(git: &Git) {
    let repo = committed("f.txt", BASE.as_bytes());
    fs::write(repo.path().join("f.txt"), two_hunks()).unwrap();

    let end = discard_lines(git, &repo, "f.txt", &["+add A"]);
    done(&end);
    assert_eq!(
        text(&repo, "f.txt"),
        BASE.replace("l02\n", "L02\n")
            .replace("l20\n", "l20\nadd B\n")
    );
    assert_eq!(entries(git, &repo)[0].manifest().kind, Kind::Lines);

    // A removed line selected alone is put back beside the line that replaced
    // it.
    let end = discard_lines(git, &repo, "f.txt", &["-l02"]);
    done(&end);
    assert_eq!(
        text(&repo, "f.txt"),
        BASE.replace("l02\n", "l02\nL02\n")
            .replace("l20\n", "l20\nadd B\n")
    );

    // A file with a line deleted: the deletion is discarded.
    let repo = committed("g.txt", BASE.as_bytes());
    fs::write(repo.path().join("g.txt"), BASE.replace("l05\n", "")).unwrap();
    done(&discard_lines(git, &repo, "g.txt", &["-l05"]));
    assert_eq!(text(&repo, "g.txt"), BASE);
}

fn a_staged_hunk_stays_staged(git: &Git) {
    let repo = committed("f.txt", BASE.as_bytes());
    fs::write(repo.path().join("f.txt"), two_hunks()).unwrap();
    // Stage the first hunk; the second stays unstaged.
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    let staging = PartRequest {
        shown: diff,
        selection: Selection::new().hunk(0),
    };
    let unstaged = entry(git, &repo, Group::Unstaged, "f.txt");
    stage_part(
        git,
        repo.path(),
        Group::Unstaged,
        &unstaged,
        &staging,
        &CancelToken::new(),
    )
    .unwrap();
    let staged = repo.git(&["show", ":f.txt"]);

    done(&discard_hunk(git, &repo, "f.txt", 0));

    assert_eq!(repo.git(&["show", ":f.txt"]), staged);
    assert_eq!(text(&repo, "f.txt"), staged);
    assert_eq!(repo.git(&["status", "--porcelain"]).trim_end(), "M  f.txt");
}

fn an_end_without_a_line_break_and_the_mode_are_kept(git: &Git) {
    let repo = committed("f.txt", b"a\nb\nc");
    fs::write(repo.path().join("f.txt"), b"a\nb\nc2").unwrap();
    done(&discard_hunk(git, &repo, "f.txt", 0));
    assert_eq!(bytes(&repo, "f.txt"), b"a\nb\nc");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let repo = committed("run.sh", BASE.as_bytes());
        repo.git(&["update-index", "--chmod=+x", "run.sh"]);
        repo.git(&["commit", "-m", "Executable"]);
        let path = repo.path().join("run.sh");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&path, BASE.replace("l05\n", "x05\n")).unwrap();
        done(&discard_hunk(git, &repo, "run.sh", 0));
        assert_eq!(text(&repo, "run.sh"), BASE);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}

fn whitespace_settings_of_the_user_do_not_touch_other_lines(git: &Git) {
    let base = "a \nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\nq\nr\ns\nt \nu\n";
    let repo = committed("f.txt", base.as_bytes());
    repo.config("apply.whitespace", "error");
    repo.config("core.whitespace", "trailing-space,cr-at-eol");
    let changed = base.replace("\nb\n", "\nB\n").replace("\nu\n", "\nU\n");
    fs::write(repo.path().join("f.txt"), &changed).unwrap();

    done(&discard_hunk(git, &repo, "f.txt", 0));

    // The lines with trailing blanks are as the user had them.
    assert_eq!(text(&repo, "f.txt"), base.replace("\nu\n", "\nU\n"));
}

fn crlf_throughout_is_discarded_under_conversion(git: &Git) {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Discard Test");
    repo.config("user.email", "discard@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("core.autocrlf", "true");
    let crlf = |text: &str| text.replace('\n', "\r\n");
    fs::write(repo.path().join("f.txt"), crlf(BASE)).unwrap();
    repo.commit("Initial");
    fs::write(repo.path().join("f.txt"), crlf(&two_hunks())).unwrap();

    done(&discard_hunk(git, &repo, "f.txt", 0));

    assert_eq!(
        bytes(&repo, "f.txt"),
        crlf(&BASE.replace("l20\n", "l20\nadd A\nadd B\n")).as_bytes()
    );
}

/// A repository whose index holds `BASE` with LF endings, under the
/// attributes `attributes`.
fn committed_under(attributes: &str, content: &str) -> TestRepo {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Discard Test");
    repo.config("user.email", "discard@example.com");
    repo.config("commit.gpgsign", "false");
    fs::write(repo.path().join(".gitattributes"), attributes).unwrap();
    fs::write(repo.path().join("f.txt"), content).unwrap();
    repo.commit("Initial");
    repo
}

fn crlf_throughout_is_discarded_under_an_eol_attribute(git: &Git) {
    let crlf = |text: &str| text.replace('\n', "\r\n");
    let repo = committed_under("* text eol=crlf\n", &crlf(BASE));
    fs::write(repo.path().join("f.txt"), crlf(&two_hunks())).unwrap();

    done(&discard_hunk(git, &repo, "f.txt", 0));

    assert_eq!(
        bytes(&repo, "f.txt"),
        crlf(&BASE.replace("l20\n", "l20\nadd A\nadd B\n")).as_bytes()
    );
}

/// Where Git converts line endings while it writes, the discard is refused and
/// the file is as it was. Git for Windows may convert nothing here, and then
/// the discard is the plain one: every other line is byte for byte as it
/// was. Either way no line that was not selected changes.
fn refused_or_exact(
    git: &Git,
    repo: &TestRepo,
    end: &DiscardEnd,
    before: &[u8],
    plain_after: &[u8],
) {
    match &end.outcome {
        DiscardOutcome::Refused(DiscardRefusal::LineEndings) if !cfg!(windows) => {
            assert_eq!(end.entry, None);
            assert_eq!(bytes(repo, "f.txt"), before);
            assert!(entries(git, repo).is_empty());
        }
        DiscardOutcome::Refused(DiscardRefusal::LineEndings) => {
            assert_eq!(bytes(repo, "f.txt"), before);
            assert!(entries(git, repo).is_empty());
        }
        DiscardOutcome::Done if cfg!(windows) => assert_eq!(bytes(repo, "f.txt"), plain_after),
        other => panic!("{other:?}"),
    }
}

fn conversions_by_attribute_that_rewrite_other_lines_are_refused(git: &Git) {
    // CRLF in the working copy and LF in the index, under `text=auto`.
    let repo = committed_under("* text=auto\n", BASE);
    let changed = two_hunks().replace('\n', "\r\n");
    fs::write(repo.path().join("f.txt"), &changed).unwrap();
    let end = discard_hunk(git, &repo, "f.txt", 0);
    let plain = BASE
        .replace("l20\n", "l20\nadd A\nadd B\n")
        .replace('\n', "\r\n");
    refused_or_exact(git, &repo, &end, changed.as_bytes(), plain.as_bytes());

    // Mixed endings under `eol=lf`.
    let repo = committed_under("* text eol=lf\n", BASE);
    let mixed = two_hunks()
        .replace("l10\n", "l10\r\n")
        .replace("l25\n", "l25\r\n");
    fs::write(repo.path().join("f.txt"), &mixed).unwrap();
    let end = discard_hunk(git, &repo, "f.txt", 0);
    let plain = BASE
        .replace("l10\n", "l10\r\n")
        .replace("l25\n", "l25\r\n")
        .replace("l20\n", "l20\nadd A\nadd B\n");
    refused_or_exact(git, &repo, &end, mixed.as_bytes(), plain.as_bytes());
}

fn mixed_endings_without_conversion_are_discarded_byte_for_byte(git: &Git) {
    let mixed = BASE.replace("l10\n", "l10\r\n").replace("l25\n", "l25\r\n");
    let repo = committed("f.txt", mixed.as_bytes());
    let changed = mixed.replace("l02\n", "L02\n");
    fs::write(repo.path().join("f.txt"), &changed).unwrap();

    done(&discard_hunk(git, &repo, "f.txt", 0));

    assert_eq!(bytes(&repo, "f.txt"), mixed.as_bytes());
}

fn conversion_that_would_rewrite_other_lines_is_refused(git: &Git) {
    // An LF file under `core.autocrlf=true`: Git writes the lines it patches
    // with CRLF, and the others stay, which the check finds.
    let mut repo = TestRepo::new();
    repo.config("user.name", "Discard Test");
    repo.config("user.email", "discard@example.com");
    repo.config("commit.gpgsign", "false");
    fs::write(repo.path().join("f.txt"), BASE).unwrap();
    repo.commit("Initial");
    repo.config("core.autocrlf", "true");
    let changed = two_hunks();
    fs::write(repo.path().join("f.txt"), &changed).unwrap();

    let end = discard_hunk(git, &repo, "f.txt", 0);

    // Git rewrote lines it should not have: refused, and the file is as it
    // was.
    let plain = BASE.replace("l20\n", "l20\nadd A\nadd B\n");
    refused_or_exact(git, &repo, &end, changed.as_bytes(), plain.as_bytes());
}

fn a_diff_out_of_date_is_refused_without_an_entry(git: &Git) {
    let repo = committed("f.txt", BASE.as_bytes());
    fs::write(repo.path().join("f.txt"), two_hunks()).unwrap();
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    // The file changes after the diff was shown.
    let later = two_hunks().replace("add A\n", "add Z\n");
    fs::write(repo.path().join("f.txt"), &later).unwrap();

    let end = discard(
        git,
        &repo,
        "f.txt",
        diff,
        Selection::new().hunk(1),
        &NoFailpoints,
    );

    refused(&end, DiscardRefusal::DiffOutOfDate);
    assert_eq!(text(&repo, "f.txt"), later);
    assert!(entries(git, &repo).is_empty());
}

fn a_file_that_changes_while_copied_is_refused(git: &Git) {
    struct Edit<'a>(&'a Path);
    impl Failpoints for Edit<'_> {
        fn after(&self, step: &str) -> Result<(), Interrupt> {
            if step == "copy" {
                let mut text = fs::read_to_string(self.0).unwrap();
                text.push_str("late\n");
                fs::write(self.0, text).unwrap();
            }
            Ok(())
        }
    }
    let repo = committed("f.txt", BASE.as_bytes());
    fs::write(repo.path().join("f.txt"), two_hunks()).unwrap();
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    let path = repo.path().join("f.txt");

    let end = discard(
        git,
        &repo,
        "f.txt",
        diff,
        Selection::new().hunk(0),
        &Edit(&path),
    );

    refused(&end, DiscardRefusal::DiffOutOfDate);
    assert_eq!(text(&repo, "f.txt"), format!("{}late\n", two_hunks()));
    assert!(entries(git, &repo).is_empty());
}

fn an_untracked_file_is_refused(git: &Git) {
    let repo = committed("f.txt", BASE.as_bytes());
    fs::write(repo.path().join("new.txt"), "one\ntwo\n").unwrap();
    let untracked = entry(git, &repo, Group::Untracked, "new.txt");
    let diff = Arc::new(
        working_diff(
            git,
            repo.path(),
            Group::Untracked,
            &untracked,
            None,
            &CancelToken::new(),
        )
        .unwrap(),
    );
    let request = PartRequest {
        shown: diff,
        selection: Selection::new().hunk(0),
    };

    let end = discard_part(
        git,
        repo.path(),
        &journal_of(git, &repo),
        &untracked,
        &request,
        &NoFailpoints,
    );

    refused(&end, DiscardRefusal::NotTracked);
    assert!(entries(git, &repo).is_empty());
}

fn a_file_edited_while_git_applies_keeps_both_versions(git: &Git) {
    // The file changes after it was compared with its copy and before Git
    // reads it: the copy is not what Git worked on, so it must not replace the
    // file.
    struct Edit<'a>(&'a Path);
    impl Failpoints for Edit<'_> {
        fn after(&self, step: &str) -> Result<(), Interrupt> {
            if step == "copied" {
                let mut text = fs::read_to_string(self.0).unwrap();
                text.push_str("late\n");
                fs::write(self.0, text).unwrap();
            }
            Ok(())
        }
    }
    let repo = committed("f.txt", BASE.as_bytes());
    fs::write(repo.path().join("f.txt"), two_hunks()).unwrap();
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    let path = repo.path().join("f.txt");

    let end = discard(
        git,
        &repo,
        "f.txt",
        diff,
        Selection::new().hunk(0),
        &Edit(&path),
    );

    // The file is as Git left it, with the late line; the entry holds the file as
    // it was copied, and restoring it brings that back.
    assert!(
        matches!(end.outcome, DiscardOutcome::Failed { .. }),
        "{end:?}"
    );
    assert!(end.entry.is_some());
    assert_eq!(
        text(&repo, "f.txt"),
        format!("{}late\n", BASE.replace("l20\n", "l20\nadd A\nadd B\n"))
    );
    let entry = entries(git, &repo).remove(0);
    assert_eq!(entry.manifest().state, State::Discarded);
    assert_eq!(read(&entry.slot(0)), two_hunks().as_bytes());
}
