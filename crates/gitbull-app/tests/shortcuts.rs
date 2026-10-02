//! Keyboard shortcuts for tabs, opening repositories and moving between
//! the hunks of a diff.
//!
//! Shortcuts for lists, copying, search and refresh arrive with those
//! features.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::os::OperatingSystem;
use eframe::egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_core::settings::Settings;
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_testkit::{FakeBackend, fake_id};
use support::{
    BURST, Setup, active_title, build, commit_list_scroll, find_row, long_history, path,
    settle_window, tab_titles, turn_wheel, wait_for_row, window, window_at_60_fps, window_on,
};

/// Three repositories open, the middle one active.
fn three_tabs() -> Setup {
    Setup {
        settings: Settings {
            tabs: vec![
                path(&["work", "git-bull"]),
                path(&["work", "linux"]),
                path(&["work", "chromium"]),
            ],
            active_tab: Some(1),
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(path(&["work", "git-bull"]))
            .with_repository(path(&["work", "linux"]))
            .with_repository(path(&["work", "chromium"])),
        ..Setup::default()
    }
}

#[test]
fn command_o_shows_the_repository_chooser() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::O);
    harness.run();
    harness.get_by_label("Choose folder…");
}

#[test]
fn command_t_shows_the_repository_chooser_for_a_new_tab() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::T);
    harness.run();
    harness.get_by_label("Choose folder…");
}

#[test]
fn command_w_closes_the_current_tab() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::W);
    harness.run();
    assert_eq!(tab_titles(harness.state()), ["git-bull", "chromium"]);
}

#[test]
fn control_tab_and_control_shift_tab_switch_tabs() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);

    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    harness.run();
    assert_eq!(active_title(harness.state()).as_deref(), Some("chromium"));

    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
    harness.run();
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn control_shift_page_down_and_up_move_the_active_tab() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);

    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::PageDown);
    harness.run();
    assert_eq!(
        tab_titles(harness.state()),
        ["git-bull", "chromium", "linux"]
    );
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));

    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::PageUp);
    harness.run();
    assert_eq!(
        tab_titles(harness.state()),
        ["git-bull", "linux", "chromium"]
    );
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn the_last_tab_moves_no_further() {
    let mut harness = window(build(three_tabs()).app);
    settle_window(&mut harness);
    for _ in 0..2 {
        harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::PageDown);
        harness.run();
    }
    assert_eq!(
        tab_titles(harness.state()),
        ["git-bull", "chromium", "linux"]
    );
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn on_macos_tabs_move_with_control_like_they_switch() {
    let mut harness = window_on(OperatingSystem::Mac, build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::PageUp);
    harness.run();
    assert_eq!(
        tab_titles(harness.state()),
        ["linux", "git-bull", "chromium"]
    );
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
}

#[test]
fn on_macos_cmd_w_closes_the_current_tab() {
    let mut harness = window_on(OperatingSystem::Mac, build(three_tabs()).app);
    settle_window(&mut harness);
    // What egui-winit sends for Cmd on macOS: both `mac_cmd` and `command`.
    harness.key_press_modifiers(Modifiers::MAC_CMD | Modifiers::COMMAND, Key::W);
    harness.run();
    assert_eq!(tab_titles(harness.state()), ["git-bull", "chromium"]);
}

#[test]
fn on_macos_tabs_switch_with_control_tab_because_cmd_tab_belongs_to_the_system() {
    let mut harness = window_on(OperatingSystem::Mac, build(three_tabs()).app);
    settle_window(&mut harness);
    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    harness.run();
    assert_eq!(active_title(harness.state()).as_deref(), Some("chromium"));
}

/// On macOS Ctrl is not Cmd, so egui takes Ctrl+Shift+Tab for Shift+Tab
/// too and would move the keyboard focus into the tab just shown.
#[test]
fn on_macos_switching_tabs_leaves_the_keyboard_focus_alone() {
    let mut harness = window_on(OperatingSystem::Mac, build(three_tabs()).app);
    settle_window(&mut harness);
    for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::SHIFT] {
        harness.key_press_modifiers(modifiers, Key::Tab);
        harness.run();
        let focused = harness.ctx.memory(|memory| memory.focused());
        assert_eq!(focused, None, "after {modifiers:?}");
    }
}

#[test]
fn a_tab_left_during_a_motion_shows_its_commit_list_at_rest_on_return() {
    let first = path(&["work", "git-bull"]);
    let second = path(&["work", "linux"]);
    let backend = FakeBackend::default()
        .with_repository(&first)
        .with_repository(&second);
    let test = build(Setup {
        settings: Settings {
            tabs: vec![first.clone(), second],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: long_history(backend, &first, 200),
        ..Setup::default()
    });
    let mut harness = window_at_60_fps(test.app);
    settle_window(&mut harness);
    wait_for_row(&mut harness, "Commit 0, ");
    let row = find_row(&harness, "Commit 3, ").expect("the row of Commit 3");
    let start = commit_list_scroll(&harness);

    harness.hover_at(row.center());
    harness.step();
    turn_wheel(&mut harness, -BURST / 40.0, Modifiers::NONE);
    for _ in 0..5 {
        harness.step();
    }
    let moving = commit_list_scroll(&harness) - start;
    assert!(moving > 0.0 && moving < 600.0, "{moving}");

    harness.key_press_modifiers(Modifiers::CTRL, Key::Tab);
    for _ in 0..30 {
        harness.step();
    }
    assert_eq!(active_title(harness.state()).as_deref(), Some("linux"));
    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab);
    harness.step();
    assert_eq!(active_title(harness.state()).as_deref(), Some("git-bull"));

    let moved = commit_list_scroll(&harness) - start;
    assert!((moved - BURST).abs() < 0.5, "{moved}");
    for _ in 0..10 {
        harness.step();
    }
    assert_eq!(commit_list_scroll(&harness) - start, moved, "still moving");
}

/// The hunk that replaces line `at`, with three lines of context.
fn hunk_at(at: u32) -> Hunk {
    let line = |kind, old, new, text: String| DiffLine {
        kind,
        old_number: old,
        new_number: new,
        text,
        no_newline: false,
        cut: false,
        crlf: false,
    };
    let first = at.saturating_sub(3).max(1);
    let context = |n: u32| line(LineKind::Context, Some(n), Some(n), format!("line {n}"));
    let mut lines: Vec<DiffLine> = (first..at).map(context).collect();
    lines.push(line(LineKind::Removed, Some(at), None, "old".to_owned()));
    lines.push(line(LineKind::Added, None, Some(at), "new".to_owned()));
    lines.extend((at + 1..=at + 3).map(context));
    let count = at + 3 - first + 1;
    Hunk {
        header: format!("@@ -{first},{count} +{first},{count} @@"),
        old_start: first,
        new_start: first,
        lines,
    }
}

/// A repository whose one commit changes a file in four places.
fn four_hunks() -> Setup {
    let root = path(&["work", "git-bull"]);
    let c = fake_id("c");
    let person = Signature {
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        time: 1_767_268_800,
        offset_minutes: 0,
    };
    let diff = FileDiff {
        old_path: Some("notes.txt".into()),
        new_path: Some("notes.txt".into()),
        old_mode: Some("100644".to_owned()),
        new_mode: Some("100644".to_owned()),
        old_blob: None,
        new_blob: None,
        new_in_working_copy: false,
        content: Content::Text([2, 50, 100, 150].map(hunk_at).to_vec()),
        truncated: false,
    };
    Setup {
        settings: Settings {
            tabs: vec![root.clone()],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: FakeBackend::default()
            .with_repository(root.clone())
            .with_history(
                root,
                vec![CommitLine {
                    timestamp: 1_767_268_800,
                    id: c,
                    parents: Vec::new(),
                }],
            )
            .with_content(
                c,
                CommitContent {
                    author: person.clone(),
                    committer: person,
                    message: "Change four places\n".to_owned(),
                },
            )
            .with_changes(
                c,
                vec![FileChange {
                    kind: ChangeKind::Modified,
                    path: "notes.txt".into(),
                    old_path: None,
                }],
            )
            .with_diff(c, "notes.txt", diff),
        ..Setup::default()
    }
}

/// The top of the header of the hunk that starts with `header`.
fn header_top(
    harness: &egui_kittest::Harness<'_, gitbull_app::app::App>,
    header: &str,
) -> Option<f32> {
    harness
        .query_all_by_role(Role::Code)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(header))
        })
        .map(|node| node.rect().top())
}

#[test]
fn f7_moves_the_diff_to_its_next_hunk() {
    let mut harness = window(build(four_hunks()).app);
    settle_window(&mut harness);
    wait_for_row(&mut harness, "Change four places");
    let row = find_row(&harness, "Change four places").unwrap().center();
    harness.hover_at(row);
    for pressed in [true, false] {
        harness.event(eframe::egui::Event::PointerButton {
            pos: row,
            button: eframe::egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    for _ in 0..200 {
        if header_top(&harness, "@@ -1,5").is_some() {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    harness.run();
    let top = header_top(&harness, "@@ -1,5").expect("the first hunk shows");

    harness.key_press(Key::F7);
    harness.run();

    assert_eq!(header_top(&harness, "@@ -47,7"), Some(top));
}

/// A repository whose one commit changes two files, one in a folder, with
/// file lists shown as trees.
fn tree_of_files() -> Setup {
    let mut setup = four_hunks();
    setup.settings.file_tree = true;
    setup.backend = setup.backend.with_changes(
        fake_id("c"),
        vec![
            FileChange {
                kind: ChangeKind::Modified,
                path: "src/main.rs".into(),
                old_path: None,
            },
            FileChange {
                kind: ChangeKind::Modified,
                path: "notes.txt".into(),
                old_path: None,
            },
        ],
    );
    setup
}

#[test]
fn left_collapses_the_folder_selected_in_a_file_tree() {
    let mut harness = window(build(tree_of_files()).app);
    settle_window(&mut harness);
    wait_for_row(&mut harness, "Change four places");
    harness.get_by_label("Change four places").click();
    let file = |harness: &egui_kittest::Harness<'_, gitbull_app::app::App>, label: &str| {
        harness
            .query_all_by_role(Role::TreeItem)
            .find(|node| node.accesskit_node().label().as_deref() == Some(label))
            .map(|node| node.rect().center())
    };
    for _ in 0..200 {
        if file(&harness, "Modified: main.rs").is_some() {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let at = file(&harness, "Modified: main.rs").expect("the file in the tree");
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(eframe::egui::Event::PointerButton {
            pos: at,
            button: eframe::egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    harness.key_press(Key::ArrowUp);
    harness.run();
    harness.key_press(Key::ArrowLeft);
    harness.run();

    let folder = harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some("src"))
        .expect("the folder");
    assert_eq!(folder.accesskit_node().data().is_expanded(), Some(false));
    assert!(file(&harness, "Modified: main.rs").is_none());
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(eframe::egui::Id::new(gitbull_app::ui::AREA_COMMIT_PANEL))
    );
}

#[test]
fn control_l_focuses_the_filter_of_the_file_status() {
    let mut setup = four_hunks();
    setup.backend = setup.backend.with_status(
        path(&["work", "git-bull"]),
        gitbull_git::status::WorkingStatus {
            untracked: vec![gitbull_git::status::StatusEntry {
                kind: gitbull_git::status::StatusKind::Untracked,
                path: "notes.md".into(),
                old_path: None,
                submodule: false,
            }],
            ..Default::default()
        },
    );
    let mut harness = window(build(setup).app);
    settle_window(&mut harness);
    wait_for_row(&mut harness, "Change four places");
    harness
        .get_by_role_and_label(Role::TreeItem, "File status")
        .click();
    for _ in 0..200 {
        if harness.query_by_label("Filter files").is_some() {
            break;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    harness.key_press_modifiers(Modifiers::COMMAND, Key::L);
    harness.run();
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(eframe::egui::Id::new(
            gitbull_app::file_status_view::STATUS_FILTER
        ))
    );
}
