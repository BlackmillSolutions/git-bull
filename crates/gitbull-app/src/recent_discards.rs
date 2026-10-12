//! What the File status view shows of the discards beside the file list: the
//! notice bar that names the last discard of the tab and offers to undo it
//! (spec `discard`, requirement "Undoing a discard"), and the keys that undo
//! and redo.

use eframe::egui::{Key, Modifiers, Ui};
use fluent_bundle::FluentArgs;
use gitbull_core::session::{DiscardRecord, Session};
use gitbull_git::journal::{Kind, State};

use crate::app::{App, TabView};
use crate::components::{self, BannerAction, BannerKind};
use crate::i18n::Msg;

/// The record the notice bar is drawn for: the last discard of the tab, while
/// the user has not closed the bar for it in its state.
fn bar_record<'a>(session: &'a Session, view: &TabView) -> Option<&'a DiscardRecord> {
    session
        .discard_notice()
        .filter(|record| view.notice_closed.as_ref() != Some(&(record.id.clone(), record.state)))
}

/// Whether anything of the discards is drawn beside the file list.
pub(crate) fn shown(app: &mut App) -> bool {
    app.active_view()
        .is_some_and(|(session, view)| bar_record(session, view).is_some())
}

/// What the bar says: "Discarded 3 files", "Restored 3 files", and for a part
/// of a file "Discarded part of a.rs".
fn bar_text(app: &App, record: &DiscardRecord) -> String {
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
        let name = record
            .files
            .first()
            .map(|(path, _)| path.to_string())
            .unwrap_or_default();
        args.set("name", name);
    } else {
        args.set("count", record.files.len() as i64);
    }
    app.texts.text_with(msg, Some(&args))
}

/// Draws the notice bar: the last discard of the tab, with "Undo" while it is
/// discarded and "Discard again" once it was undone, and a button that closes
/// the bar.
pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    let Some(record) = app
        .active_view()
        .and_then(|(session, view)| bar_record(session, view).cloned())
    else {
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
    let chosen = ui
        .add_enabled_ui(!busy, |ui| {
            components::banner(ui, BannerKind::Information, &text, &[&action], &dismiss)
        })
        .inner;
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
