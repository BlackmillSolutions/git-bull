//! The search field, the marks in the commit list and the Search view (spec
//! `commit-search`).
//!
//! What Git matches, such as a text taken literally, a whole path or the
//! commits of the branch filter only, is tested against real repositories
//! in `gitbull-git/tests/search.rs`; here the backend answers as Git would,
//! and the tests check what the search is asked for and what it shows.

mod support;

use eframe::egui::accesskit::Role;
use eframe::egui::{Event, Id, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gitbull_app::app::App;
use gitbull_app::search_view::SEARCH_RESULTS;
use gitbull_app::ui::AREA_SIDEBAR;
use gitbull_core::settings::Settings;
use gitbull_git::content::CommitContent;
use gitbull_git::history::CommitLine;
use gitbull_git::object_id::ObjectId;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::search::{HashMatch, Location, SearchKind};
use gitbull_testkit::{FakeBackend, HistoryFeed, Probe, commit_line, fake_id};
use support::{Setup, build, path, settle_window, unnamed_tab_stops, window};

fn root() -> std::path::PathBuf {
    path(&["work", "git-bull"])
}

fn content(summary: &str) -> CommitContent {
    CommitContent {
        message: format!("{summary}\n"),
        ..CommitContent::default()
    }
}

/// `main` at c, below the commit x of another branch.
fn history() -> Vec<CommitLine> {
    vec![
        commit_line("x", &["b"]),
        commit_line("c", &["b"]),
        commit_line("b", &[]),
    ]
}

fn with_contents(mut backend: FakeBackend) -> FakeBackend {
    for (name, summary) in [("x", "Side work"), ("c", "Head work"), ("b", "Base")] {
        backend = backend.with_content(fake_id(name), content(summary));
    }
    backend
}

fn backend() -> FakeBackend {
    with_contents(
        FakeBackend::default()
            .with_repository(root())
            .with_history(root(), history())
            .with_references(
                root(),
                vec![Reference {
                    name: "refs/heads/main".into(),
                    short: "main".into(),
                    kind: RefKind::Branch,
                    commit: Some(fake_id("c").to_string()),
                    upstream: None,
                }],
            ),
    )
}

fn open_with(backend: FakeBackend, first: &str) -> Harness<'static, App> {
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
    wait_until(&mut harness, |h| has_row(h, first));
    harness
}

fn open(backend: FakeBackend) -> Harness<'static, App> {
    open_with(backend, "Base")
}

fn wait_until(harness: &mut Harness<'_, App>, done: impl Fn(&Harness<'_, App>) -> bool) {
    for _ in 0..1500 {
        if done(harness) {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("timed out");
}

fn has_row(harness: &Harness<'_, App>, prefix: &str) -> bool {
    harness.query_all_by_role(Role::Row).any(|node| {
        node.accesskit_node()
            .label()
            .is_some_and(|label| label.starts_with(prefix))
    })
}

/// The summaries of the rows marked as matches, from the top.
fn marked(harness: &Harness<'_, App>) -> Vec<String> {
    let mut rows: Vec<(f32, String)> = harness
        .query_all_by_role(Role::Row)
        .filter(|node| node.accesskit_node().description().as_deref() == Some("Search match"))
        .filter_map(|node| {
            let label = node.accesskit_node().label()?;
            let summary = label.split(", ").next()?.to_owned();
            Some((node.rect().top(), summary))
        })
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.into_iter().map(|(_, summary)| summary).collect()
}

fn selected(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::Row)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

fn click_at(harness: &mut Harness<'_, App>, at: Pos2) {
    harness.hover_at(at);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    for _ in 0..3 {
        harness.step();
    }
}

fn press(harness: &mut Harness<'_, App>, key: Key, modifiers: Modifiers) {
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
        harness.step();
    }
    harness.step();
}

/// The search field of the toolbar, above the filter field of the sidebar.
/// Its placeholder is gone once it holds text.
fn field<'a>(harness: &'a Harness<'_, App>) -> egui_kittest::Node<'a> {
    harness
        .query_all_by_role(Role::TextInput)
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .expect("the search field")
}

fn field_center(harness: &Harness<'_, App>) -> Pos2 {
    field(harness).rect().center()
}

/// Types `text` into the search field.
fn type_text(harness: &mut Harness<'_, App>, text: &str) {
    let at = field_center(harness);
    click_at(harness, at);
    harness.event(Event::Text(text.to_owned()));
    harness.step();
}

/// Removes every character of the search field.
fn clear(harness: &mut Harness<'_, App>) {
    let at = field_center(harness);
    click_at(harness, at);
    let length = field(harness).value().unwrap_or_default().chars().count();
    for _ in 0..length {
        press(harness, Key::Backspace, Modifiers::NONE);
    }
}

/// Chooses the search mode with the label `mode`.
fn choose_mode(harness: &mut Harness<'_, App>, mode: &str) {
    // The branch filter is a combo box too.
    let combo = harness
        .query_all_by_role(Role::ComboBox)
        .find(|node| {
            let value = node.accesskit_node().value().unwrap_or_default();
            ["Message", "Author", "File path", "Hash"].contains(&value.as_str())
        })
        .expect("the mode of the search")
        .rect()
        .center();
    click_at(harness, combo);
    let option = harness
        .query_all_by_label(mode)
        .rfind(|node| node.accesskit_node().role() != Role::ComboBox)
        .expect("the mode in the list")
        .rect()
        .center();
    click_at(harness, option);
}

fn sidebar_item(harness: &Harness<'_, App>, label: &str) -> Pos2 {
    harness
        .query_all_by_role(Role::TreeItem)
        .find(|node| node.accesskit_node().label().as_deref() == Some(label))
        .expect("an item of the sidebar")
        .rect()
        .center()
}

fn text_shown(harness: &Harness<'_, App>, text: &str) -> bool {
    harness.query_all_by_value(text).next().is_some()
        || harness.query_all_by_label(text).next().is_some()
}

fn ids(names: &[&str]) -> Vec<ObjectId> {
    names.iter().map(|name| fake_id(name)).collect()
}

#[test]
fn matches_of_a_search_by_message_are_marked_and_listed() {
    let backend = backend().with_matches(SearchKind::Message, "work", ids(&["x", "c"]));
    let probe = backend.probe();
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    wait_until(&mut harness, |h| marked(h).len() == 2);
    assert_eq!(marked(&harness), ["Side work", "Head work"]);
    assert_eq!(probe.searches(), [(SearchKind::Message, "work".to_owned())]);
    assert!(text_shown(&harness, "2 matches"));

    let search = sidebar_item(&harness, "Search");
    click_at(&mut harness, search);
    wait_until(&mut harness, |h| {
        h.query_all_by_role(Role::ListItem).count() == 2
    });
    let listed: Vec<String> = harness
        .query_all_by_role(Role::ListItem)
        .filter_map(|node| node.accesskit_node().label())
        .collect();
    assert!(listed[0].starts_with("Side work"), "{listed:?}");
    assert!(listed[1].starts_with("Head work"), "{listed:?}");
}

#[test]
fn the_mode_is_chosen_next_to_the_field_and_the_text_is_passed_as_typed() {
    let backend = backend()
        .with_matches(SearchKind::Author, "novak", ids(&["b"]))
        .with_matches(SearchKind::Path, "a[1].txt", ids(&["c"]));
    let probe = backend.probe();
    let mut harness = open(backend);

    choose_mode(&mut harness, "Author");
    type_text(&mut harness, "novak");
    wait_until(&mut harness, |h| marked(h) == ["Base"]);

    clear(&mut harness);
    choose_mode(&mut harness, "File path");
    type_text(&mut harness, "a[1].txt");
    wait_until(&mut harness, |h| marked(h) == ["Head work"]);

    clear(&mut harness);
    choose_mode(&mut harness, "Message");
    type_text(&mut harness, "a.b");
    wait_until(&mut harness, |_| probe.searches().len() == 3);
    assert_eq!(
        probe.searches(),
        [
            (SearchKind::Author, "novak".to_owned()),
            (SearchKind::Path, "a[1].txt".to_owned()),
            (SearchKind::Message, "a.b".to_owned()),
        ]
    );
}

#[test]
fn an_abbreviated_hash_selects_its_commit() {
    let mut harness = open(backend().with_hash("abcd", HashMatch::Found(fake_id("c"))));
    choose_mode(&mut harness, "Hash");
    type_text(&mut harness, "abcd");
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Head work"))
    });
}

#[test]
fn an_unknown_hash_says_that_no_commit_was_found() {
    let mut harness = open(backend());
    choose_mode(&mut harness, "Hash");
    type_text(&mut harness, "abcd");
    wait_until(&mut harness, |h| {
        text_shown(h, "No commit was found for abcd.")
    });
}

#[test]
fn an_ambiguous_hash_says_so() {
    let mut harness = open(backend().with_hash("abcd", HashMatch::Ambiguous));
    choose_mode(&mut harness, "Hash");
    type_text(&mut harness, "abcd");
    wait_until(&mut harness, |h| {
        text_shown(
            h,
            "The hash abcd is ambiguous: several commits start with it.",
        )
    });
}

#[test]
fn a_hash_hidden_by_the_branch_filter_offers_to_show_all_branches() {
    let backend = backend()
        .with_hash("abcd", HashMatch::Found(fake_id("x")))
        .with_location(fake_id("x"), Location::HiddenByFilter);
    let mut harness = open(backend);
    choose_mode(&mut harness, "Hash");
    type_text(&mut harness, "abcd");
    let notice = format!(
        "The commit of {} is hidden by the branch filter.",
        fake_id("x").short(7)
    );
    wait_until(&mut harness, |h| text_shown(h, &notice));
    harness.get_by_label("Show all branches").click();
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Side work"))
    });
}

#[test]
fn a_hash_that_no_branch_leads_to_is_not_part_of_the_history() {
    let backend = backend()
        .with_hash("abcd", HashMatch::Found(fake_id("lost")))
        .with_location(fake_id("lost"), Location::NotInHistory);
    let mut harness = open(backend);
    choose_mode(&mut harness, "Hash");
    type_text(&mut harness, "abcd");
    let notice = format!(
        "The commit {} exists but is not part of the displayed history.",
        fake_id("lost").short(7)
    );
    wait_until(&mut harness, |h| text_shown(h, &notice));
}

#[test]
fn matches_appear_while_the_search_runs() {
    let feed = HistoryFeed::new();
    let backend = backend().with_search_feed(SearchKind::Message, "work", &feed);
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    feed.send([commit_line("x", &[])]);
    wait_until(&mut harness, |h| marked(h) == ["Side work"]);
    assert!(text_shown(&harness, "Searching… 1 match"));
    feed.send([commit_line("c", &[])]);
    feed.finish();
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));
    assert_eq!(marked(&harness), ["Side work", "Head work"]);
}

/// A history of 200 commits, c0 newest to c199 oldest.
fn long_history() -> Vec<CommitLine> {
    (0..200)
        .map(|n| {
            let parent = format!("c{}", n + 1);
            let parents: Vec<&str> = if n < 199 {
                vec![parent.as_str()]
            } else {
                vec![]
            };
            commit_line(&format!("c{n}"), &parents)
        })
        .collect()
}

fn selected_hash(harness: &Harness<'_, App>, name: &str) -> bool {
    let short = fake_id(name).short(7);
    selected(harness)
        .first()
        .is_some_and(|row| row.ends_with(&short))
}

#[test]
fn next_and_previous_select_the_matches_and_scroll_to_them() {
    let backend = FakeBackend::default()
        .with_repository(root())
        .with_history(root(), long_history())
        .with_matches(SearchKind::Message, "fix", ids(&["c3", "c150"]));
    let mut harness = open_with(backend, "Loading");
    type_text(&mut harness, "fix");
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));

    harness.get_by_label("Next").click();
    wait_until(&mut harness, |h| selected_hash(h, "c3"));
    harness.get_by_label("Next").click();
    // Far below the rows in view at first: selected and scrolled to.
    wait_until(&mut harness, |h| selected_hash(h, "c150"));
    harness.get_by_label("Previous").click();
    wait_until(&mut harness, |h| selected_hash(h, "c3"));
}

#[test]
fn enter_in_the_search_field_goes_to_the_next_match() {
    let backend = backend().with_matches(SearchKind::Message, "work", ids(&["x", "c"]));
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));
    press(&mut harness, Key::Enter, Modifiers::NONE);
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Side work"))
    });
}

#[test]
fn a_search_without_matches_says_so_in_the_search_view() {
    let mut harness = open(backend());
    type_text(&mut harness, "nothing");
    let search = sidebar_item(&harness, "Search");
    click_at(&mut harness, search);
    wait_until(&mut harness, |h| text_shown(h, "Nothing was found."));
}

fn focused(harness: &Harness<'_, App>) -> Option<Id> {
    harness.ctx.memory(|memory| memory.focused())
}

/// The areas of the Search view are the sidebar and the matches; the
/// commit list and the panels below it are not drawn there.
#[test]
fn tab_moves_between_the_sidebar_and_the_matches_of_the_search_view() {
    let backend = backend().with_matches(SearchKind::Message, "work", ids(&["x", "c"]));
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));
    let search = sidebar_item(&harness, "Search");
    click_at(&mut harness, search);
    wait_until(&mut harness, |h| {
        h.query_all_by_role(Role::ListItem).count() == 2
    });
    assert_eq!(focused(&harness), Some(Id::new(AREA_SIDEBAR)));

    let mut unnamed = unnamed_tab_stops(&mut harness, 1);
    assert_eq!(focused(&harness), Some(Id::new(SEARCH_RESULTS)));
    unnamed.extend(unnamed_tab_stops(&mut harness, 1));
    assert_eq!(focused(&harness), Some(Id::new(AREA_SIDEBAR)));
    assert!(unnamed.is_empty(), "{unnamed:#?}");
}

#[test]
fn tab_in_the_search_view_without_matches_reaches_only_named_areas() {
    let mut harness = open(backend());
    type_text(&mut harness, "nothing");
    let search = sidebar_item(&harness, "Search");
    click_at(&mut harness, search);
    wait_until(&mut harness, |h| text_shown(h, "Nothing was found."));
    let unnamed = unnamed_tab_stops(&mut harness, 4);
    assert!(unnamed.is_empty(), "{unnamed:#?}");
}

#[test]
fn a_match_chosen_in_the_search_view_is_selected_in_the_history() {
    let backend = backend().with_matches(SearchKind::Message, "work", ids(&["x", "c"]));
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));
    let search = sidebar_item(&harness, "Search");
    click_at(&mut harness, search);
    wait_until(&mut harness, |h| {
        h.query_all_by_role(Role::ListItem).count() == 2
    });
    let second = harness
        .query_all_by_role(Role::ListItem)
        .nth(1)
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, second);
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Head work"))
    });
}

#[test]
fn a_match_beyond_the_loaded_history_is_selected_once_it_has_loaded() {
    let feed = HistoryFeed::new();
    let backend = with_contents(
        FakeBackend::default()
            .with_repository(root())
            .with_history_feed(root(), &feed)
            .with_matches(SearchKind::Message, "base", ids(&["b"])),
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
    feed.send(history().into_iter().take(2));
    wait_until(&mut harness, |h| has_row(h, "Head work"));
    type_text(&mut harness, "base");
    wait_until(&mut harness, |h| text_shown(h, "1 match"));
    harness.get_by_label("Next").click();
    harness.step();

    feed.send(history().into_iter().skip(2));
    feed.finish();
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Base"))
    });
}

#[test]
fn typing_quickly_starts_one_search_for_the_whole_text() {
    let backend = backend();
    let probe = backend.probe();
    let mut harness = open(backend);
    let at = field_center(&harness);
    click_at(&mut harness, at);
    for letter in ["l", "a", "n", "e"] {
        harness.event(Event::Text(letter.to_owned()));
        harness.step();
    }
    wait_until(&mut harness, |_| !probe.searches().is_empty());
    for _ in 0..200 {
        harness.step();
    }
    assert_eq!(probe.searches(), [(SearchKind::Message, "lane".to_owned())]);
}

fn searches(probe: &Probe) -> usize {
    probe.searches().len()
}

#[test]
fn new_input_stops_the_running_search_and_its_marks_go() {
    let feed = HistoryFeed::new();
    let backend = backend()
        .with_search_feed(SearchKind::Message, "work", &feed)
        .with_matches(SearchKind::Message, "workbase", ids(&["b"]));
    let probe = backend.probe();
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    feed.send([commit_line("x", &[])]);
    wait_until(&mut harness, |h| marked(h) == ["Side work"]);

    type_text(&mut harness, "base");
    assert!(feed.was_cancelled());
    // The new text is taken after the frame that received it.
    harness.step();
    assert!(marked(&harness).is_empty());
    wait_until(&mut harness, |h| marked(h) == ["Base"]);
    assert_eq!(searches(&probe), 2);
}

#[test]
fn clearing_the_search_field_stops_the_search_and_removes_the_marks() {
    let feed = HistoryFeed::new();
    let backend = backend().with_search_feed(SearchKind::Message, "work", &feed);
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    feed.send([commit_line("x", &[])]);
    wait_until(&mut harness, |h| marked(h) == ["Side work"]);

    clear(&mut harness);

    assert!(feed.was_cancelled());
    assert!(marked(&harness).is_empty());
}

#[test]
fn ctrl_f_focuses_the_search_field() {
    let mut harness = open(backend());
    assert_eq!(
        field(&harness).accesskit_node().placeholder(),
        Some("Search commits…")
    );
    assert!(!field(&harness).is_focused());
    press(&mut harness, Key::F, Modifiers::COMMAND);
    assert!(field(&harness).is_focused());
}

/// The rows of the sidebar that are selected.
fn selected_items(harness: &Harness<'_, App>) -> Vec<String> {
    harness
        .query_all_by_role(Role::TreeItem)
        .filter(|node| node.accesskit_node().is_selected() == Some(true))
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

#[test]
fn a_match_chosen_in_the_search_view_selects_history_in_the_sidebar() {
    let backend = backend().with_matches(SearchKind::Message, "work", ids(&["x", "c"]));
    let mut harness = open(backend);
    type_text(&mut harness, "work");
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));
    let search = sidebar_item(&harness, "Search");
    click_at(&mut harness, search);
    wait_until(&mut harness, |h| {
        h.query_all_by_role(Role::ListItem).count() == 2
    });
    assert_eq!(selected_items(&harness), ["Search"]);

    let first = harness
        .query_all_by_role(Role::ListItem)
        .next()
        .unwrap()
        .rect()
        .center();
    click_at(&mut harness, first);
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Side work"))
    });
    assert_eq!(selected_items(&harness), ["History"]);
}

#[test]
fn next_keeps_a_branch_selected_in_the_sidebar() {
    let backend = backend().with_matches(SearchKind::Message, "work", ids(&["x", "c"]));
    let mut harness = open(backend);
    let main = sidebar_item(&harness, "main");
    click_at(&mut harness, main);
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Head work"))
    });
    type_text(&mut harness, "work");
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));

    harness.get_by_label("Next").click();
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Side work"))
    });
    assert_eq!(selected_items(&harness), ["main"]);
}

#[test]
fn a_view_selected_with_the_arrow_keys_gives_way_when_next_shows_history() {
    let backend = backend().with_matches(SearchKind::Message, "work", ids(&["x", "c"]));
    let mut harness = open(backend);
    let history = sidebar_item(&harness, "History");
    click_at(&mut harness, history);
    press(&mut harness, Key::ArrowDown, Modifiers::NONE);
    press(&mut harness, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(selected_items(&harness), ["Search"]);
    assert!(has_row(&harness, "Base"), "the History view stays");

    type_text(&mut harness, "work");
    wait_until(&mut harness, |h| text_shown(h, "2 matches"));
    harness.get_by_label("Next").click();
    wait_until(&mut harness, |h| {
        selected(h)
            .first()
            .is_some_and(|row| row.starts_with("Side work"))
    });
    for _ in 0..3 {
        harness.step();
    }
    assert_eq!(selected_items(&harness), ["History"]);
}
