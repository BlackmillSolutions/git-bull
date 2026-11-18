//! What the File status view shows of the discards beside the file list: the
//! notice bar that names the last discard of the tab and offers to undo it
//! (spec `discard`, requirement "Undoing a discard"), and the keys that undo
//! and redo.

use eframe::egui::{self, Align, EventFilter, Id, Key, Label, Layout, Modifiers, Ui};
use fluent_bundle::FluentArgs;
use gitbull_core::session::{DiscardRecord, Session};
use gitbull_git::journal::{Kind, Running, State};

use crate::app::{App, TabView};
use crate::commit_form;
use crate::commit_list::local_date;
use crate::components::{self, BannerAction, BannerKind};
use crate::file_status_view::STATUS_LIST;
use crate::i18n::Msg;
use crate::ui::AREA_DIFF;

/// Where the group keeps the controls Tab stops at, in their order, from
/// one pass to the next.
const STOPS: &str = "recent-discards-stops";

/// The controls of the notice bar and the list that Tab stops at, in the
/// order of the page, as drawn last: those that are available. Empty while
/// nothing is drawn.
pub(crate) fn stops(ui: &Ui) -> Vec<Id> {
    ui.data(|data| data.get_temp(Id::new(STOPS)))
        .unwrap_or_default()
}

/// Where the group keeps the height of the bar and the header, which it
/// needs whatever room it gets.
const BASE: &str = "recent-discards-base";

/// How high the bar and the header were when they were drawn last: the room
/// the commit form leaves them. Zero while they are not drawn.
pub(crate) fn base_height(ui: &Ui) -> f32 {
    ui.data(|data| data.get_temp(Id::new(BASE))).unwrap_or(0.0)
}

/// Nothing is drawn in this pass: what the last one left for Tab and for the
/// layout is gone.
pub(crate) fn forget(ui: &Ui) {
    ui.data_mut(|data| {
        data.remove_temp::<Vec<Id>>(Id::new(STOPS));
        data.remove_temp::<f32>(Id::new(BASE));
    });
}

/// The record the notice bar is drawn for: the last discard of the tab, while
/// the user has not closed the bar for it in its state.
fn bar_record<'a>(session: &'a Session, view: &TabView) -> Option<&'a DiscardRecord> {
    session
        .discard_notice()
        // An entry that an exchange left half way offers neither Undo nor
        // Discard again; the list offers "Put back".
        .filter(|record| matches!(record.state, State::Discarded | State::Restored))
        .filter(|record| view.notice_closed.as_ref() != Some(&(record.id.clone(), record.state)))
}

/// Whether anything of the discards is drawn beside the file list: the notice
/// bar, or the list while the journal has entries.
pub(crate) fn shown(app: &mut App) -> bool {
    app.active_view().is_some_and(|(session, view)| {
        bar_record(session, view).is_some() || !session.journal().is_empty()
    })
}

/// What the bar says: "Discarded 3 files", "Restored 3 files", and for a part
/// of a file "Discarded part of a.rs".
/// What the bar needs of the record, so that the paths of a discard of many
/// thousand files are not copied in every frame.
struct Bar {
    id: String,
    state: State,
    kind: Kind,
    files: usize,
    first: String,
}

fn bar_text(app: &App, record: &Bar) -> String {
    let part = matches!(record.kind, Kind::Hunk | Kind::Lines);
    let restored = record.state == State::Restored;
    let mut args = FluentArgs::new();
    let msg = match (part, restored) {
        (true, false) => Msg::NoticeDiscardedPart,
        (true, true) => Msg::NoticeRestoredPart,
        (false, false) => Msg::NoticeDiscarded,
        (false, true) => Msg::NoticeRestored,
    };
    if part {
        args.set("name", record.first.clone());
    } else {
        args.set("count", record.files as i64);
    }
    app.texts.text_with(msg, Some(&args))
}

/// Draws what the discards show beside the file list: the notice bar and the
/// list "Recently discarded". `room` is the height the file list leaves them
/// and the commit form: the bar and the header always show, and the rows of
/// the list as many as fit in what is left.
pub(crate) fn show(app: &mut App, ui: &mut Ui, room: f32) {
    let mut now: Vec<Id> = Vec::new();
    bar(app, ui, &mut now);
    list(app, ui, &mut now, room);
    tab(ui, &now);
    ui.data_mut(|data| data.insert_temp(Id::new(STOPS), now));
}

/// Tab moves through the controls that are available, from the file list to
/// the first and on from the last to the commit form, or to the diff when
/// there is none; Shift+Tab goes back, from the first to the file list. It
/// is taken before egui moves the focus itself.
fn tab(ui: &Ui, now: &[Id]) {
    let focused = ui.memory(|memory| memory.focused());
    let Some(at) = focused.and_then(|id| now.iter().position(|stop| *stop == id)) else {
        return;
    };
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            now[at],
            EventFilter {
                tab: true,
                ..EventFilter::default()
            },
        );
    });
    // Shift first: a plain Tab pattern also matches Tab with Shift.
    let target = if ui.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::Tab)) {
        Some(match at {
            0 => Id::new(STATUS_LIST),
            _ => now[at - 1],
        })
    } else if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Tab)) {
        Some(
            now.get(at + 1)
                .copied()
                .or_else(|| commit_form::stops(ui).first().copied())
                .unwrap_or(Id::new(AREA_DIFF)),
        )
    } else {
        None
    };
    if let Some(target) = target {
        ui.memory_mut(|memory| memory.request_focus(target));
    }
}

/// Draws the notice bar: the last discard of the tab, with "Undo" while it is
/// discarded and "Discard again" once it was undone, and a button that closes
/// the bar.
fn bar(app: &mut App, ui: &mut Ui, now: &mut Vec<Id>) {
    let Some(record) = app.active_view().and_then(|(session, view)| {
        bar_record(session, view).map(|record| Bar {
            id: record.id.clone(),
            state: record.state,
            kind: record.kind,
            files: record.files.len(),
            first: record
                .files
                .first()
                .map(|(path, _)| path.to_string())
                .unwrap_or_default(),
        })
    }) else {
        return;
    };
    let text = bar_text(app, &record);
    let action = app.texts.text(match record.state {
        State::Restored => Msg::NoticeDiscardAgain,
        _ => Msg::NoticeUndo,
    });
    let dismiss = app.texts.text(Msg::NoticeDismiss);
    let Some((session, view)) = app.active_view() else {
        return;
    };
    // Nothing is asked while an action runs.
    let busy = session.action().is_some();
    let (chosen, stops) = ui
        .add_enabled_ui(!busy, |ui| {
            components::banner(ui, BannerKind::Information, &text, &[&action], &dismiss)
        })
        .inner;
    now.extend(stops);
    match chosen {
        Some(BannerAction::Action(_)) => {
            match record.state {
                State::Restored => session.redo_discard(),
                _ => session.undo_discard(),
            };
        }
        Some(BannerAction::Dismiss) => {
            view.notice_closed = Some((record.id, record.state));
        }
        None => {}
    }
}

/// Ctrl+Z, or Cmd+Z on macOS, restores the last discard of the tab, and
/// Ctrl+Shift+Z, or Cmd+Shift+Z, discards it again, while the File status
/// view is shown and no text field has the keyboard focus. A text field keeps
/// its own undo.
pub(crate) fn keys(app: &mut App, ui: &Ui) {
    if ui.ctx().text_edit_focused() {
        return;
    }
    // The window behind a dialog takes no keys.
    if app.dialog.is_some()
        || app.close_question.is_some()
        || app.create_dialog.is_some()
        || app.has_discard_dialog()
        || app.has_clear_question()
        || app.detach_pending.is_some()
        || app.branch_choice.is_some()
        || app.action_dialog().is_some()
    {
        return;
    }
    // Shift first: a plain pattern also matches the key with Shift.
    let redo =
        ui.input_mut(|input| input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z));
    let undo = !redo && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Z));
    if let Some((session, _)) = app.active_view() {
        if redo {
            session.redo_discard();
        } else if undo {
            session.undo_discard();
        }
    }
}

/// How many rows of entries the list shows before it scrolls.
const LIST_ROWS: usize = 5;

/// How many file names an entry of the list gives before it counts the rest.
const NAMES: usize = 3;

/// What an entry of the list draws.
struct Entry {
    id: String,
    /// When it last changed, such as `2026-10-12 09:30`.
    time: String,
    /// What it holds, such as `2 files: a.rs, b.rs`.
    what: String,
    /// Its state: discarded, restored, interrupted or running.
    state: String,
    /// The button of the entry: its label and what it does.
    button: Option<(String, EntryAction)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryAction {
    Restore,
    PutBack,
}

fn entry_of(app: &App, record: &DiscardRecord) -> Entry {
    let texts = &app.texts;
    let what = match record.kind {
        Kind::Hunk | Kind::Lines => {
            let mut args = FluentArgs::new();
            args.set(
                "name",
                record
                    .files
                    .first()
                    .map(|(path, _)| path.to_string())
                    .unwrap_or_default(),
            );
            texts.text_with(Msg::RecentEntryPart, Some(&args))
        }
        Kind::Files | Kind::Group => {
            let mut names: Vec<String> = record
                .files
                .iter()
                .take(NAMES)
                .map(|(path, _)| path.to_string())
                .collect();
            if record.files.len() > NAMES {
                names.push("…".to_owned());
            }
            let mut args = FluentArgs::new();
            args.set("count", record.files.len() as i64);
            args.set("names", names.join(", "));
            texts.text_with(Msg::RecentEntryFiles, Some(&args))
        }
    };
    let (state, button) = match (record.running, record.state) {
        (Running::Running, _) => (Msg::RecentStateRunning, None),
        (Running::Interrupted, _) => (
            Msg::RecentStateInterrupted,
            Some((Msg::RecentPutBack, EntryAction::PutBack)),
        ),
        (Running::Settled, State::Restored) => (
            Msg::RecentStateRestored,
            Some((Msg::NoticeDiscardAgain, EntryAction::Restore)),
        ),
        (Running::Settled, _) => (
            Msg::RecentStateDiscarded,
            Some((Msg::RecentRestore, EntryAction::Restore)),
        ),
    };
    Entry {
        id: record.id.clone(),
        time: local_date(record.changed as i64, &app.time_zone),
        what,
        state: texts.text(state),
        button: button.map(|(label, action)| (texts.text(label), action)),
    }
}

/// Draws the section "Recently discarded": a header that opens and closes it
/// and, while it is open, "Clear" and the entries of the journal, each with
/// its time, what it holds, its state and the button that fits the state.
fn list(app: &mut App, ui: &mut Ui, now: &mut Vec<Id>, room: f32) {
    let Some((open, busy)) = app
        .active_view()
        .map(|(session, view)| (view.discards_open, session.action().is_some()))
    else {
        return;
    };
    let Some((count, running)) = app.active_view().map(|(session, _)| {
        let journal = session.journal();
        let running = journal
            .entries
            .iter()
            .any(|record| record.running == Running::Running);
        (journal.entries.len() + journal.unreadable.len(), running)
    }) else {
        return;
    };
    if count == 0 {
        return;
    }
    let unreadable: Vec<String> = if open {
        app.workspace()
            .and_then(|w| w.active())
            .and_then(|t| t.session())
            .map(|session| {
                session
                    .journal()
                    .unreadable
                    .iter()
                    .map(|item| {
                        let mut args = FluentArgs::new();
                        args.set("folder", item.folder.clone());
                        args.set("why", item.why.clone());
                        app.texts.text_with(Msg::RecentUnreadable, Some(&args))
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let entries: Vec<Entry> = if open {
        match app
            .workspace()
            .and_then(|w| w.active())
            .and_then(|t| t.session())
        {
            Some(session) => session
                .journal()
                .entries
                .iter()
                .map(|record| entry_of(app, record))
                .collect(),
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let mut args = FluentArgs::new();
    args.set("count", count as i64);
    let title = app.texts.text_with(Msg::RecentTitle, Some(&args));
    let clear = app.texts.text(Msg::RecentClear);

    let mut toggle = false;
    let mut ask_clear = false;
    let mut chosen: Option<(String, EntryAction)> = None;
    ui.horizontal(|ui| {
        let header = components::Button::new(&title)
            .kind(components::Kind::Ghost)
            .show(ui);
        ui.ctx().accesskit_node_builder(header.id, |node| {
            node.set_expanded(open);
        });
        toggle = header.clicked();
        now.push(header.id);
        if open {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Refused while an entry is being worked on by another
                // instance, and while an action runs.
                ui.add_enabled_ui(!running && !busy, |ui| {
                    let button = components::Button::new(&clear).show(ui);
                    ask_clear = button.clicked();
                    if ui.is_enabled() {
                        now.push(button.id);
                    }
                });
            });
        }
    });
    let base = ui.min_rect().height();
    ui.data_mut(|data| data.insert_temp(Id::new(BASE), base));
    // The list has the height of its rows, up to a few, and scrolls beyond;
    // the panel it lies in does not give it a height of its own. In a small
    // window it gets what the bar and the header leave of the room.
    let row = crate::theme::SHAPE.control_height + ui.spacing().item_spacing.y;
    let free = room - ui.min_rect().height() - ui.spacing().item_spacing.y;
    let rows = (entries.len() + unreadable.len())
        .min(LIST_ROWS)
        .min((free.max(0.0) / row).floor() as usize);
    if open && rows > 0 {
        let height = rows as f32 * row;
        let width = ui.available_width();
        ui.allocate_ui(egui::vec2(width, height), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("recent-discards")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for text in &unreadable {
                        ui.add(Label::new(text).truncate().selectable(false));
                    }
                    for entry in &entries {
                        ui.horizontal(|ui| {
                            ui.weak(&entry.time);
                            ui.label(&entry.state);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if let Some((label, action)) = &entry.button {
                                    ui.add_enabled_ui(!busy, |ui| {
                                        let button = components::Button::new(label).show(ui);
                                        // Its name stays what it does; the entry it acts
                                        // on is its description, so that the buttons of
                                        // several entries are told apart.
                                        let described = format!("{}, {}", entry.time, entry.what);
                                        ui.ctx().accesskit_node_builder(button.id, |node| {
                                            node.set_description(described);
                                        });
                                        if button.clicked() {
                                            chosen = Some((entry.id.clone(), *action));
                                        }
                                        if ui.is_enabled() {
                                            now.push(button.id);
                                        }
                                    });
                                }
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    // The text gives way to the buttons; the whole of
                                    // it is in the tooltip.
                                    ui.add(Label::new(&entry.what).truncate().selectable(false))
                                        .on_hover_text(&entry.what);
                                });
                            });
                        });
                    }
                });
        });
    }
    let Some((session, view)) = app.active_view() else {
        return;
    };
    if toggle {
        view.discards_open = !view.discards_open;
    }
    if ask_clear {
        view.clear_question = true;
    }
    match chosen {
        Some((id, EntryAction::Restore)) => {
            session.restore(&id);
        }
        Some((id, EntryAction::PutBack)) => {
            session.put_back(&id);
        }
        None => {}
    }
}
