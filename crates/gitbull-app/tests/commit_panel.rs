//! The commit panel: details of the selected commit and its changed files
//! (spec `commit-details`).

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{CursorIcon, Event, Modifiers, OutputCommand, PointerButton, Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_app::icons;
use gitbull_core::settings::{Layout, Settings};
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::status::{StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, fake_id};
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};
use support::{Setup, build, drag_by, path, settle_window, wait_for_references, window};

fn seconds(text: &str) -> i64 {
    text.parse::<Timestamp>().unwrap().as_second()
}

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn person(name: &str, when: &str) -> Signature {
    Signature {
        name: name.to_owned(),
        email: format!("{}@example.com", name.to_lowercase().replace(' ', ".")),
        time: seconds(when),
        offset_minutes: 0,
    }
}

fn line(name: &str, parents: &[&str], when: &str) -> CommitLine {
    CommitLine {
        timestamp: seconds(when),
        id: fake_id(name),
        parents: parents.iter().map(|p| fake_id(p)).collect(),
    }
}

fn change(kind: ChangeKind, path: &str, old_path: Option<&str>) -> FileChange {
    FileChange {
        kind,
        path: path.into(),
        old_path: old_path.map(Into::into),
    }
}

const C_DATE: &str = "2026-09-29T09:00:00Z";
const B_AUTHORED: &str = "2026-01-10T09:00:00Z";
const B_COMMITTED: &str = "2026-02-20T15:30:00Z";
const A_DATE: &str = "2025-12-01T08:00:00Z";

/// c on top of b on top of a, with main on c. b was written by one person
/// and committed by another.
fn backend() -> FakeBackend {
    FakeBackend::default()
        .with_repository(root())
        .with_history(
            root(),
            vec![
                line("c", &["b"], C_DATE),
                line("b", &["a"], B_COMMITTED),
                line("a", &[], A_DATE),
            ],
        )
        .with_references(
            root(),
            vec![Reference {
                name: "refs/heads/main".to_owned(),
                short: "main".to_owned(),
                kind: RefKind::Branch,
                commit: Some(fake_id("c").to_string()),
                upstream: None,
            }],
        )
        .with_content(
            fake_id("c"),
            CommitContent {
                author: person("Ada Lovelace", C_DATE),
                committer: person("Ada Lovelace", C_DATE),
                message: "Fix the parser\n\nThe body explains why.\n".to_owned(),
            },
        )
        .with_content(
            fake_id("b"),
            CommitContent {
                author: person("Grace Hopper", B_AUTHORED),
                committer: person("Linus Torvalds", B_COMMITTED),
                message: "Rebased change\n\nApplied by the maintainer.\n".to_owned(),
            },
        )
        .with_content(
            fake_id("a"),
            CommitContent {
                author: person("Alan Turing", A_DATE),
                committer: person("Alan Turing", A_DATE),
                message: "First commit\n\nThe start.\n".to_owned(),
            },
        )
        .with_changes(
            fake_id("c"),
            vec![
                change(ChangeKind::Modified, "src/parser.rs", None),
                change(ChangeKind::Renamed, "src/b.rs", Some("src/a.rs")),
                change(ChangeKind::Deleted, "old.txt", None),
            ],
        )
        .with_changes(
            fake_id("a"),
            vec![
                change(ChangeKind::Added, "README.md", None),
                change(ChangeKind::Added, "src/main.rs", None),
            ],
        )
}

fn open(backend: FakeBackend) -> Harness<'static, App> {
    open_with(backend, Layout::default())
}

fn open_with(backend: FakeBackend, layout: Layout) -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            layout,
            ..Settings::default()
        },
        backend,
        time_zone: Some(TimeZone::fixed(Offset::constant(2))),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| commit_row(h, "Fix the parser").is_some());
    wait_for_references(&mut harness);
    harness
}

fn wait_until(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

/// The row of a commit in the commit list, whose label starts with its
/// summary.
fn commit_row<'a>(harness: &'a Harness<'_, App>, summary: &str) -> Option<egui_kittest::Node<'a>> {
    harness.query_all_by_role(Role::Row).find(|node| {
        node.accesskit_node()
            .label()
            .is_some_and(|label| label.starts_with(summary))
    })
}

fn click_at(harness: &mut Harness<'_, App>, at: eframe::egui::Pos2, button: PointerButton) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

/// Selects a commit in the list and waits for its details.
fn select(harness: &mut Harness<'_, App>, summary: &str) {
    let at = commit_row(harness, summary).expect("row").rect().center();
    click_at(harness, at, PointerButton::Primary);
    wait_until(harness, |h| !panel_texts(h).iter().any(|t| t == "Loading…"));
}

/// The texts of the widgets inside `area`: labels carry theirs as a
/// value, other widgets as a label. An empty panel is a pane that only
/// repeats its title for assistive technology, which shows no text.
fn texts_in(harness: &Harness<'_, App>, area: eframe::egui::Rect) -> Vec<String> {
    harness
        .query_all_by(|node| {
            !matches!(node.role(), Role::TextRun | Role::Pane)
                && (node.label().is_some() || node.value().is_some())
        })
        .filter(|node| area.contains_rect(node.rect()))
        .filter_map(|node| {
            let node = node.accesskit_node();
            node.label().or_else(|| node.value())
        })
        .collect()
}

/// The texts in the commit panel: below its title and left of the diff.
fn panel_texts(harness: &Harness<'_, App>) -> Vec<String> {
    let title = harness.get_by_role_and_label(Role::Label, "COMMIT").rect();
    let diff = harness.get_by_role_and_label(Role::Label, "DIFF").rect();
    let area = eframe::egui::Rect::from_min_max(
        eframe::egui::pos2(title.left() - 12.0, title.top()),
        eframe::egui::pos2(diff.left() - 4.0, f32::INFINITY),
    );
    texts_in(harness, area)
}

/// The texts in the diff panel: below its title and above the status bar.
fn diff_texts(harness: &Harness<'_, App>) -> Vec<String> {
    let diff = harness.get_by_role_and_label(Role::Label, "DIFF").rect();
    let status_bar = harness
        .query_all_by_value("Git 2.55.0")
        .next()
        .expect("the status bar")
        .rect();
    let area = eframe::egui::Rect::from_min_max(
        eframe::egui::pos2(diff.left() - 4.0, diff.top()),
        eframe::egui::pos2(f32::INFINITY, status_bar.top()),
    );
    texts_in(harness, area)
}

fn has_files(harness: &Harness<'_, App>) -> bool {
    harness.query_all_by_role(Role::ListItem).next().is_some()
}

fn file<'a>(harness: &'a Harness<'_, App>, label: &'a str) -> egui_kittest::Node<'a> {
    harness.get_by_role_and_label(Role::ListItem, label)
}

fn copied(harness: &Harness<'_, App>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

#[test]
fn a_selected_commit_shows_hash_message_people_dates_references_and_parents() {
    let mut harness = open(backend());
    select(&mut harness, "Fix the parser");
    let texts = panel_texts(&harness);
    for expected in [
        fake_id("c").to_string(),
        "Fix the parser\n\nThe body explains why.".to_owned(),
        "Ada Lovelace <ada.lovelace@example.com>".to_owned(),
        // 09:00 UTC in the local zone, two hours east.
        "2026-09-29 11:00".to_owned(),
        "main".to_owned(),
        fake_id("b").short(10),
    ] {
        assert!(
            texts.contains(&expected),
            "{expected:?} missing in {texts:?}"
        );
    }
    // Author and committer, each with a name and a date.
    let names = texts
        .iter()
        .filter(|t| t.starts_with("Ada Lovelace"))
        .count();
    assert_eq!(names, 2, "{texts:?}");
}

#[test]
fn when_author_and_committer_differ_both_names_and_dates_are_shown() {
    let mut harness = open(backend());
    select(&mut harness, "Rebased change");
    let texts = panel_texts(&harness);
    for expected in [
        "Grace Hopper <grace.hopper@example.com>",
        "Linus Torvalds <linus.torvalds@example.com>",
        "2026-01-10 11:00",
        "2026-02-20 17:30",
    ] {
        assert!(
            texts.iter().any(|t| t == expected),
            "{expected:?} missing in {texts:?}"
        );
    }
}

#[test]
fn with_nothing_selected_both_panels_are_empty() {
    let harness = open(backend());
    assert_eq!(panel_texts(&harness), ["COMMIT"]);
    assert_eq!(diff_texts(&harness), ["DIFF"]);
}

#[test]
fn choosing_a_parent_selects_it_in_the_commit_list() {
    let mut harness = open_with(backend(), roomy());
    select(&mut harness, "Fix the parser");
    let link = harness
        .get_by_label(&fake_id("b").short(10))
        .rect()
        .center();
    click_at(&mut harness, link, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        commit_row(h, "Rebased change")
            .is_some_and(|row| row.accesskit_node().is_selected() == Some(true))
    });
    assert!(panel_texts(&harness).contains(&fake_id("b").to_string()));
}

#[test]
fn a_parent_far_down_the_list_is_scrolled_into_view() {
    // A merge whose second parent is 300 rows further down.
    let mut lines = vec![line("merge", &["m0", "side"], C_DATE)];
    lines.extend((0..300).map(|n| {
        let parent = format!("m{}", n + 1);
        line(&format!("m{n}"), &[parent.as_str()], C_DATE)
    }));
    lines.push(line("m300", &["side"], C_DATE));
    lines.push(line("side", &[], A_DATE));
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), lines)
        .with_content(
            fake_id("merge"),
            CommitContent {
                author: person("Ada Lovelace", C_DATE),
                committer: person("Ada Lovelace", C_DATE),
                message: "Merge the side\n".to_owned(),
            },
        )
        .with_content(
            fake_id("side"),
            CommitContent {
                author: person("Ada Lovelace", A_DATE),
                committer: person("Ada Lovelace", A_DATE),
                message: "Side work\n".to_owned(),
            },
        );
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| commit_row(h, "Merge the side").is_some());
    select(&mut harness, "Merge the side");

    let link = harness
        .get_by_label(&fake_id("side").short(10))
        .rect()
        .center();
    click_at(&mut harness, link, PointerButton::Primary);
    wait_until(&mut harness, |h| {
        commit_row(h, "Side work")
            .is_some_and(|row| row.accesskit_node().is_selected() == Some(true))
    });
}

#[test]
fn the_files_show_their_kind_of_change_and_both_paths_of_a_rename() {
    let mut harness = open(backend());
    select(&mut harness, "Fix the parser");
    wait_until(&mut harness, has_files);
    let labels: Vec<String> = harness
        .get_all_by_role(Role::ListItem)
        .filter_map(|node| node.accesskit_node().label())
        .collect();
    assert_eq!(
        labels,
        [
            "Modified: src/parser.rs",
            "Renamed: src/a.rs → src/b.rs",
            "Deleted: old.txt",
        ]
    );
}

#[test]
fn a_root_commit_lists_its_files_as_added() {
    let mut harness = open(backend());
    select(&mut harness, "First commit");
    wait_until(&mut harness, has_files);
    file(&harness, "Added: README.md");
    file(&harness, "Added: src/main.rs");
}

#[test]
fn the_first_file_is_selected() {
    let mut harness = open(backend());
    select(&mut harness, "Fix the parser");
    wait_until(&mut harness, has_files);
    let selected = |label| file(&harness, label).accesskit_node().is_selected();
    assert_eq!(selected("Modified: src/parser.rs"), Some(true));
    assert_eq!(selected("Deleted: old.txt"), Some(false));
}

#[test]
fn a_commit_without_changed_files_says_so_and_the_diff_stays_empty() {
    let mut harness = open(backend());
    select(&mut harness, "Rebased change");
    wait_until(&mut harness, |h| {
        h.query_by_label("No files changed.").is_some()
    });
    assert!(!has_files(&harness));
    assert_eq!(diff_texts(&harness), ["DIFF"]);
}

#[test]
fn the_context_menu_copies_the_path_of_a_file() {
    let mut harness = open(backend());
    select(&mut harness, "Fix the parser");
    wait_until(&mut harness, has_files);
    let at = file(&harness, "Renamed: src/a.rs → src/b.rs")
        .rect()
        .center();
    click_at(&mut harness, at, PointerButton::Secondary);
    assert!(harness.query_by_label("File history").is_some());
    assert!(harness.query_by_label("Blame").is_some());
    harness.get_by_label("Copy path").click();
    harness.step();
    assert_eq!(copied(&harness), Some("src/b.rs".to_owned()));
}

#[test]
fn control_c_copies_the_path_of_the_selected_file() {
    let mut harness = open(backend());
    select(&mut harness, "Fix the parser");
    wait_until(&mut harness, has_files);
    let at = file(&harness, "Deleted: old.txt").rect().center();
    click_at(&mut harness, at, PointerButton::Primary);
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key: eframe::egui::Key::C,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        });
    }
    harness.step();
    assert_eq!(copied(&harness), Some("old.txt".to_owned()));
}

#[test]
fn a_commit_with_many_references_shows_the_first_and_counts_the_rest() {
    let tags = (0..100).map(|n| Reference {
        name: format!("refs/tags/v{n:03}"),
        short: format!("v{n:03}"),
        kind: RefKind::Tag,
        commit: Some(fake_id("a").to_string()),
        upstream: None,
    });
    let mut harness = open(backend().with_references(root(), tags.collect()));
    select(&mut harness, "First commit");
    let texts = panel_texts(&harness);
    let badges = texts.iter().filter(|t| t.starts_with('v')).count();
    assert_eq!(badges, 20, "{texts:?}");
    assert!(texts.iter().any(|t| t == "+80"), "{texts:?}");
}

#[test]
fn each_kind_of_reference_shows_its_icon_in_its_badge() {
    let reference = |name: &str, short: &str, kind| Reference {
        name: name.to_owned(),
        short: short.to_owned(),
        kind,
        commit: Some(fake_id("c").to_string()),
        upstream: None,
    };
    let references = vec![
        reference("refs/heads/main", "main", RefKind::Branch),
        reference(
            "refs/remotes/origin/main",
            "origin/main",
            RefKind::RemoteBranch,
        ),
        reference("refs/tags/v1.0", "v1.0", RefKind::Tag),
    ];
    // Room for every field without scrolling them.
    let tall = Layout {
        details_height: Some(420.0),
        ..Layout::default()
    };
    let mut harness = open_with(backend().with_references(root(), references), tall);
    select(&mut harness, "Fix the parser");
    let field = harness.get_by_label("References").rect();
    for (name, icon) in [
        ("HEAD", icons::HEAD),
        ("main", icons::BRANCH),
        ("origin/main", icons::REMOTE_BRANCH),
        ("v1.0", icons::TAG),
    ] {
        // The badge in the field of the panel; the list and the status bar
        // name the branch too.
        let label = harness
            .get_all_by_label(name)
            .map(|node| node.rect())
            .find(|rect| (rect.center().y - field.center().y).abs() < 12.0)
            .unwrap_or_else(|| panic!("no badge {name} in the panel"));
        // The icon stands left of the name inside the badge.
        let badge = eframe::egui::Rect::from_min_max(
            label.left_top() - eframe::egui::vec2(24.0, 4.0),
            label.right_bottom() + eframe::egui::vec2(0.0, 4.0),
        );
        let texts = support::texts_in(harness.output(), badge);
        assert!(texts.iter().any(|text| text == icon), "{name}: {texts:?}");
    }
}

#[test]
fn choosing_another_commit_closes_the_context_menu_of_a_file() {
    let mut harness = open(backend().with_changes(
        fake_id("b"),
        vec![
            change(ChangeKind::Added, "one.txt", None),
            change(ChangeKind::Added, "two.txt", None),
        ],
    ));
    select(&mut harness, "Fix the parser");
    wait_until(&mut harness, has_files);
    let at = file(&harness, "Renamed: src/a.rs → src/b.rs")
        .rect()
        .center();
    click_at(&mut harness, at, PointerButton::Secondary);
    assert!(harness.query_by_label("Copy path").is_some());

    // With the menu open, the keyboard moves the selection of the commit
    // list to the next commit. Its files load afresh, so the menu cannot
    // act on a file of the other commit.
    harness.key_press_modifiers(Modifiers::SHIFT, eframe::egui::Key::Tab);
    harness.run();
    harness.key_press(eframe::egui::Key::ArrowDown);
    harness.run();
    wait_until(&mut harness, |h| {
        h.query_all_by_role(Role::ListItem)
            .any(|node| node.accesskit_node().label().as_deref() == Some("Added: two.txt"))
    });
    assert!(harness.query_by_label("Copy path").is_none());
}

/// The list of changed files, which starts below the divider.
fn file_list(harness: &Harness<'_, App>) -> Rect {
    harness.get_by_role_and_label(Role::List, "COMMIT").rect()
}

/// The divider between the details and the files: the filter field above
/// the list starts 6 points below it. A press on it takes the divider to
/// the pointer.
fn divider(harness: &Harness<'_, App>) -> Pos2 {
    let list = file_list(harness);
    let field = harness
        .get_by_role_and_label(Role::TextInput, FILTER)
        .rect();
    Pos2::new(list.center().x, field.top() - 6.0)
}

/// Room for the details to grow and shrink: a panel 500 points high, with
/// the details 200 high in it.
fn roomy() -> Layout {
    Layout {
        details_height: Some(500.0),
        commit_details_height: Some(200.0),
        ..Layout::default()
    }
}

#[test]
fn dragging_the_divider_below_the_details_moves_it_and_keeps_its_height() {
    let mut harness = open_with(backend(), roomy());
    select(&mut harness, "Fix the parser");
    let before = file_list(&harness);
    let at = divider(&harness);
    drag_by(&mut harness, at, vec2(0.0, 40.0));

    let after = file_list(&harness);
    assert!(
        (after.top() - before.top() - 40.0).abs() < 1.0,
        "{before:?} {after:?}"
    );
    assert!(
        (after.bottom() - before.bottom()).abs() < 1.0,
        "{before:?} {after:?}"
    );
    let kept = harness.state().settings().layout.commit_details_height;
    assert!(
        kept.is_some_and(|height| (height - 240.0).abs() < 1.0),
        "{kept:?}"
    );
}

#[test]
fn a_saved_height_of_the_details_is_used_at_start() {
    let tops = [150.0, 200.0].map(|height| {
        let layout = Layout {
            commit_details_height: Some(height),
            ..roomy()
        };
        let mut harness = open_with(backend(), layout);
        select(&mut harness, "Fix the parser");
        file_list(&harness).top()
    });
    assert!((tops[1] - tops[0] - 50.0).abs() < 0.5, "{tops:?}");
}

#[test]
fn the_divider_stays_where_it_was_for_a_message_of_one_line() {
    let long = (1..=20)
        .map(|n| {
            format!(
                "Line {n} of the body.
"
            )
        })
        .collect::<String>();
    let backend = backend()
        .with_content(
            fake_id("c"),
            CommitContent {
                author: person("Ada Lovelace", C_DATE),
                committer: person("Ada Lovelace", C_DATE),
                message: format!(
                    "Fix the parser

{long}"
                ),
            },
        )
        .with_content(
            fake_id("a"),
            CommitContent {
                author: person("Alan Turing", A_DATE),
                committer: person("Alan Turing", A_DATE),
                message: "First commit
"
                .to_owned(),
            },
        );
    let mut harness = open_with(backend, roomy());
    select(&mut harness, "Fix the parser");
    let long_message = file_list(&harness).top();
    select(&mut harness, "First commit");
    wait_until(&mut harness, has_files);
    assert!((file_list(&harness).top() - long_message).abs() < 0.5);
}

#[test]
fn the_divider_keeps_room_for_four_files() {
    let mut harness = open_with(backend(), roomy());
    select(&mut harness, "Fix the parser");
    let at = divider(&harness);
    drag_by(&mut harness, at, vec2(0.0, 1000.0));
    let list = file_list(&harness);
    assert!(list.height() >= 4.0 * 24.0 - 1.0, "{list:?}");
}

#[test]
fn the_pointer_over_the_divider_shows_that_it_can_be_dragged() {
    let mut harness = open_with(backend(), roomy());
    select(&mut harness, "Fix the parser");
    let at = divider(&harness);
    harness.hover_at(at);
    harness.run();
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        CursorIcon::ResizeVertical
    );
}

/// Whether the pointer finds a divider to drag anywhere in the commit
/// panel below its title.
fn divider_found(harness: &mut Harness<'_, App>) -> bool {
    let title = harness.get_by_role_and_label(Role::Label, "COMMIT").rect();
    let diff = harness.get_by_role_and_label(Role::Label, "DIFF").rect();
    let x = (title.left() + diff.left()) / 2.0;
    let mut y = title.bottom();
    while y < title.top() + 500.0 {
        harness.hover_at(Pos2::new(x, y));
        harness.run();
        if harness.output().platform_output.cursor_icon == CursorIcon::ResizeVertical {
            return true;
        }
        y += 2.0;
    }
    false
}

#[test]
fn the_row_of_uncommitted_changes_shows_no_divider() {
    let status = WorkingStatus {
        unstaged: vec![StatusEntry {
            kind: StatusKind::Changed(ChangeKind::Modified),
            path: RepoPath::new("edit.txt"),
            old_path: None,
            submodule: false,
        }],
        ..WorkingStatus::default()
    };
    let mut harness = open_with(backend().with_status(root(), status), roomy());
    wait_until(&mut harness, |h| {
        commit_row(h, "Uncommitted changes").is_some()
    });
    select(&mut harness, "Fix the parser");
    assert!(divider_found(&mut harness), "a commit shows the divider");

    // A click on the row opens the File status view; the keyboard only
    // selects it.
    harness.key_press(eframe::egui::Key::ArrowUp);
    harness.run();
    wait_until(&mut harness, |h| {
        h.query_by_label("Open File status").is_some()
    });
    assert!(!divider_found(&mut harness));
}

// The file list as a tree and with a filter (spec `file-lists`).

const FILTER: &str = "Filter files";
const TREE: &str = "Show as tree";

/// The newest commit changes two files of `src/app` and the readme; the
/// root commit adds the readme and `src/main.rs`.
fn tree_backend() -> FakeBackend {
    backend().with_changes(
        fake_id("c"),
        vec![
            change(ChangeKind::Modified, "src/app/main.rs", None),
            change(ChangeKind::Modified, "src/app/view.rs", None),
            change(ChangeKind::Added, "README.md", None),
        ],
    )
}

/// Opens `backend` with room for the files and selects its newest commit.
fn open_newest(backend: FakeBackend) -> Harness<'static, App> {
    let mut harness = open_with(backend, roomy());
    select(&mut harness, "Fix the parser");
    wait_until(&mut harness, has_items);
    harness
}

fn is_row(role: Role) -> bool {
    matches!(role, Role::ListItem | Role::TreeItem)
}

/// The rows of the file list, which lies right of the sidebar and its tree.
fn rows<'a>(harness: &'a Harness<'_, App>) -> Vec<egui_kittest::Node<'a>> {
    let left = harness
        .get_by_role_and_label(Role::Label, "COMMIT")
        .rect()
        .left()
        - 12.0;
    harness
        .query_all_by(|node| is_row(node.role()))
        .filter(|node| node.rect().left() >= left)
        .collect()
}

fn has_items(harness: &Harness<'_, App>) -> bool {
    !rows(harness).is_empty()
}

/// The rows of the file list, top to bottom, by their labels.
fn items(harness: &Harness<'_, App>) -> Vec<String> {
    let mut rows: Vec<(f32, String)> = rows(harness)
        .into_iter()
        .map(|node| {
            let label = node.accesskit_node().label().unwrap_or_default();
            (node.rect().top(), label)
        })
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.into_iter().map(|(_, label)| label).collect()
}

fn item<'a>(harness: &'a Harness<'_, App>, label: &str) -> egui_kittest::Node<'a> {
    rows(harness)
        .into_iter()
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no row {label:?} in {:?}", items(harness)))
}

fn selected_item(harness: &Harness<'_, App>) -> Option<String> {
    rows(harness)
        .into_iter()
        .find(|node| node.accesskit_node().is_selected() == Some(true))
        .and_then(|node| node.accesskit_node().label())
}

/// The index of the file the diff panel shows, among the files of the
/// commit.
fn shown_file(harness: &Harness<'_, App>) -> Option<usize> {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .and_then(|session| session.details().file())
}

fn tree_toggled(harness: &Harness<'_, App>) -> Option<eframe::egui::accesskit::Toggled> {
    harness
        .get_by_role_and_label(Role::Button, TREE)
        .accesskit_node()
        .toggled()
}

fn show_tree(harness: &mut Harness<'_, App>) {
    harness.get_by_role_and_label(Role::Button, TREE).click();
    harness.run();
}

fn click_item(harness: &mut Harness<'_, App>, label: &str) {
    let at = item(harness, label).rect().center();
    click_at(harness, at, PointerButton::Primary);
}

/// Types `text` into the filter field of the file list.
fn type_filter(harness: &mut Harness<'_, App>, text: &str) {
    harness
        .get_by_role_and_label(Role::TextInput, FILTER)
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::TextInput, FILTER)
        .type_text(text);
    harness.run();
}

fn clear_filter(harness: &mut Harness<'_, App>) {
    let length = harness
        .get_by_role_and_label(Role::TextInput, FILTER)
        .accesskit_node()
        .value()
        .map_or(0, |value| value.chars().count());
    harness
        .get_by_role_and_label(Role::TextInput, FILTER)
        .click();
    harness.run();
    for _ in 0..length {
        harness.key_press(eframe::egui::Key::Backspace);
    }
    harness.run();
}

#[test]
fn switching_to_the_tree_shows_folders_before_files() {
    let mut harness = open_newest(tree_backend());
    assert_eq!(
        items(&harness),
        [
            "Modified: src/app/main.rs",
            "Modified: src/app/view.rs",
            "Added: README.md"
        ]
    );
    assert_eq!(
        tree_toggled(&harness),
        Some(eframe::egui::accesskit::Toggled::False)
    );
    show_tree(&mut harness);
    assert_eq!(
        items(&harness),
        [
            "src/app",
            "Modified: main.rs",
            "Modified: view.rs",
            "Added: README.md"
        ]
    );
    assert_eq!(
        tree_toggled(&harness),
        Some(eframe::egui::accesskit::Toggled::True)
    );
    assert!(harness.state().settings().file_tree);
}

#[test]
fn a_click_on_a_folder_collapses_and_expands_it() {
    let mut harness = open_newest(tree_backend());
    show_tree(&mut harness);
    click_item(&mut harness, "src/app");
    assert_eq!(items(&harness), ["src/app", "Added: README.md"]);
    click_item(&mut harness, "src/app");
    assert_eq!(
        items(&harness),
        [
            "src/app",
            "Modified: main.rs",
            "Modified: view.rs",
            "Added: README.md"
        ]
    );
}

#[test]
fn a_folder_tells_assistive_technology_its_level_and_state() {
    let mut harness = open_newest(tree_backend());
    show_tree(&mut harness);
    harness.get_by_role_and_label(Role::Tree, "COMMIT");
    let folder = item(&harness, "src/app");
    assert_eq!(folder.accesskit_node().role(), Role::TreeItem);
    assert_eq!(folder.accesskit_node().level(), Some(1));
    assert_eq!(folder.accesskit_node().data().is_expanded(), Some(true));
    assert_eq!(
        item(&harness, "Modified: main.rs").accesskit_node().level(),
        Some(2)
    );
    click_item(&mut harness, "src/app");
    assert_eq!(
        item(&harness, "src/app")
            .accesskit_node()
            .data()
            .is_expanded(),
        Some(false)
    );
}

#[test]
fn the_filter_narrows_the_list_regardless_of_case() {
    let mut harness = open_newest(tree_backend());
    type_filter(&mut harness, "VIEW");
    assert_eq!(items(&harness), ["Modified: src/app/view.rs"]);
    show_tree(&mut harness);
    assert_eq!(items(&harness), ["src/app", "Modified: view.rs"]);
    clear_filter(&mut harness);
    assert_eq!(items(&harness).len(), 4);
}

#[test]
fn the_filter_finds_a_renamed_file_by_its_old_path() {
    let mut harness = open_newest(backend());
    type_filter(&mut harness, "a.rs");
    assert_eq!(items(&harness), ["Renamed: src/a.rs → src/b.rs"]);
}

#[test]
fn a_filter_without_matches_says_so_and_shows_no_diff() {
    let mut harness = open_newest(tree_backend());
    type_filter(&mut harness, "nothing like it");
    assert!(items(&harness).is_empty());
    assert!(
        harness
            .query_by_label("No file matches the filter.")
            .is_some()
    );
    assert_eq!(shown_file(&harness), None);
    assert_eq!(diff_texts(&harness), ["DIFF"]);
}

#[test]
fn a_selected_file_the_filter_hides_gives_way_to_the_first_file_shown() {
    let mut harness = open_newest(tree_backend());
    click_item(&mut harness, "Modified: src/app/view.rs");
    assert_eq!(shown_file(&harness), Some(1));
    type_filter(&mut harness, "main");
    assert_eq!(
        selected_item(&harness).as_deref(),
        Some("Modified: src/app/main.rs")
    );
    assert_eq!(shown_file(&harness), Some(0));
    clear_filter(&mut harness);
    assert_eq!(
        selected_item(&harness).as_deref(),
        Some("Modified: src/app/main.rs")
    );
}

#[test]
fn the_selection_stays_on_its_file_while_filtering() {
    let mut harness = open_newest(tree_backend());
    click_item(&mut harness, "Modified: src/app/view.rs");
    type_filter(&mut harness, "view");
    assert_eq!(
        selected_item(&harness).as_deref(),
        Some("Modified: src/app/view.rs")
    );
    assert_eq!(shown_file(&harness), Some(1));
}

#[test]
fn folders_collapse_while_filtering_until_the_filter_changes() {
    let backend = backend().with_changes(
        fake_id("c"),
        vec![
            change(ChangeKind::Added, "docs/guide.md", None),
            change(ChangeKind::Modified, "src/app/main.rs", None),
            change(ChangeKind::Modified, "src/app/view.rs", None),
            change(ChangeKind::Added, "README.md", None),
        ],
    );
    let mut harness = open_newest(backend);
    show_tree(&mut harness);
    click_item(&mut harness, "docs");
    type_filter(&mut harness, ".r");
    click_item(&mut harness, "src/app");
    assert_eq!(items(&harness), ["src/app"]);
    type_filter(&mut harness, "s");
    assert_eq!(
        items(&harness),
        ["src/app", "Modified: main.rs", "Modified: view.rs"]
    );
    clear_filter(&mut harness);
    assert_eq!(
        items(&harness),
        [
            "docs",
            "src/app",
            "Modified: main.rs",
            "Modified: view.rs",
            "Added: README.md"
        ]
    );
}

#[test]
fn the_filter_stays_while_other_commits_are_selected() {
    let mut harness = open_newest(tree_backend());
    type_filter(&mut harness, ".rs");
    select(&mut harness, "First commit");
    wait_until(&mut harness, |h| items(h) == ["Added: src/main.rs"]);
    assert_eq!(
        selected_item(&harness).as_deref(),
        Some("Added: src/main.rs")
    );
}

#[test]
fn in_the_tree_the_first_file_of_the_tree_is_selected() {
    let mut harness = open_newest(tree_backend());
    show_tree(&mut harness);
    select(&mut harness, "First commit");
    wait_until(&mut harness, |h| {
        items(h).contains(&"Added: main.rs".to_owned())
    });
    assert_eq!(
        items(&harness),
        ["src", "Added: main.rs", "Added: README.md"]
    );
    assert_eq!(selected_item(&harness).as_deref(), Some("Added: main.rs"));
    assert_eq!(shown_file(&harness), Some(1));
}

#[test]
fn a_selected_folder_shows_no_diff() {
    let mut harness = open_newest(tree_backend());
    show_tree(&mut harness);
    click_item(&mut harness, "src/app");
    click_item(&mut harness, "src/app");
    assert_eq!(selected_item(&harness).as_deref(), Some("src/app"));
    assert_eq!(shown_file(&harness), None);
}
