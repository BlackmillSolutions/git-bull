//! The commit list of the History view.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Id, Key, Modifiers, OutputCommand, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_app::icons;
use gitbull_app::ui::{AREA_COMMIT_PANEL, AREA_DIFF, AREA_SIDEBAR, COMMIT_LIST};
use gitbull_core::settings::Settings;
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::head::Head;
use gitbull_git::history::CommitLine;
use gitbull_git::path::RepoPath;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::status::{StatusEntry, StatusKind, WorkingStatus};
use gitbull_testkit::{FakeBackend, HistoryFeed, LiveRepo, fake_id};
use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};
use support::{Setup, build, path, settle_window, window};

fn seconds(text: &str) -> i64 {
    text.parse::<Timestamp>().unwrap().as_second()
}

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn signature(name: &str, when: &str, offset_minutes: i32) -> Signature {
    Signature {
        name: name.to_owned(),
        email: format!("{}@example.com", name.to_lowercase().replace(' ', ".")),
        time: seconds(when),
        offset_minutes,
    }
}

fn content(message: &str, author: Signature, committer: Signature) -> CommitContent {
    CommitContent {
        author,
        committer,
        message: message.to_owned(),
    }
}

fn line(name: &str, parents: &[&str], when: &str) -> CommitLine {
    CommitLine {
        timestamp: seconds(when),
        id: fake_id(name),
        parents: parents.iter().map(|p| fake_id(p)).collect(),
    }
}

fn reference(name: &str, kind: RefKind, commit: &str) -> Reference {
    let short = name
        .rsplit_once("heads/")
        .map(|(_, s)| s)
        .unwrap_or_else(|| {
            name.strip_prefix("refs/remotes/")
                .or_else(|| name.strip_prefix("refs/tags/"))
                .unwrap_or(name)
        });
    Reference {
        name: name.to_owned(),
        short: short.to_owned(),
        kind,
        commit: Some(fake_id(commit).to_string()),
        upstream: None,
    }
}

const C_DATE: &str = "2026-09-29T11:40:00+09:00";
/// Authored on one day, committed on a later one, as after a rebase.
const B_AUTHORED: &str = "2026-01-10T09:00:00Z";
const B_COMMITTED: &str = "2026-02-20T09:00:00Z";
const A_DATE: &str = "2025-12-01T08:00:00Z";

/// c on top of b on top of a; main, origin/main and HEAD on c, v1.0 on a.
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
            vec![
                reference("refs/heads/main", RefKind::Branch, "c"),
                reference("refs/remotes/origin/main", RefKind::RemoteBranch, "c"),
                reference("refs/tags/v1.0", RefKind::Tag, "a"),
            ],
        )
        .with_content(
            fake_id("c"),
            content(
                "Fix the parser\n\nThe body.\n",
                signature("Ada Lovelace", C_DATE, 540),
                signature("Ada Lovelace", C_DATE, 540),
            ),
        )
        .with_content(
            fake_id("b"),
            content(
                "Rebased change\n",
                signature("Grace Hopper", B_AUTHORED, 0),
                signature("Grace Hopper", B_COMMITTED, 0),
            ),
        )
        .with_content(
            fake_id("a"),
            content(
                "First commit\n",
                signature("Alan Turing", A_DATE, 0),
                signature("Alan Turing", A_DATE, 0),
            ),
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
    wait_for_label(&mut harness, "Fix the parser");
    harness
}

/// Steps until a node with `label` appears.
fn wait_for_label(harness: &mut Harness<'_, App>, label: &str) {
    for _ in 0..500 {
        if harness.query_by_label(label).is_some() {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("{label} did not appear");
}

fn short(name: &str) -> String {
    fake_id(name).short(7)
}

/// The accessible row of a commit, whose label starts with its summary.
fn row<'a>(harness: &'a Harness<'_, App>, summary: &str) -> egui_kittest::Node<'a> {
    harness
        .get_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(summary))
        })
        .unwrap_or_else(|| panic!("no row for {summary}"))
}

#[test]
fn each_row_shows_description_date_author_and_hash() {
    let harness = open(backend());
    harness.get_by_label("Fix the parser");
    harness.get_by_label("Ada Lovelace");
    harness.get_by_label("2026-09-29 04:40");
    harness.get_by_label(&short("c"));
}

#[test]
fn the_date_column_shows_the_commit_date_not_the_author_date() {
    let harness = open(backend());
    harness.get_by_label("2026-02-20 11:00");
    assert!(harness.query_by_label_contains("2026-01-10").is_none());
}

#[test]
fn the_date_tooltip_shows_the_original_offset() {
    let mut harness = open(backend());
    harness.get_by_label("2026-09-29 04:40").hover();
    harness.run();
    wait_for_label(&mut harness, "2026-09-29 11:40 +09:00");
}

#[test]
fn head_branch_and_remote_branch_are_badges_before_the_description() {
    let harness = open(backend());
    let description = harness.get_by_label("Fix the parser").rect();
    for name in ["HEAD", "main", "origin/main"] {
        let rect = badge(&harness, name, description);
        assert!(
            rect.right() <= description.left(),
            "{name} is not before the description"
        );
    }
    let first = harness.get_by_label("First commit").rect();
    badge(&harness, "v1.0", first);
}

#[test]
fn each_kind_of_reference_shows_its_icon_in_its_badge() {
    let harness = open(backend());
    let top = harness.get_by_label("Fix the parser").rect();
    let bottom = harness.get_by_label("First commit").rect();
    for (name, icon, row) in [
        ("HEAD", icons::HEAD, top),
        ("main", icons::BRANCH, top),
        ("origin/main", icons::REMOTE_BRANCH, top),
        ("v1.0", icons::TAG, bottom),
    ] {
        let rect = badge(&harness, name, row);
        let texts = support::texts_in(harness.output(), rect);
        assert!(texts.iter().any(|text| text == icon), "{name}: {texts:?}");
    }
}

/// Where the badge `name` is drawn, if it is.
fn badge_node(harness: &Harness<'_, App>, name: &str) -> Option<eframe::egui::Rect> {
    harness
        .get_all_by_role(Role::Label)
        .find(|node| node.accesskit_node().value().as_deref() == Some(name))
        .map(|node| node.rect())
}

/// The badge `name` in the row whose description is at `description`; the
/// status bar names the branch too.
fn badge(
    harness: &Harness<'_, App>,
    name: &str,
    description: eframe::egui::Rect,
) -> eframe::egui::Rect {
    harness
        .get_all_by_label(name)
        .map(|node| node.rect())
        .find(|rect| (rect.center().y - description.center().y).abs() < 2.0)
        .unwrap_or_else(|| panic!("no badge {name} in the row of the commit"))
}

#[test]
fn a_detached_head_has_its_badge_on_the_checked_out_commit() {
    let harness = open(backend().with_head(root(), Head::Detached(fake_id("b").to_string())));
    let head = harness.get_by_label("HEAD").rect();
    let rebased = harness.get_by_label("Rebased change").rect();
    assert!((head.center().y - rebased.center().y).abs() < 2.0);
}

#[test]
fn badges_that_do_not_fit_are_counted_and_the_description_stays_visible() {
    let tags: Vec<Reference> = (1..=40)
        .map(|n| reference(&format!("refs/tags/release-{n:03}"), RefKind::Tag, "a"))
        .collect();
    let mut harness = open(backend().with_references(root(), tags));
    // The sidebar lists the tags too; the badges are the label nodes.
    for _ in 0..500 {
        if badge_node(&harness, "release-001").is_some() {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    // The count reads "+<number>".
    let rest: usize = harness
        .get_all_by_role(Role::Label)
        .filter_map(|node| node.accesskit_node().value())
        .find_map(|text| text.strip_prefix('+')?.parse().ok())
        .expect("a count of the badges that do not fit");
    let shown = (1..=40)
        .filter(|n| badge_node(&harness, &format!("release-{n:03}")).is_some())
        .count();
    assert!(shown > 0 && rest > 0, "{shown} shown, {rest} counted");
    assert_eq!(shown + rest, 40);

    // Badges and count take at most half of the description column.
    let column = harness.get_by_label("Description").rect().left()
        ..harness.get_by_label("Date").rect().left();
    let count = harness.get_by_label(&format!("+{rest}")).rect();
    let first_badge = badge_node(&harness, "release-001").unwrap();
    assert!(
        count.right() - first_badge.left() <= (column.end - column.start) / 2.0,
        "badges take {} of {}",
        count.right() - first_badge.left(),
        column.end - column.start
    );
    let first = harness.get_by_label("First commit").rect();
    assert!(first.width() > 0.0);
}

#[test]
fn a_row_without_content_shows_placeholders_and_its_date_and_hash() {
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history(
            root(),
            vec![line("c", &["b"], C_DATE), line("b", &[], B_COMMITTED)],
        )
        .with_content(
            fake_id("c"),
            content(
                "Fix the parser\n",
                signature("Ada Lovelace", C_DATE, 540),
                signature("Ada Lovelace", C_DATE, 540),
            ),
        );
    let harness = open(backend);
    // b has no content: description and author wait, date and hash do not.
    harness.get_by_label("2026-02-20 11:00");
    harness.get_by_label(&short("b"));
    assert_eq!(harness.get_all_by_label("Loading…").count(), 2);
    assert!(harness.query_by_label("Rebased change").is_none());
}

/// Clicks the row of a commit with `button`.
fn click_row(harness: &mut Harness<'_, App>, summary: &str, button: PointerButton) {
    let at = harness.get_by_label(summary).rect().center();
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
fn the_context_menu_copies_the_full_hash() {
    let mut harness = open(backend());
    click_row(&mut harness, "Rebased change", PointerButton::Secondary);
    harness.get_by_label("Copy full hash").click();
    harness.step();
    assert_eq!(copied(&harness), Some(fake_id("b").to_string()));
}

/// "Commit 0" on top of "Commit 1" and so on down to "Commit 199".
fn long_lines() -> Vec<CommitLine> {
    (0..200)
        .map(|i| CommitLine {
            timestamp: seconds(A_DATE) - i,
            id: fake_id(&format!("n{i}")),
            parents: if i < 199 {
                vec![fake_id(&format!("n{}", i + 1))]
            } else {
                Vec::new()
            },
        })
        .collect()
}

/// The history of `long_lines`, with main on its top, that `live` can
/// change while it is shown.
fn open_long(live: &LiveRepo) -> Harness<'static, App> {
    live.set_lines(long_lines());
    live.set_references(vec![reference("refs/heads/main", RefKind::Branch, "n0")]);
    live.set_status(WorkingStatus::default());
    let mut backend = FakeBackend::default()
        .with_repository(root())
        .with_live(root(), live);
    let author = signature("Ada Lovelace", A_DATE, 0);
    for i in 0..200 {
        backend = backend.with_content(
            fake_id(&format!("n{i}")),
            content(
                &format!(
                    "Commit {i}
"
                ),
                author.clone(),
                author.clone(),
            ),
        );
    }
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
    wait_for_label(&mut harness, "Commit 0");
    harness
}

/// How many commits the history of the active tab holds.
fn loaded(harness: &Harness<'_, App>) -> usize {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .map_or(0, |session| session.history().store.len())
}

/// Clicks the row of the commit with `summary` in the list, found by its
/// row: the commit panel shows the summary of the selected commit too.
fn click_list_row(harness: &mut Harness<'_, App>, summary: &str, button: PointerButton) {
    let prefix = format!("{summary}, ");
    for _ in 0..500 {
        if harness.query_all_by_role(Role::Row).any(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(&prefix))
        }) {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let at = row(harness, &prefix).rect().center();
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

#[test]
fn the_menu_copies_the_hash_of_its_commit_after_the_history_was_replaced() {
    let live = LiveRepo::new();
    let mut harness = open_long(&live);
    click_list_row(&mut harness, "Commit 0", PointerButton::Primary);
    harness.key_press(Key::End);
    harness.run();
    click_list_row(&mut harness, "Commit 199", PointerButton::Secondary);
    assert!(harness.query_by_label("Copy full hash").is_some());

    // Another branch makes the history load again; the new one has 50 of
    // its commits so far when it takes the place of the one shown.
    let feed = HistoryFeed::new();
    live.set_feed(&feed);
    live.set_references(vec![
        reference("refs/heads/main", RefKind::Branch, "n0"),
        reference("refs/heads/side", RefKind::Branch, "n5"),
    ]);
    harness.key_press(Key::F5);
    feed.send(long_lines().into_iter().take(50));
    for _ in 0..1000 {
        if loaded(&harness) == 50 {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(loaded(&harness), 50);
    harness.run();

    harness.get_by_label("Copy full hash").click();
    harness.step();
    assert_eq!(copied(&harness), Some(fake_id("n199").to_string()));
}

#[test]
fn the_menu_copies_the_hash_of_its_commit_when_uncommitted_changes_appear_above_it() {
    let live = LiveRepo::new();
    let mut harness = open_long(&live);
    click_list_row(&mut harness, "Commit 3", PointerButton::Secondary);
    assert!(harness.query_by_label("Copy full hash").is_some());

    live.set_status(WorkingStatus {
        unstaged: vec![StatusEntry {
            kind: StatusKind::Changed(gitbull_git::changes::ChangeKind::Modified),
            path: RepoPath::new("edit.txt"),
            old_path: None,
            submodule: false,
        }],
        ..WorkingStatus::default()
    });
    harness.key_press(Key::F5);
    for _ in 0..1000 {
        let shown = harness.query_all_by_role(Role::Row).any(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with("Uncommitted changes"))
        });
        if shown {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    harness.run();

    harness.get_by_label("Copy full hash").click();
    harness.step();
    assert_eq!(copied(&harness), Some(fake_id("n3").to_string()));
}

#[test]
fn command_c_copies_the_full_hash_of_the_selected_commit() {
    let mut harness = open(backend());
    click_row(&mut harness, "First commit", PointerButton::Primary);
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key: Key::C,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        });
    }
    harness.step();
    assert_eq!(copied(&harness), Some(fake_id("a").to_string()));
}

#[test]
fn the_copy_event_of_the_platform_copies_the_hash_too() {
    let mut harness = open(backend());
    click_row(&mut harness, "First commit", PointerButton::Primary);
    harness.input_mut().events.push(Event::Copy);
    harness.step();
    assert_eq!(copied(&harness), Some(fake_id("a").to_string()));
}

#[test]
fn the_selected_row_is_exposed_to_assistive_technology() {
    let mut harness = open(backend());
    click_row(&mut harness, "Rebased change", PointerButton::Primary);
    assert_eq!(
        row(&harness, "Rebased change")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
    assert_eq!(
        row(&harness, "Fix the parser")
            .accesskit_node()
            .is_selected(),
        Some(false)
    );
    let label = row(&harness, "Rebased change")
        .accesskit_node()
        .label()
        .unwrap();
    assert!(
        label.contains("Grace Hopper") && label.contains(&short("b")),
        "{label}"
    );
}

fn focused(harness: &Harness<'_, App>) -> Option<Id> {
    harness.ctx.memory(|memory| memory.focused())
}

fn press(harness: &mut Harness<'_, App>, modifiers: Modifiers) {
    harness.key_press_modifiers(modifiers, Key::Tab);
    harness.run();
}

#[test]
fn tab_moves_focus_through_the_areas() {
    let mut harness = open(backend());
    click_row(&mut harness, "Fix the parser", PointerButton::Primary);
    assert_eq!(focused(&harness), Some(Id::new(COMMIT_LIST)));
    press(&mut harness, Modifiers::NONE);
    assert_eq!(focused(&harness), Some(Id::new(AREA_COMMIT_PANEL)));
    press(&mut harness, Modifiers::NONE);
    assert_eq!(focused(&harness), Some(Id::new(AREA_DIFF)));
    press(&mut harness, Modifiers::NONE);
    assert_eq!(focused(&harness), Some(Id::new(AREA_SIDEBAR)));
    press(&mut harness, Modifiers::NONE);
    assert_eq!(focused(&harness), Some(Id::new(COMMIT_LIST)));
}

#[test]
fn each_area_shows_the_focus_ring_when_tab_reaches_it() {
    let mut harness = open(backend());
    click_row(&mut harness, "Fix the parser", PointerButton::Primary);
    for area in [COMMIT_LIST, AREA_COMMIT_PANEL, AREA_DIFF, AREA_SIDEBAR] {
        assert_eq!(focused(&harness), Some(Id::new(area)));
        assert!(
            !support::focus_rings(harness.output()).is_empty(),
            "no ring in {area}"
        );
        press(&mut harness, Modifiers::NONE);
    }
}

#[test]
fn shift_tab_moves_focus_back_through_the_areas() {
    let mut harness = open(backend());
    click_row(&mut harness, "Fix the parser", PointerButton::Primary);
    press(&mut harness, Modifiers::SHIFT);
    assert_eq!(focused(&harness), Some(Id::new(AREA_SIDEBAR)));
    press(&mut harness, Modifiers::SHIFT);
    assert_eq!(focused(&harness), Some(Id::new(AREA_DIFF)));
}

#[test]
fn dragging_the_edge_of_the_graph_column_changes_and_keeps_its_width() {
    let mut harness = open(backend());
    let graph = harness.get_by_label("Graph").rect();
    let description = harness.get_by_label("Description").rect();
    let before = description.left();
    // The edge between the two headers.
    let edge = eframe::egui::pos2(description.left() - 4.0, graph.center().y);
    harness.hover_at(edge);
    harness.drag_at(edge);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(edge + eframe::egui::vec2(step as f32 * 10.0, 0.0));
        harness.run();
    }
    harness.drop_at(edge + eframe::egui::vec2(40.0, 0.0));
    harness.run();

    let after = harness.get_by_label("Description").rect().left();
    assert!(
        (after - before - 40.0).abs() < 1.0,
        "moved by {}",
        after - before
    );
    let kept = harness.state().settings().layout.graph_column.unwrap();
    assert!(
        (kept - (before - graph.left() + 40.0)).abs() < 12.0,
        "{kept}"
    );
}

#[test]
fn a_low_window_keeps_room_for_the_commit_list() {
    let test = build(Setup {
        settings: Settings {
            tabs: vec![root()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: backend(),
        ..Setup::default()
    });
    let mut harness = Harness::builder()
        .with_size((1280.0, 360.0))
        .build_ui_state(
            |ui, app: &mut App| {
                app.logic();
                gitbull_app::ui::show(app, ui);
            },
            test.app,
        );
    settle_window(&mut harness);
    wait_for_label(&mut harness, "Fix the parser");
}

#[test]
fn arrow_keys_move_the_selection_in_the_focused_commit_list() {
    let mut harness = open(backend());
    click_row(&mut harness, "Fix the parser", PointerButton::Primary);
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(
        row(&harness, "Rebased change")
            .accesskit_node()
            .is_selected(),
        Some(true)
    );
}
