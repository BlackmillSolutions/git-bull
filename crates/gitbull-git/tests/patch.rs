//! Staging and unstaging hunks and lines with real Git. One test owns
//! process-wide configuration before any worker starts, like `tests/index.rs`.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use gitbull_git::cancel::CancelToken;
use gitbull_git::diff::{Content, FileDiff, LINE_CHARS, LineKind};
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::patch::{PartRequest, Selection, stage_part};
use gitbull_git::refusal::{Refusal, WriteFailure};
use gitbull_git::status::{Group, StatusEntry, status};
use gitbull_git::working_copy::working_diff;
use gitbull_git::{Error, Git};
use gitbull_testkit::{Marker, TestRepo};

#[test]
fn partial_index_operations_preserve_git_behaviour() {
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

    one_of_two_hunks_is_staged(&git);
    lines_are_staged_alone(&git);
    a_selection_reaches_over_two_hunks(&git);
    hunks_and_lines_are_unstaged_down_to_the_last(&git);
    a_later_hunk_is_unstaged_while_an_earlier_one_stays(&git);
    an_untracked_file_is_staged_in_parts(&git);
    an_intent_to_add_entry_is_staged_in_parts(&git);
    a_new_and_a_deleted_file_keep_their_path_while_lines_are_left(&git);
    a_part_of_a_rename_and_of_a_copy_moves_no_path(&git);
    a_file_that_is_emptied_or_filled_stays_modified(&git);
    an_end_without_a_line_break_is_staged_whole_or_refused(&git);
    whitespace_settings_do_not_touch_what_is_staged(&git);
    line_endings_are_staged_as_git_add_stores_them(&git);
    bytes_and_long_lines_are_staged_as_they_are(&git);
    only_the_file_named_changes(&git);
    an_empty_context_line_keeps_the_places_of_the_lines(&git);
    a_diff_out_of_date_changes_nothing(&git);
    an_untracked_file_staged_meanwhile_is_a_failure_of_git(&git);
    a_merge_stays_under_way(&git);
    no_changed_line_starts_no_git(&git);
    a_locked_index_fails_with_gits_message(&git);
    no_filter_runs_for_a_part(&git);
    files_under_a_filter_driver_are_marked(&git, &global);
    #[cfg(unix)]
    {
        an_executable_file_keeps_its_mode(&git);
        a_change_of_the_mode_is_staged_with_a_part_and_stays_when_one_is_unstaged(&git);
        a_symbolic_link_shows_its_mode(&git);
    }
}

/// A repository with one commit: `f.txt` of the lines `l01` to `l30`.
fn repository() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Patch Test");
    repo.config("user.email", "patch@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("maintenance.auto", "false");
    repo.config("gc.auto", "0");
    repo.write("f.txt", &numbered(1..=30));
    repo.commit("Initial");
    repo
}

fn numbered(lines: std::ops::RangeInclusive<u32>) -> String {
    lines.map(|n| format!("l{n:02}\n")).collect()
}

/// `f.txt` with `l02` replaced by `L02` and two lines added after `l20`:
/// two hunks.
fn two_hunks(repo: &TestRepo) {
    let text = numbered(1..=30)
        .replace("l02\n", "L02\n")
        .replace("l20\n", "l20\nadd A\nadd B\n");
    repo.write("f.txt", &text);
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
        let found = hunks.iter().enumerate().find_map(|(hunk, lines)| {
            let line = lines
                .lines
                .iter()
                .position(|line| line.kind == kind && line.text == text)?;
            Some((hunk, line))
        });
        let (hunk, line) = found.unwrap_or_else(|| panic!("no line {want}"));
        selection = selection.line(hunk, line);
    }
    selection
}

fn apply(
    git: &Git,
    repo: &TestRepo,
    group: Group,
    name: &str,
    shown: Arc<FileDiff>,
    selection: Selection,
) -> Result<(), WriteFailure> {
    let entry = entry(git, repo, group, name);
    let request = PartRequest { shown, selection };
    stage_part(
        git,
        repo.path(),
        group,
        &entry,
        &request,
        &CancelToken::new(),
    )
}

/// Stages, or unstages, the lines `wanted` of the diff of `name` as it is.
fn part(git: &Git, repo: &TestRepo, group: Group, name: &str, wanted: &[&str]) {
    let diff = shown(git, repo, group, name);
    let selection = lines(&diff, wanted);
    apply(git, repo, group, name, diff, selection).unwrap();
}

fn hunk(git: &Git, repo: &TestRepo, group: Group, name: &str, hunk: usize) {
    let diff = shown(git, repo, group, name);
    apply(git, repo, group, name, diff, Selection::new().hunk(hunk)).unwrap();
}

fn index(repo: &TestRepo, name: &str) -> String {
    repo.git(&["show", &format!(":{name}")])
}

fn short(repo: &TestRepo) -> Vec<String> {
    repo.git(&["status", "--porcelain", "--untracked-files=all"])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn read(repo: &TestRepo, name: &str) -> String {
    fs::read_to_string(repo.path().join(name)).unwrap()
}

fn one_of_two_hunks_is_staged(git: &Git) {
    let repo = repository();
    two_hunks(&repo);
    let working = read(&repo, "f.txt");
    hunk(git, &repo, Group::Unstaged, "f.txt", 0);
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30).replace("l02", "L02")
    );
    assert_eq!(short(&repo), ["MM f.txt"]);
    // The working copy is as it was, and the other hunk is still unstaged.
    assert_eq!(read(&repo, "f.txt"), working);
    let rest = repo.git(&["diff", "--", "f.txt"]);
    assert!(rest.contains("+add A") && !rest.contains("L02"), "{rest}");
    // The only hunk left: the file leaves the Unstaged group.
    hunk(git, &repo, Group::Unstaged, "f.txt", 0);
    assert_eq!(short(&repo), ["M  f.txt"]);
}

fn lines_are_staged_alone(git: &Git) {
    let repo = repository();
    two_hunks(&repo);
    // One added line of two.
    part(git, &repo, Group::Unstaged, "f.txt", &["+add B"]);
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30).replace("l20\n", "l20\nadd B\n")
    );
    // A removal alone: the index has neither `l02` nor `L02`.
    part(git, &repo, Group::Unstaged, "f.txt", &["-l02"]);
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30)
            .replace("l02\n", "")
            .replace("l20\n", "l20\nadd B\n")
    );
    let rest = repo.git(&["diff", "--", "f.txt"]);
    assert!(rest.contains("+L02") && rest.contains("+add A"), "{rest}");
}

fn a_selection_reaches_over_two_hunks(git: &Git) {
    let repo = repository();
    two_hunks(&repo);
    part(git, &repo, Group::Unstaged, "f.txt", &["+L02", "+add A"]);
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30)
            .replace("l02\n", "l02\nL02\n")
            .replace("l20\n", "l20\nadd A\n")
    );
}

fn hunks_and_lines_are_unstaged_down_to_the_last(git: &Git) {
    let repo = repository();
    two_hunks(&repo);
    repo.git(&["add", "f.txt"]);
    hunk(git, &repo, Group::Staged, "f.txt", 0);
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30).replace("l20\n", "l20\nadd A\nadd B\n")
    );
    part(git, &repo, Group::Staged, "f.txt", &["+add B"]);
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30).replace("l20\n", "l20\nadd A\n")
    );
    // The last staged line: the file leaves the Staged group.
    part(git, &repo, Group::Staged, "f.txt", &["+add A"]);
    assert_eq!(index(&repo, "f.txt"), numbered(1..=30));
    assert_eq!(short(&repo), [" M f.txt"]);
}

fn a_later_hunk_is_unstaged_while_an_earlier_one_stays(git: &Git) {
    // Blocks that repeat, so that a hunk placed wrongly still finds context.
    let block = "{\n  open\n  work\n  close\n}\n\n";
    let mut repo = repository();
    repo.write("blocks.txt", &block.repeat(8));
    repo.commit("Blocks");
    let top: String = (1..=11).map(|n| format!("top {n}\n")).collect();
    let mut blocks: Vec<String> = vec![block.to_owned(); 8];
    blocks[5] = block.replace("  work\n", "  work\n  flag\n");
    let staged = format!("{top}{}", blocks.concat());
    repo.write("blocks.txt", &staged);
    repo.git(&["add", "blocks.txt"]);
    // The second hunk adds `flag` in the sixth block.
    part(git, &repo, Group::Staged, "blocks.txt", &["+  flag"]);
    assert_eq!(
        index(&repo, "blocks.txt"),
        format!("{top}{}", block.repeat(8)),
        "exactly the line of the later hunk left the index"
    );
    assert_eq!(short(&repo), ["MM blocks.txt"]);
}

fn an_untracked_file_is_staged_in_parts(git: &Git) {
    let repo = repository();
    repo.write("my notes.txt", "a\nb\nc\n");
    part(git, &repo, Group::Untracked, "my notes.txt", &["+a"]);
    assert_eq!(index(&repo, "my notes.txt"), "a\n");
    assert_eq!(short(&repo), ["AM \"my notes.txt\""]);
    // The second time it is a tracked file with further lines.
    part(git, &repo, Group::Unstaged, "my notes.txt", &["+c"]);
    assert_eq!(index(&repo, "my notes.txt"), "a\nc\n");
    assert_eq!(read(&repo, "my notes.txt"), "a\nb\nc\n");

    // A hunk of an untracked file is all of it.
    repo.write("whole.txt", "x\ny\n");
    hunk(git, &repo, Group::Untracked, "whole.txt", 0);
    assert_eq!(index(&repo, "whole.txt"), "x\ny\n");
}

fn an_intent_to_add_entry_is_staged_in_parts(git: &Git) {
    let repo = repository();
    repo.write("later.txt", "a\nb\n");
    repo.git(&["add", "-N", "later.txt"]);
    part(git, &repo, Group::Unstaged, "later.txt", &["+b"]);
    assert_eq!(index(&repo, "later.txt"), "b\n");
}

fn a_new_and_a_deleted_file_keep_their_path_while_lines_are_left(git: &Git) {
    // One line of a staged new file unstaged: the file stays in the index.
    let repo = repository();
    repo.write("new.txt", "a\nb\nc\n");
    repo.git(&["add", "new.txt"]);
    part(git, &repo, Group::Staged, "new.txt", &["+b"]);
    assert_eq!(index(&repo, "new.txt"), "a\nc\n");
    // All of what is left: the file leaves the index.
    hunk(git, &repo, Group::Staged, "new.txt", 0);
    assert_eq!(short(&repo), ["?? new.txt"]);

    // A part of an unstaged deletion: the file stays with its other lines.
    let mut repo = repository();
    repo.write("gone.txt", "a\nb\nc\n");
    repo.commit("Gone");
    fs::remove_file(repo.path().join("gone.txt")).unwrap();
    part(git, &repo, Group::Unstaged, "gone.txt", &["-b"]);
    assert_eq!(index(&repo, "gone.txt"), "a\nc\n");
    // The rest of it: the deletion is staged.
    hunk(git, &repo, Group::Unstaged, "gone.txt", 0);
    assert_eq!(short(&repo), ["D  gone.txt"]);
    // One line of the staged deletion unstaged: the path is back with it.
    part(git, &repo, Group::Staged, "gone.txt", &["-b"]);
    assert_eq!(index(&repo, "gone.txt"), "b\n");
    assert_eq!(short(&repo), ["MD gone.txt"]);
}

fn a_part_of_a_rename_and_of_a_copy_moves_no_path(git: &Git) {
    let repo = repository();
    repo.git(&["mv", "f.txt", "new name.txt"]);
    let text = numbered(1..=30)
        .replace("l02\n", "L02\n")
        .replace("l20\n", "l20\nadd A\n");
    repo.write("new name.txt", &text);
    repo.git(&["add", "new name.txt"]);
    assert_eq!(short(&repo), ["R  f.txt -> \"new name.txt\""]);
    hunk(git, &repo, Group::Staged, "new name.txt", 0);
    // Still a rename, with the second hunk alone.
    assert_eq!(short(&repo), ["RM f.txt -> \"new name.txt\""]);
    assert_eq!(
        index(&repo, "new name.txt"),
        numbered(1..=30).replace("l20\n", "l20\nadd A\n")
    );

    let repo = repository();
    repo.config("status.renames", "copies");
    repo.write("f.txt", &numbered(1..=30).replace("l02\n", "L02\n"));
    let copy = numbered(1..=30)
        .replace("l02\n", "L02\n")
        .replace("l20\n", "l20\nin the copy\n");
    repo.write("copy.txt", &copy);
    repo.git(&["add", "f.txt", "copy.txt"]);
    assert_eq!(short(&repo), ["C  f.txt -> copy.txt", "M  f.txt"]);
    part(git, &repo, Group::Staged, "copy.txt", &["+in the copy"]);
    // The copy stays in the index, and its source keeps its staged change.
    assert_eq!(
        index(&repo, "copy.txt"),
        numbered(1..=30).replace("l02\n", "L02\n")
    );
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30).replace("l02\n", "L02\n")
    );
}

fn a_file_that_is_emptied_or_filled_stays_modified(git: &Git) {
    let mut repo = repository();
    repo.write("e.txt", "a\nb\n");
    repo.write("z.txt", "");
    repo.commit("Two more");
    repo.write("e.txt", "");
    repo.write("z.txt", "a\nb\n");
    hunk(git, &repo, Group::Unstaged, "e.txt", 0);
    part(git, &repo, Group::Unstaged, "z.txt", &["+b"]);
    // Emptied, the file is still in the index, empty.
    assert_eq!(index(&repo, "e.txt"), "");
    assert_eq!(index(&repo, "z.txt"), "b\n");
    assert_eq!(short(&repo), ["M  e.txt", "MM z.txt"]);
}

fn an_end_without_a_line_break_is_staged_whole_or_refused(git: &Git) {
    let mut repo = repository();
    repo.write("end.txt", "a\nb");
    repo.commit("No line break at the end");
    repo.write("end.txt", "a\nb\nc\n");
    // The added line alone would be joined with the line before it.
    let diff = shown(git, &repo, Group::Unstaged, "end.txt");
    let selection = lines(&diff, &["+c"]);
    let result = apply(git, &repo, Group::Unstaged, "end.txt", diff, selection);
    assert!(
        matches!(result, Err(WriteFailure::Refused(Refusal::EndOfFileSplit))),
        "{result:?}"
    );
    assert_eq!(index(&repo, "end.txt"), "a\nb");
    // All changed lines together are sound.
    hunk(git, &repo, Group::Unstaged, "end.txt", 0);
    assert_eq!(index(&repo, "end.txt"), "a\nb\nc\n");

    // The new side ends without a line break.
    let mut repo = repository();
    repo.write("end.txt", "a\nb\n");
    repo.commit("With a line break");
    repo.write("end.txt", "a\nb\nc");
    part(git, &repo, Group::Unstaged, "end.txt", &["+c"]);
    assert_eq!(index(&repo, "end.txt"), "a\nb\nc");
}

fn whitespace_settings_do_not_touch_what_is_staged(git: &Git) {
    for (key, value) in [
        ("apply.whitespace", "fix"),
        ("apply.whitespace", "error"),
        ("apply.ignoreWhitespace", "change"),
    ] {
        let repo = repository();
        repo.config(key, value);
        repo.write(
            "f.txt",
            &numbered(1..=30).replace("l20\n", "l20\nspaces  \n"),
        );
        part(git, &repo, Group::Unstaged, "f.txt", &["+spaces  "]);
        assert_eq!(
            index(&repo, "f.txt"),
            numbered(1..=30).replace("l20\n", "l20\nspaces  \n"),
            "{key}={value}"
        );
    }
}

fn line_endings_are_staged_as_git_add_stores_them(git: &Git) {
    // What `git add` stores of a file, as the hash of its blob.
    let stored_by_add = |repo: &TestRepo, name: &str| {
        let hash = repo.git(&["hash-object", "--path", name, "--", name]);
        hash.trim().to_owned()
    };
    let staged = |repo: &TestRepo, name: &str| {
        repo.git(&["rev-parse", &format!(":{name}")])
            .trim()
            .to_owned()
    };

    // CR LF in the working copy, converted on the way in.
    let repo = repository();
    repo.config("core.autocrlf", "true");
    let crlf = numbered(1..=30)
        .replace("l20\n", "l20\nadded\n")
        .replace('\n', "\r\n");
    repo.write("f.txt", &crlf);
    hunk(git, &repo, Group::Unstaged, "f.txt", 0);
    assert_eq!(staged(&repo, "f.txt"), stored_by_add(&repo, "f.txt"));
    assert!(!index(&repo, "f.txt").contains('\r'));

    // The same through an attribute.
    let repo = repository();
    repo.write_git_file("info/attributes", "*.txt text eol=crlf\n");
    repo.write("f.txt", &crlf);
    hunk(git, &repo, Group::Unstaged, "f.txt", 0);
    assert_eq!(staged(&repo, "f.txt"), stored_by_add(&repo, "f.txt"));
}

fn bytes_and_long_lines_are_staged_as_they_are(git: &Git) {
    let repo = repository();
    let long = "x".repeat(LINE_CHARS + 500);
    let mut content = numbered(1..=30).into_bytes();
    content.extend_from_slice(b"caf\xe9 \xff\n");
    content.extend_from_slice(long.as_bytes());
    content.push(b'\n');
    fs::write(repo.path().join("f.txt"), &content).unwrap();
    // What the diff shows of both lines is not what the file holds.
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    let Content::Text(hunks) = &diff.content else {
        panic!("no text");
    };
    let added: Vec<usize> = hunks[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.kind == LineKind::Added)
        .map(|(at, _)| at)
        .collect();
    assert_eq!(added.len(), 2);
    assert!(hunks[0].lines[added[1]].cut, "the long line is shown cut");
    let selection = Selection::new().line(0, added[0]).line(0, added[1]);
    apply(git, &repo, Group::Unstaged, "f.txt", diff, selection).unwrap();
    let hash = |args: &[&str]| repo.git(args).trim().to_owned();
    assert_eq!(
        hash(&["rev-parse", ":f.txt"]),
        hash(&["hash-object", "--", "f.txt"])
    );
}

fn only_the_file_named_changes(git: &Git) {
    let mut repo = repository();
    repo.write("my notes [1].txt", "a\nb\n");
    repo.commit("A path with a space and a bracket");
    repo.write("my notes [1].txt", "a\nb\nc\n");
    two_hunks(&repo);
    hunk(git, &repo, Group::Unstaged, "my notes [1].txt", 0);
    assert_eq!(index(&repo, "my notes [1].txt"), "a\nb\nc\n");
    assert_eq!(short(&repo), [" M f.txt", "M  \"my notes [1].txt\""]);
}

fn an_empty_context_line_keeps_the_places_of_the_lines(git: &Git) {
    let mut repo = repository();
    repo.config("diff.suppressBlankEmpty", "true");
    repo.write("blank.txt", "a\n\nb\n\nc\n");
    repo.commit("Empty lines");
    repo.write("blank.txt", "a\n\nB\n\nc\nd\n");
    // The added line below the empty context lines, and no other.
    part(git, &repo, Group::Unstaged, "blank.txt", &["+d"]);
    assert_eq!(index(&repo, "blank.txt"), "a\n\nb\n\nc\nd\n");
}

fn a_diff_out_of_date_changes_nothing(git: &Git) {
    let out_of_date = |result: Result<(), WriteFailure>, why: &str| {
        assert!(
            matches!(result, Err(WriteFailure::Refused(Refusal::DiffOutOfDate))),
            "{why}: {result:?}"
        );
    };

    // An editor adds a hunk.
    let repo = repository();
    repo.write("f.txt", &numbered(1..=30).replace("l20\n", "l20\nadd A\n"));
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    two_hunks(&repo);
    let result = apply(
        git,
        &repo,
        Group::Unstaged,
        "f.txt",
        diff,
        Selection::new().hunk(0),
    );
    out_of_date(result, "a hunk was added");
    assert_eq!(index(&repo, "f.txt"), numbered(1..=30));

    // A line is edited in place: the same places, another text.
    let repo = repository();
    repo.write("f.txt", &numbered(1..=30).replace("l20\n", "l20\nfoo\n"));
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    repo.write("f.txt", &numbered(1..=30).replace("l20\n", "l20\nbar\n"));
    let result = apply(
        git,
        &repo,
        Group::Unstaged,
        "f.txt",
        diff,
        Selection::new().hunk(0),
    );
    out_of_date(result, "a line was edited in place");
    assert_eq!(index(&repo, "f.txt"), numbered(1..=30));

    // `git add` ran meanwhile: no diff of the file is left.
    let repo = repository();
    two_hunks(&repo);
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    let entry = entry(git, &repo, Group::Unstaged, "f.txt");
    repo.git(&["add", "f.txt"]);
    let staged = index(&repo, "f.txt");
    let request = PartRequest {
        shown: diff,
        selection: Selection::new().hunk(0),
    };
    let result = stage_part(
        git,
        repo.path(),
        Group::Unstaged,
        &entry,
        &request,
        &CancelToken::new(),
    );
    out_of_date(result, "the file was staged meanwhile");
    assert_eq!(index(&repo, "f.txt"), staged);
}

fn an_untracked_file_staged_meanwhile_is_a_failure_of_git(git: &Git) {
    // Its diff against nothing is the same as before, so it is not seen as
    // out of date: Git refuses the patch of a new file for a path it has.
    let repo = repository();
    repo.write("new.txt", "a\nb\n");
    let diff = shown(git, &repo, Group::Untracked, "new.txt");
    let entry = entry(git, &repo, Group::Untracked, "new.txt");
    repo.git(&["add", "new.txt"]);
    let request = PartRequest {
        shown: Arc::clone(&diff),
        selection: lines(&diff, &["+a"]),
    };
    let result = stage_part(
        git,
        repo.path(),
        Group::Untracked,
        &entry,
        &request,
        &CancelToken::new(),
    );
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { .. }))
        ),
        "{result:?}"
    );
    assert_eq!(index(&repo, "new.txt"), "a\nb\n");
}

fn a_merge_stays_under_way(git: &Git) {
    let mut repo = repository();
    repo.write("other.txt", "one\n");
    repo.commit("Another file");
    repo.git(&["switch", "-q", "-c", "theirs"]);
    repo.write("other.txt", "theirs\n");
    repo.commit("Theirs");
    repo.git(&["switch", "-q", "main"]);
    repo.write("other.txt", "ours\n");
    repo.commit("Ours");
    assert!(repo.try_git(&["merge", "theirs"]).is_err());
    two_hunks(&repo);
    hunk(git, &repo, Group::Unstaged, "f.txt", 0);
    assert_eq!(short(&repo), ["MM f.txt", "UU other.txt"]);
    assert!(repo.path().join(".git/MERGE_HEAD").exists());
}

fn no_changed_line_starts_no_git(git: &Git) {
    let repo = repository();
    two_hunks(&repo);
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    let entry = entry(git, &repo, Group::Unstaged, "f.txt");
    // A context line, and a hunk that is not there.
    let request = PartRequest {
        shown: diff,
        selection: Selection::new().line(0, 0).hunk(9),
    };
    // With an executable that does not exist, starting it would fail.
    let none = Git::new(
        repo.path().join("no-such-git"),
        repo.path().join("no-hooks"),
    );
    stage_part(
        &none,
        repo.path(),
        Group::Unstaged,
        &entry,
        &request,
        &CancelToken::new(),
    )
    .unwrap();
    assert_eq!(index(&repo, "f.txt"), numbered(1..=30));
}

fn a_locked_index_fails_with_gits_message(git: &Git) {
    let repo = repository();
    two_hunks(&repo);
    let diff = shown(git, &repo, Group::Unstaged, "f.txt");
    let entry = entry(git, &repo, Group::Unstaged, "f.txt");
    repo.write_git_file("index.lock", "");
    let request = PartRequest {
        shown: diff,
        selection: Selection::new().hunk(0),
    };
    let result = stage_part(
        git,
        repo.path(),
        Group::Unstaged,
        &entry,
        &request,
        &CancelToken::new(),
    );
    assert!(
        matches!(
            &result,
            Err(WriteFailure::Failed(Error::CommandFailed { stderr, .. }))
                if stderr.contains("index.lock")
        ),
        "{result:?}"
    );
    fs::remove_file(repo.path().join(".git/index.lock")).unwrap();
    assert_eq!(index(&repo, "f.txt"), numbered(1..=30));
}

fn install_filter(repo: &TestRepo, marker: &Marker, pattern: &str) {
    let script = marker.script("clean", "cat");
    repo.config("filter.test.clean", &format!("'{script}'"));
    repo.write_git_file("info/attributes", &format!("{pattern} filter=test\n"));
}

fn no_filter_runs_for_a_part(git: &Git) {
    // The filter is on another file than the one staged: reading the diff
    // and applying the patch must not run it.
    let mut repo = repository();
    repo.write("big.dat", "raw\n");
    repo.commit("A file for the filter");
    let marker = Marker::new();
    install_filter(&repo, &marker, "*.dat");
    repo.write("big.dat", "raw changed\n");
    two_hunks(&repo);
    hunk(git, &repo, Group::Unstaged, "f.txt", 0);
    assert!(marker.labels().is_empty(), "{:?}", marker.labels());
}

fn files_under_a_filter_driver_are_marked(git: &Git, global: &Path) {
    // A filter of the repository: marked in both groups.
    let mut repo = repository();
    repo.write("big.dat", "raw\n");
    repo.commit("A file for the filter");
    let marker = Marker::new();
    install_filter(&repo, &marker, "*.dat");
    repo.write("big.dat", "raw changed\n");
    two_hunks(&repo);
    assert!(shown(git, &repo, Group::Unstaged, "big.dat").filtered);
    assert!(!shown(git, &repo, Group::Unstaged, "f.txt").filtered);
    repo.git(&["add", "big.dat", "f.txt"]);
    assert!(shown(git, &repo, Group::Staged, "big.dat").filtered);
    assert!(!shown(git, &repo, Group::Staged, "f.txt").filtered);

    // An attribute that names a driver nobody configured, one that is unset,
    // and a driver with an empty command: not marked.
    let repo = repository();
    repo.config("filter.off.clean", "");
    repo.write_git_file(
        "info/attributes",
        "f.txt filter=nowhere\nu.txt -filter\no.txt filter=off\n",
    );
    for name in ["u.txt", "o.txt"] {
        repo.write(name, "a\n");
    }
    two_hunks(&repo);
    assert!(!shown(git, &repo, Group::Unstaged, "f.txt").filtered);
    assert!(!shown(git, &repo, Group::Untracked, "u.txt").filtered);
    assert!(!shown(git, &repo, Group::Untracked, "o.txt").filtered);

    // A filter of the user's own configuration, as Git LFS: marked too.
    let before = fs::read_to_string(global).unwrap();
    fs::write(
        global,
        format!("{before}[filter \"mine\"]\n clean = cat\n smudge = cat\n"),
    )
    .unwrap();
    let repo = repository();
    repo.write_git_file("info/attributes", "f.txt filter=mine\n");
    two_hunks(&repo);
    let marked = shown(git, &repo, Group::Unstaged, "f.txt").filtered;
    fs::write(global, before).unwrap();
    assert!(marked, "a filter of the global configuration");
}

#[cfg(unix)]
fn an_executable_file_keeps_its_mode(git: &Git) {
    use std::os::unix::fs::PermissionsExt;
    let repo = repository();
    repo.write("run.sh", "a\nb\nc\n");
    let file = repo.path().join("run.sh");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    part(git, &repo, Group::Untracked, "run.sh", &["+a", "+c"]);
    assert!(
        repo.git(&["ls-files", "--stage", "run.sh"])
            .starts_with("100755 ")
    );
    assert_eq!(index(&repo, "run.sh"), "a\nc\n");
}

#[cfg(unix)]
fn a_change_of_the_mode_is_staged_with_a_part_and_stays_when_one_is_unstaged(git: &Git) {
    use std::os::unix::fs::PermissionsExt;
    let repo = repository();
    two_hunks(&repo);
    let file = repo.path().join("f.txt");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    let mode = |repo: &TestRepo| repo.git(&["ls-files", "--stage", "f.txt"])[..6].to_owned();
    // Staging one line takes the mode along, as Git applies it.
    part(git, &repo, Group::Unstaged, "f.txt", &["+add A"]);
    assert_eq!(mode(&repo), "100755");
    part(git, &repo, Group::Unstaged, "f.txt", &["+add B"]);
    // Unstaging one line leaves the mode staged, with the other line.
    part(git, &repo, Group::Staged, "f.txt", &["+add A"]);
    assert_eq!(mode(&repo), "100755");
    assert_eq!(
        index(&repo, "f.txt"),
        numbered(1..=30).replace("l20\n", "l20\nadd B\n")
    );
}

#[cfg(unix)]
fn a_symbolic_link_shows_its_mode(git: &Git) {
    // What the interface decides "staged whole only" from.
    let mut repo = repository();
    std::os::unix::fs::symlink("f.txt", repo.path().join("link")).unwrap();
    repo.git(&["add", "link"]);
    repo.commit("A link");
    fs::remove_file(repo.path().join("link")).unwrap();
    std::os::unix::fs::symlink("elsewhere", repo.path().join("link")).unwrap();
    let diff = shown(git, &repo, Group::Unstaged, "link");
    let link = Some("120000".to_owned());
    assert!(
        diff.old_mode == link || diff.new_mode == link,
        "{:?} {:?}",
        diff.old_mode,
        diff.new_mode
    );
    // A file that became a symbolic link is reported as a change of type.
    fs::remove_file(repo.path().join("f.txt")).unwrap();
    std::os::unix::fs::symlink("elsewhere", repo.path().join("f.txt")).unwrap();
    let entry = entry(git, &repo, Group::Unstaged, "f.txt");
    assert_eq!(
        entry.kind,
        gitbull_git::status::StatusKind::Changed(gitbull_git::changes::ChangeKind::TypeChanged)
    );
}
