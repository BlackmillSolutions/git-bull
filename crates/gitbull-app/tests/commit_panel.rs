//! The commit panel: details of the selected commit and its changed files
//! (spec `commit-details`).

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Modifiers, OutputCommand, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::settings::Settings;
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_testkit::{FakeBackend, fake_id};
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};
use support::{Setup, build, path, settle_window, window};

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
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend,
        time_zone: Some(TimeZone::fixed(Offset::constant(2))),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_until(&mut harness, |h| commit_row(h, "Fix the parser").is_some());
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
/// value, other widgets as a label.
fn texts_in(harness: &Harness<'_, App>, area: eframe::egui::Rect) -> Vec<String> {
    harness
        .query_all_by(|node| {
            node.role() != Role::TextRun && (node.label().is_some() || node.value().is_some())
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
    let title = harness.get_by_label("COMMIT").rect();
    let diff = harness.get_by_label("DIFF").rect();
    let area = eframe::egui::Rect::from_min_max(
        eframe::egui::pos2(title.left() - 12.0, title.top()),
        eframe::egui::pos2(diff.left() - 4.0, f32::INFINITY),
    );
    texts_in(harness, area)
}

/// The texts in the diff panel: below its title and above the status bar.
fn diff_texts(harness: &Harness<'_, App>) -> Vec<String> {
    let diff = harness.get_by_label("DIFF").rect();
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
    let mut harness = open(backend());
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
