//! The sidebar: sections, views, references and navigating to them.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_core::session::BranchFilter;
use gitbull_core::settings::Settings;
use gitbull_git::content::CommitContent;
use gitbull_git::head::Head;
use gitbull_git::history::CommitLine;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::stashes::{Stash, Submodule, SubmoduleState};
use gitbull_testkit::{FakeBackend, HistoryFeed, commit_line, fake_id};
use support::{Setup, build, path, settle_window, tab_titles, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn reference(name: &str, kind: RefKind, commit: Option<&str>) -> Reference {
    let short = name
        .strip_prefix("refs/heads/")
        .or_else(|| name.strip_prefix("refs/remotes/"))
        .or_else(|| name.strip_prefix("refs/tags/"))
        .unwrap_or(name);
    Reference {
        name: name.to_owned(),
        short: short.to_owned(),
        kind,
        commit: commit.map(|c| fake_id(c).to_string()),
        upstream: None,
    }
}

/// e..a on main; side at x; tags on c and on a tree.
fn lines() -> Vec<CommitLine> {
    vec![
        commit_line("x", &["b"]),
        commit_line("e", &["d"]),
        commit_line("d", &["c"]),
        commit_line("c", &["b"]),
        commit_line("b", &["a"]),
        commit_line("a", &[]),
    ]
}

fn references() -> Vec<Reference> {
    vec![
        reference("refs/heads/feature/diff-view", RefKind::Branch, Some("d")),
        reference(
            "refs/heads/feature/graph-layout",
            RefKind::Branch,
            Some("c"),
        ),
        reference("refs/heads/main", RefKind::Branch, Some("e")),
        reference("refs/heads/side", RefKind::Branch, Some("x")),
        reference("refs/remotes/origin/main", RefKind::RemoteBranch, Some("e")),
        reference(
            "refs/remotes/origin/release/0.1",
            RefKind::RemoteBranch,
            Some("b"),
        ),
        reference("refs/tags/v1.0", RefKind::Tag, Some("c")),
        reference("refs/tags/tree-tag", RefKind::Tag, None),
    ]
}

fn with_contents(mut backend: FakeBackend) -> FakeBackend {
    for (name, summary) in [
        ("x", "Side work"),
        ("e", "Fifth"),
        ("d", "Fourth"),
        ("c", "Third"),
        ("b", "Second"),
        ("a", "First"),
    ] {
        backend = backend.with_content(
            fake_id(name),
            CommitContent {
                message: format!("{summary}\n"),
                ..CommitContent::default()
            },
        );
    }
    backend
}

fn backend() -> FakeBackend {
    with_contents(
        FakeBackend::default()
            .with_repository(root())
            .with_repository(root().join("libs").join("inner"))
            .with_history(root(), lines())
            .with_references(root(), references())
            .with_stashes(
                root(),
                vec![Stash {
                    commit: fake_id("s").to_string(),
                    parents: Vec::new(),
                    selector: "stash@{0}".into(),
                    message: "On main: try the layout".into(),
                }],
            )
            .with_submodules(
                root(),
                vec![
                    Submodule {
                        path: "libs/inner".into(),
                        commit: fake_id("i").to_string(),
                        state: SubmoduleState::Current,
                    },
                    Submodule {
                        path: "libs/missing".into(),
                        commit: fake_id("m").to_string(),
                        state: SubmoduleState::NotInitialised,
                    },
                ],
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
        ..Setup::default()
    });
    let mut harness = window(test.app);
    settle_window(&mut harness);
    wait_for(&mut harness, |h| h.query_by_label("graph-layout").is_some());
    wait_for(&mut harness, |h| h.query_by_label("Fifth").is_some());
    harness
}

fn wait_for(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1000 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

/// The row of the sidebar with `label`.
fn item<'a>(harness: &'a Harness<'_, App>, label: &str) -> egui_kittest::Node<'a> {
    harness
        .get_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no sidebar row {label}"))
}

fn has_item(harness: &Harness<'_, App>, label: &str) -> bool {
    harness
        .get_all_by_role(Role::TreeItem)
        .any(|node| node.accesskit_node().label().as_deref() == Some(label))
}

fn click(harness: &mut Harness<'_, App>, label: &str) {
    item(harness, label).click();
    harness.run();
}

/// Whether the commit row whose summary is `summary` is selected.
fn commit_selected(harness: &Harness<'_, App>, summary: &str) -> bool {
    harness
        .get_all_by_role(Role::Row)
        .find(|node| {
            node.accesskit_node()
                .label()
                .is_some_and(|label| label.starts_with(summary))
        })
        .is_some_and(|node| node.accesskit_node().is_selected() == Some(true))
}

#[test]
fn references_stashes_and_submodules_are_listed_in_their_sections() {
    let harness = open(backend());
    for label in [
        "WORKSPACE",
        "BRANCHES",
        "TAGS",
        "REMOTES",
        "STASHES",
        "SUBMODULES",
        "main",
        "v1.0",
        "On main: try the layout",
        "libs/inner",
    ] {
        assert!(has_item(&harness, label), "{label} is missing");
    }
    let order: Vec<f32> = [
        "WORKSPACE",
        "BRANCHES",
        "TAGS",
        "REMOTES",
        "STASHES",
        "SUBMODULES",
    ]
    .iter()
    .map(|label| item(&harness, label).rect().top())
    .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
}

#[test]
fn a_collapsed_section_hides_its_entries_and_leaves_the_others() {
    let mut harness = open(backend());
    click(&mut harness, "TAGS");
    assert!(!has_item(&harness, "v1.0"));
    assert!(has_item(&harness, "graph-layout"));
    assert_eq!(
        item(&harness, "TAGS").accesskit_node().data().is_expanded(),
        Some(false)
    );
    click(&mut harness, "TAGS");
    assert!(has_item(&harness, "v1.0"));
}

#[test]
fn branches_with_a_common_prefix_are_grouped_in_a_folder() {
    let mut harness = open(backend());
    let folder = item(&harness, "feature").rect().top();
    let graph = item(&harness, "graph-layout").rect();
    let diff = item(&harness, "diff-view").rect();
    assert!(folder < diff.top() && folder < graph.top());
    click(&mut harness, "feature");
    assert!(!has_item(&harness, "graph-layout"));
    assert!(!has_item(&harness, "diff-view"));
}

#[test]
fn remote_branches_are_grouped_by_remote_and_by_folder() {
    let harness = open(backend());
    let origin = item(&harness, "origin").rect().top();
    let release = item(&harness, "release").rect().top();
    let entry = item(&harness, "0.1").rect().top();
    assert!(origin < release && release < entry);
}

#[test]
fn the_checked_out_branch_is_emphasised() {
    let harness = open(backend());
    assert_eq!(checked_out(&harness), ["main"]);
}

#[test]
fn with_a_detached_head_no_branch_is_emphasised() {
    let harness = open(backend().with_head(root(), Head::Detached(fake_id("c").to_string())));
    assert!(checked_out(&harness).is_empty());
}

/// The labels of the rows described as checked out.
fn checked_out(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .get_all_by_role(Role::TreeItem)
        .filter(|node| node.accesskit_node().description().as_deref() == Some("checked out"))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

fn filter_field<'a>(harness: &'a Harness<'_, App>) -> egui_kittest::Node<'a> {
    harness.get_by_role(Role::TextInput)
}

#[test]
fn the_filter_narrows_references_ignoring_case_and_clearing_it_shows_all() {
    let mut harness = open(backend());
    filter_field(&harness).click();
    harness.run();
    filter_field(&harness).type_text("GRAPH");
    harness.run();
    assert!(has_item(&harness, "graph-layout"));
    assert!(has_item(&harness, "feature"));
    assert!(!has_item(&harness, "diff-view"));
    assert!(!has_item(&harness, "v1.0"));

    for _ in 0.."GRAPH".len() {
        harness.key_press(Key::Backspace);
    }
    harness.run();
    assert!(has_item(&harness, "diff-view"));
    assert!(has_item(&harness, "v1.0"));
}

#[test]
fn choosing_file_status_shows_it_and_history_comes_back_as_it_was() {
    let mut harness = open(backend());
    harness.get_by_label("Third").click();
    harness.run();
    assert!(commit_selected(&harness, "Third"));

    click(&mut harness, "File status");
    assert!(harness.query_by_label("Description").is_none());
    harness.get_by_role_and_label(Role::Heading, "File status");

    click(&mut harness, "History");
    harness.get_by_label("Description");
    assert!(commit_selected(&harness, "Third"));
}

#[test]
fn choosing_a_tag_selects_its_commit() {
    let mut harness = open(backend());
    click(&mut harness, "v1.0");
    assert!(commit_selected(&harness, "Third"));
}

#[test]
fn choosing_a_branch_before_its_commit_has_loaded_selects_it_when_it_arrives() {
    let feed = HistoryFeed::new();
    let backend = with_contents(
        FakeBackend::default()
            .with_repository(root())
            .with_history_feed(root(), &feed)
            .with_references(root(), references()),
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
    wait_for(&mut harness, |h| h.query_by_label("graph-layout").is_some());
    feed.send(lines().into_iter().take(2));
    wait_for(&mut harness, |h| h.query_by_label("Fifth").is_some());

    click(&mut harness, "0.1");
    feed.send(lines().into_iter().skip(2));
    feed.finish();
    wait_for(&mut harness, |h| commit_selected(h, "Second"));
}

#[test]
fn a_branch_outside_the_filtered_graph_offers_to_show_all_branches() {
    let backend = backend().with_history_for(
        root(),
        &["--end-of-options", "HEAD"],
        lines().into_iter().skip(1).collect(),
    );
    let mut harness = open(backend);
    harness
        .state_mut()
        .workspace_mut()
        .unwrap()
        .active_mut()
        .unwrap()
        .session_mut()
        .unwrap()
        .set_filter(BranchFilter::Current);
    // The rows of the current branch alone: Side work is gone, Fifth back.
    wait_for(&mut harness, |h| {
        h.query_by_label("Side work").is_none() && h.query_by_label("Fifth").is_some()
    });

    click(&mut harness, "side");
    // While the history still loads, the answer comes once it has loaded.
    wait_for(&mut harness, |h| {
        h.query_by_label_contains("hidden by the branch filter")
            .is_some()
    });
    harness
        .get_by_role_and_label(Role::Button, "Show all branches")
        .click();
    // One frame applies the click; the history of all branches then loads
    // before the next frame, which finds the commit at once.
    harness.step();
    std::thread::sleep(std::time::Duration::from_millis(100));
    harness.step();
    wait_for(&mut harness, |h| commit_selected(h, "Side work"));
    assert!(
        harness
            .query_by_label_contains("hidden by the branch filter")
            .is_none()
    );
}

#[test]
fn a_tag_that_points_to_no_commit_says_so_and_keeps_the_selection() {
    let mut harness = open(backend());
    harness.get_by_label("Second").click();
    harness.run();
    click(&mut harness, "tree-tag");
    harness.get_by_label_contains("does not point to a commit");
    assert!(commit_selected(&harness, "Second"));
}

#[test]
fn an_initialised_submodule_opens_in_a_new_tab() {
    let mut harness = open(backend());
    click(&mut harness, "libs/inner");
    harness.key_press(Key::Enter);
    settle_window(&mut harness);
    assert_eq!(tab_titles(harness.state()), ["git-bull", "inner"]);
}

#[test]
fn a_submodule_that_is_not_initialised_is_marked_and_does_not_open() {
    let mut harness = open(backend());
    let missing = item(&harness, "libs/missing");
    assert_eq!(
        missing.accesskit_node().description().as_deref(),
        Some("not initialised")
    );
    click(&mut harness, "libs/missing");
    harness.key_press(Key::Enter);
    settle_window(&mut harness);
    assert_eq!(tab_titles(harness.state()), ["git-bull"]);
    // Its empty folder lies inside the repository, so an attempt would end
    // in the existing tab; but every opening is recorded as recent.
    assert!(harness.state().settings().recent.is_empty());
}

#[test]
fn arrow_keys_move_through_the_sidebar_and_navigate() {
    let mut harness = open(backend());
    click(&mut harness, "graph-layout");
    assert!(commit_selected(&harness, "Third"));
    harness.key_press_modifiers(Modifiers::NONE, Key::ArrowUp);
    harness.run();
    assert!(commit_selected(&harness, "Fourth"));
}

#[test]
fn scrolling_ten_thousand_tags_stays_fluid() {
    let mut many = references();
    many.extend(
        (0..10_000).map(|n| reference(&format!("refs/tags/v0.{n:05}"), RefKind::Tag, Some("a"))),
    );
    let mut harness = open(backend().with_references(root(), many));
    click(&mut harness, "v1.0");
    let mut slowest = std::time::Duration::ZERO;
    for _ in 0..60 {
        harness.key_press(Key::PageDown);
        let started = std::time::Instant::now();
        harness.step();
        slowest = slowest.max(started.elapsed());
    }
    // Sixty pages further down, the tags of the thousands are in view.
    assert!(
        harness.get_all_by_role(Role::TreeItem).any(|node| node
            .accesskit_node()
            .label()
            .is_some_and(|label| label.starts_with("v0.0") && label.as_str() > "v0.00500")),
        "the list did not scroll"
    );
    // Debug builds are several times slower than release builds; the
    // target of 16.7 ms holds for release builds and is measured there.
    let limit = if cfg!(debug_assertions) { 100 } else { 17 };
    assert!(
        slowest < std::time::Duration::from_millis(limit),
        "a frame took {slowest:?}"
    );
}

const ALL: [&str; 4] = ["--branches", "--tags", "--remotes", "--end-of-options"];
const CURRENT: [&str; 2] = ["--end-of-options", "HEAD"];
const DIFF_VIEW: [&str; 2] = ["--end-of-options", "refs/heads/feature/diff-view"];

/// All branches hold x and e..a, the current branch e..a, and
/// feature/diff-view d..a.
fn per_filter() -> FakeBackend {
    backend()
        .with_history_for(root(), &ALL, lines())
        .with_history_for(root(), &CURRENT, lines().into_iter().skip(1).collect())
        .with_history_for(root(), &DIFF_VIEW, lines().into_iter().skip(2).collect())
}

fn branch_filter<'a>(harness: &'a Harness<'_, App>) -> egui_kittest::Node<'a> {
    harness.get_by_role(Role::ComboBox)
}

fn choose_filter(harness: &mut Harness<'_, App>, option: &str) {
    branch_filter(harness).click();
    harness.run();
    harness.get_by_role_and_label(Role::Button, option).click();
    harness.run();
}

fn right_click(harness: &mut Harness<'_, App>, label: &str) {
    let at = item(harness, label).rect().center();
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(eframe::egui::Event::PointerButton {
            pos: at,
            button: eframe::egui::PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
}

#[test]
fn the_switch_shows_the_current_branch_only_and_all_branches_again() {
    let mut harness = open(per_filter());
    assert_eq!(
        branch_filter(&harness).accesskit_node().value().as_deref(),
        Some("All branches")
    );
    choose_filter(&mut harness, "Current branch");
    // The labels of new rows arrive with their content.
    wait_for(&mut harness, |h| {
        h.query_by_label("Side work").is_none() && h.query_by_label("Fifth").is_some()
    });

    choose_filter(&mut harness, "All branches");
    wait_for(&mut harness, |h| h.query_by_label("Side work").is_some());
}

#[test]
fn show_only_this_branch_restricts_the_graph_and_names_the_branch() {
    let mut harness = open(per_filter());
    right_click(&mut harness, "diff-view");
    harness.get_by_label("Show only this branch").click();
    harness.run();
    wait_for(&mut harness, |h| {
        h.query_by_label("Fifth").is_none() && h.query_by_label("Fourth").is_some()
    });
    assert!(harness.query_by_label("Side work").is_none());
    assert_eq!(
        branch_filter(&harness).accesskit_node().value().as_deref(),
        Some("feature/diff-view")
    );
}

#[test]
fn a_tag_offers_no_show_only_this_branch() {
    let mut harness = open(per_filter());
    right_click(&mut harness, "v1.0");
    assert!(harness.query_by_label("Show only this branch").is_none());
}
