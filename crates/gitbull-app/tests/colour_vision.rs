//! The colour vision: the diff, the kinds of change, the badges and the
//! commit graph take the palette of the colour vision and the theme, while
//! syntax highlighting keeps the colours of the theme.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::epaint::Shape;
use eframe::egui::{Color32, FullOutput};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::app::App;
use gitbull_app::theme::{DARK, DARK_BLUE_YELLOW, DARK_RED_GREEN, LIGHT_BLUE_YELLOW, Palette};
use gitbull_app::ui::color;
use gitbull_core::settings::{ColourVision, Settings, ThemeSetting};
use gitbull_git::changes::{ChangeKind, FileChange};
use gitbull_git::content::{CommitContent, Signature};
use gitbull_git::diff::{Content, DiffLine, FileDiff, Hunk, LineKind};
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_testkit::{FakeBackend, fake_id};
use support::{Setup, build, path, settle_window, text_colours, wait_for_references, window};

/// A commit with a remote branch on it that modifies a Rust file and
/// replaces a line of it.
fn backend() -> FakeBackend {
    let root = path(&["work", "git-bull"]);
    let c = fake_id("c");
    let person = Signature {
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.com".to_owned(),
        time: 1_767_268_800,
        offset_minutes: 0,
    };
    let line = |kind, old, new, text: &str| DiffLine {
        kind,
        old_number: old,
        new_number: new,
        text: text.to_owned(),
        no_newline: false,
        cut: false,
        crlf: false,
    };
    FakeBackend::default()
        .with_repository(root.clone())
        .with_history(
            root.clone(),
            vec![CommitLine {
                timestamp: 1_767_268_800,
                id: c,
                parents: Vec::new(),
            }],
        )
        .with_references(
            root,
            vec![Reference {
                name: "refs/remotes/origin/main".to_owned(),
                short: "origin/main".to_owned(),
                kind: RefKind::RemoteBranch,
                commit: Some(c.to_string()),
                upstream: None,
            }],
        )
        .with_content(
            c,
            CommitContent {
                author: person.clone(),
                committer: person,
                message: "Change the parser\n".to_owned(),
            },
        )
        .with_changes(
            c,
            vec![FileChange {
                kind: ChangeKind::Modified,
                path: "src/parser.rs".into(),
                old_path: None,
            }],
        )
        .with_diff(
            c,
            "src/parser.rs",
            FileDiff {
                old_path: Some("src/parser.rs".into()),
                new_path: Some("src/parser.rs".into()),
                old_mode: Some("100644".to_owned()),
                new_mode: Some("100644".to_owned()),
                old_blob: Some(fake_id("o")),
                new_blob: Some(fake_id("n")),
                new_in_working_copy: false,
                content: Content::Text(vec![Hunk {
                    header: "@@ -1 +1 @@".to_owned(),
                    old_start: 1,
                    new_start: 1,
                    lines: vec![
                        line(LineKind::Removed, Some(1), None, "let colour = red;"),
                        line(LineKind::Added, None, Some(1), "let colour = blue;"),
                    ],
                }]),
                truncated: false,
            },
        )
        // Both versions of the file, which highlighting reads whole.
        .with_blob(fake_id("o"), b"let colour = red;\n")
        .with_blob(fake_id("n"), b"let colour = blue;\n")
}

fn open(theme: ThemeSetting, colour_vision: ColourVision) -> Harness<'static, App> {
    let test = build(Setup {
        settings: Settings {
            theme,
            colour_vision,
            tabs: vec![path(&["work", "git-bull"])],
            active_tab: Some(0),
            ..Settings::default()
        },
        backend: backend(),
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_for(&mut harness, |h| {
        h.query_by_label("Change the parser").is_some()
    });
    harness.get_by_label("Change the parser").click();
    // The diff shows first in plain text; its colours follow from a
    // background thread.
    wait_for(&mut harness, |h| {
        h.query_all_by_role(Role::Code).next().is_some() && highlighted(h)
    });
    wait_for_references(&mut harness);
    harness
}

/// Whether the diff of the selected commit has its syntax colours.
fn highlighted(harness: &Harness<'_, App>) -> bool {
    harness
        .state()
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .is_some_and(|session| session.details().highlighting().is_some())
}

fn wait_for(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1000 {
        if done(harness) {
            harness.run();
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the content did not arrive");
}

/// The fill colours of the circles drawn: the nodes of the graph.
fn circle_fills(output: &FullOutput) -> Vec<Color32> {
    fn walk(shape: &Shape, found: &mut Vec<Color32>) {
        match shape {
            Shape::Circle(circle) => found.push(circle.fill),
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

/// Asserts that the meaning-bearing colours drawn are those of `palette`.
fn drawn_with(harness: &Harness<'_, App>, palette: &Palette) {
    let output = harness.output();
    assert!(
        text_colours(output, "+").contains(&color(palette.diff_added_marker)),
        "added marker {:?}",
        text_colours(output, "+")
    );
    assert!(
        text_colours(output, "M").contains(&color(palette.status_modified)),
        "kind of change {:?}",
        text_colours(output, "M")
    );
    assert!(
        text_colours(output, "origin/main").contains(&color(palette.badge_remote)),
        "badge {:?}",
        text_colours(output, "origin/main")
    );
    assert!(
        circle_fills(output).contains(&color(palette.lanes[0])),
        "graph {:?}",
        circle_fills(output)
    );
}

#[test]
fn choosing_red_green_changes_the_meaning_colours_at_once() {
    let mut harness = open(ThemeSetting::Dark, ColourVision::Standard);
    drawn_with(&harness, &DARK);

    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(Role::RadioButton, "Red-green")
        .click();
    harness.run();

    drawn_with(&harness, &DARK_RED_GREEN);
}

#[test]
fn blue_yellow_follows_a_change_of_theme() {
    let mut harness = open(ThemeSetting::Light, ColourVision::BlueYellow);
    drawn_with(&harness, &LIGHT_BLUE_YELLOW);

    harness.get_by_role_and_label(Role::Button, "Theme").click();
    harness.run();
    harness
        .get_by_role_and_label(Role::RadioButton, "Dark")
        .click();
    harness.run();

    drawn_with(&harness, &DARK_BLUE_YELLOW);
}

#[test]
fn syntax_highlighting_keeps_the_colours_of_the_theme() {
    let line = "let colour = blue;";
    let standard = open(ThemeSetting::Dark, ColourVision::Standard);
    let red_green = open(ThemeSetting::Dark, ColourVision::RedGreen);
    let colours = |harness: &Harness<'_, App>| text_colours(harness.output(), line);
    assert!(!colours(&standard).is_empty(), "the line is drawn");
    assert_eq!(colours(&standard), colours(&red_green));
}
