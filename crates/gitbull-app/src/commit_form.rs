//! The commit form of the File status view (spec `commit`): the message of
//! a commit, the boxes for amending and for committing without hooks, and the
//! button. It is not the commit panel, which shows the details of a commit
//! in the History view.

use eframe::egui::{
    self, EventFilter, Id, Key, Label, Modifiers, RichText, Ui, WidgetInfo, WidgetType,
};
use fluent_bundle::FluentArgs;
use gitbull_core::commit_draft::{AmendUnavailable, CommitOffer, CommitUnavailable};
use gitbull_core::file_status::StatusState;
use gitbull_core::session::Action as WriteAction;
use gitbull_git::commit::Operation;
use gitbull_git::head::Head;

use crate::app::App;
use crate::commit_list::color;
use crate::components;
use crate::file_status_view::STATUS_LIST;
use crate::i18n::Msg;
use crate::icons;

use crate::theme::{Palette, SHAPE};
use crate::ui::{AREA_DIFF, action_text, operation_msg};

/// The field of the summary.
pub const SUMMARY: &str = "commit-form-summary";
/// The field of the description.
pub const DESCRIPTION: &str = "commit-form-description";
/// Where the form keeps the controls Tab stops at, in their order, from one
/// pass to the next.
const STOPS: &str = "commit-form-stops";
/// Where the form keeps its height.
const HEIGHT: &str = "commit-form-height";
/// The height the form leaves to the file list above it in a window too
/// small for both: the title of the list, its filter and five rows, which
/// are the titles of the two groups and three files.
pub(crate) const LIST_ROOM: f32 = 178.0;
/// A summary longer than this is marked; Git's tools show about this much.
const SUMMARY_LENGTH: usize = 72;

/// How high the form was when it was drawn last, if it was: what it holds,
/// whatever room it was given.
pub(crate) fn height(ui: &Ui) -> Option<f32> {
    ui.data(|data| data.get_temp(Id::new(HEIGHT)))
}

/// The controls of the form that Tab stops at, in their order, as drawn
/// last: those that are available. Empty while no form is shown.
pub(crate) fn stops(ui: &Ui) -> Vec<Id> {
    ui.data(|data| data.get_temp(Id::new(STOPS)))
        .unwrap_or_default()
}

/// Whether the active tab shows a status, and with it the form.
pub(crate) fn shown(app: &App) -> bool {
    app.workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .and_then(|session| session.file_status())
        .is_some_and(|status| matches!(status.state(), StatusState::Loaded(_)))
}

/// What the form says, found before the session is borrowed to change it.
struct Texts {
    amend: String,
    amend_reason: Option<String>,
    summary: String,
    description: String,
    without_hooks: String,
    /// The label of the button, whatever it would do.
    button: String,
    /// Why the button does nothing, if it does nothing.
    reason: Option<String>,
    /// What runs, while a commit runs.
    running: Option<String>,
    stop: String,
    notices: Vec<(&'static str, String)>,
}

fn texts(app: &App) -> Option<Texts> {
    let session = app.workspace()?.active()?.session()?;
    let text = |msg| app.texts.text(msg);
    let counted = |msg, count: usize| {
        let mut args = FluentArgs::new();
        args.set("count", count as i64);
        app.texts.text_with(msg, Some(&args))
    };
    let staged = match session.file_status()?.state() {
        StatusState::Loaded(status) => status.staged.len(),
        _ => return None,
    };
    let offer = session.commit_offer();
    let button = match offer {
        CommitOffer::Commit { files } => counted(Msg::CommitFiles, files),
        CommitOffer::ConcludeMerge { .. } => text(Msg::CommitMerge),
        CommitOffer::StageAllAndCommit => text(Msg::CommitStageAll),
        CommitOffer::Amend => text(Msg::CommitAmend),
        // What it would do, were it available.
        CommitOffer::Unavailable(_) if session.draft().amend() => text(Msg::CommitAmend),
        CommitOffer::Unavailable(_) if session.operation() == Some(Operation::Merge) => {
            text(Msg::CommitMerge)
        }
        CommitOffer::Unavailable(_) => counted(Msg::CommitFiles, staged),
    };
    let reason = match offer {
        CommitOffer::Unavailable(why) => Some(text(match why {
            CommitUnavailable::StatusUnknown => Msg::CommitStatusUnknown,
            CommitUnavailable::Busy => Msg::CommitBusy,
            CommitUnavailable::Rebase => Msg::CommitRebase,
            CommitUnavailable::ApplyingPatches => Msg::CommitPatches,
            CommitUnavailable::Conflicts => Msg::CommitConflicts,
            CommitUnavailable::NoSummary => Msg::CommitNoSummary,
            CommitUnavailable::NothingToCommit => Msg::CommitNothing,
        })),
        _ => None,
    };
    let running = match session.action() {
        Some(action @ WriteAction::Commit { .. }) => Some(action_text(app, action)),
        _ => None,
    };
    let amend_reason = session.amend_unavailable().map(|why| match why {
        AmendUnavailable::NoCommit => text(Msg::AmendNoCommit),
        AmendUnavailable::InProgress(operation) => {
            let mut args = FluentArgs::new();
            args.set("operation", text(operation_msg(operation)));
            app.texts.text_with(Msg::AmendInProgress, Some(&args))
        }
    });
    let mut notices = Vec::new();
    match session.operation() {
        Some(Operation::Merge) => notices.push((icons::INFO, text(Msg::CommitDuringMerge))),
        Some(Operation::CherryPick) => {
            notices.push((icons::INFO, text(Msg::CommitDuringCherryPick)));
        }
        Some(Operation::Revert) => notices.push((icons::INFO, text(Msg::CommitDuringRevert))),
        // A rebase and a series of patches close the button, whose line
        // says so.
        Some(Operation::Rebase | Operation::ApplyingPatches) | None => {}
    }
    if matches!(session.opened().head, Head::Detached(_)) {
        notices.push((icons::WARNING, text(Msg::CommitDetached)));
    }
    if session.amends_a_pushed_commit() {
        notices.push((icons::WARNING, text(Msg::CommitOnRemote)));
    }
    Some(Texts {
        amend: text(Msg::CommitAmend),
        amend_reason,
        summary: text(Msg::CommitSummary),
        description: text(Msg::CommitDescription),
        without_hooks: text(Msg::CommitWithoutHooks),
        button,
        reason,
        running,
        stop: text(Msg::CommitStop),
        notices,
    })
}

/// A line of the form that says something: `icon`, then `text`, which is
/// what assistive technology reads.
fn notice(ui: &mut Ui, icon: &str, text: &str, colour: egui::Color32) {
    ui.horizontal_top(|ui| {
        components::icon(ui, icon, 14.0, colour);
        ui.add(Label::new(RichText::new(text).color(colour)).wrap());
    });
}

/// No form is drawn in this pass: what the last one left for Tab and for
/// the layout is gone, so that Tab from the file list does not look for a
/// control that is not there.
pub(crate) fn forget(ui: &Ui) {
    ui.data_mut(|data| {
        data.remove_temp::<Vec<Id>>(Id::new(STOPS));
        data.remove_temp::<f32>(Id::new(HEIGHT));
    });
}

/// Forgets what a text field keeps to undo: its text was replaced from
/// outside, and undoing would bring back what is no longer the draft.
fn forget_undo(ui: &Ui, field: Id) {
    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), field) {
        state.clear_undoer();
        state.store(ui.ctx(), field);
    }
}

/// Draws the commit form of the active tab.
pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let Some(texts) = texts(app) else {
        forget(ui);
        return;
    };
    let tab = app
        .workspace()
        .and_then(|workspace| workspace.active())
        .map(|tab| tab.id());
    let count_name = |app: &App, count: usize| {
        let mut args = FluentArgs::new();
        args.set("count", count as i64);
        let msg = match count > SUMMARY_LENGTH {
            true => Msg::CommitSummaryLong,
            false => Msg::CommitSummaryLength,
        };
        app.texts.text_with(msg, Some(&args))
    };
    let draft = app
        .workspace()
        .and_then(|workspace| workspace.active())
        .and_then(|tab| tab.session())
        .map(|session| session.draft().clone())
        .unwrap_or_default();
    let count = draft.summary().chars().count();
    let count_name = count_name(app, count);
    let Some((session, view)) = app.active_view() else {
        return;
    };
    // Each tab has fields of its own: a field keeps its cursor and what it
    // can undo by its id, and another tab holds another message.
    let (summary_id, description_id) = (Id::new((SUMMARY, tab)), Id::new((DESCRIPTION, tab)));
    // The draft is not what the fields left in the last pass: a commit
    // emptied it, or a message was filled in.
    let written = (draft.summary().to_owned(), draft.description().to_owned());
    if view
        .commit_form_seen
        .as_ref()
        .is_some_and(|seen| *seen != written)
    {
        forget_undo(ui, summary_id);
        forget_undo(ui, description_id);
    }

    // Tab moves through the controls that are available and on to the
    // areas beside the form. It is taken before the fields see it, so that
    // the description gets no tab character.
    let known = stops(ui);
    let focused = ui.memory(|memory| memory.focused());
    let at = focused.and_then(|id| known.iter().position(|stop| *stop == id));
    let mut go = None;
    let mut submit = false;
    if let Some(at) = at {
        // Shift first: a plain Tab pattern also matches Tab with Shift.
        if ui.input_mut(|input| input.consume_key(Modifiers::SHIFT, Key::Tab)) {
            go = Some(match at {
                0 => Id::new(STATUS_LIST),
                _ => known[at - 1],
            });
        } else if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Tab)) {
            go = Some(known.get(at + 1).copied().unwrap_or(Id::new(AREA_DIFF)));
        }
        let in_field = focused == Some(summary_id) || focused == Some(description_id);
        // With the modifier first: the plain pattern matches it too.
        if in_field && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Enter)) {
            submit = true;
        } else if focused == Some(summary_id)
            && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter))
        {
            go = Some(description_id);
        }
    }

    let committing = texts.running.is_some();
    let mut now = Vec::new();
    let width = ui.available_width();
    let top = ui.cursor().top();
    ui.add_space(SHAPE.space[1]);

    // "Amend last commit".
    let mut amend = draft.amend();
    let amend_enabled = !committing && texts.amend_reason.is_none();
    let amend_box = ui
        .add_enabled_ui(amend_enabled, |ui| {
            components::checkbox(ui, &mut amend, &texts.amend)
        })
        .inner;
    if let Some(reason) = &texts.amend_reason {
        amend_box.clone().on_disabled_hover_text(reason);
    }
    if amend_enabled {
        now.push(amend_box.id);
    }
    if amend != draft.amend() {
        session.set_amend(amend);
    }

    for (icon, text) in &texts.notices {
        let colour = match *icon == icons::WARNING {
            true => color(palette.warning_fg),
            false => color(palette.text_muted),
        };
        notice(ui, icon, text, colour);
    }

    // The summary, with the count of its characters at its end.
    let mut summary = draft.summary().to_owned();
    let long = count > SUMMARY_LENGTH;
    ui.horizontal(|ui| {
        let counter = format!("{count}");
        let counter_width = 44.0;
        let field = ui.add_enabled(
            !committing,
            components::text_edit(
                &mut summary,
                &texts.summary,
                width - counter_width - SHAPE.space[1],
            )
            .id(summary_id),
        );
        ui.ctx().accesskit_node_builder(field.id, |node| {
            node.set_label(texts.summary.as_str());
        });
        if !committing {
            now.push(field.id);
        }
        // Marked by an icon, not by its colour alone.
        let colour = match long {
            true => color(palette.warning_fg),
            false => color(palette.text_muted),
        };
        if long {
            components::icon(ui, icons::WARNING, 14.0, colour);
        }
        let shown = ui.label(RichText::new(counter).color(colour));
        shown.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &count_name));
    });
    if summary != draft.summary() {
        session.set_summary(summary.clone());
    }

    // The description.
    let mut description = draft.description().to_owned();
    let field = ui.add_enabled(
        !committing,
        egui::TextEdit::multiline(&mut description)
            .id(description_id)
            .hint_text(texts.description.as_str())
            .desired_rows(4)
            .desired_width(width),
    );
    ui.ctx().accesskit_node_builder(field.id, |node| {
        node.set_label(texts.description.as_str());
    });
    if !committing {
        now.push(field.id);
    }
    if description != draft.description() {
        session.set_description(description.clone());
    }
    view.commit_form_seen = Some((summary, description));

    // "Commit without hooks".
    let mut without_hooks = draft.without_hooks();
    let hooks_box = ui
        .add_enabled_ui(!committing, |ui| {
            components::checkbox(ui, &mut without_hooks, &texts.without_hooks)
        })
        .inner;
    if !committing {
        now.push(hooks_box.id);
    }
    if without_hooks != draft.without_hooks() {
        session.set_without_hooks(without_hooks);
    }

    // The button, or what runs with Stop.
    let mut stop = false;
    if let Some(running) = &texts.running {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("{running}…"));
            let button = components::Button::new(&texts.stop)
                .kind(components::Kind::Secondary)
                .show(ui);
            now.push(button.id);
            if std::mem::take(&mut view.focus_commit_stop) {
                button.request_focus();
            }
            stop = button.clicked();
        });
    } else {
        let available = texts.reason.is_none();
        let button = ui
            .add_enabled_ui(available, |ui| {
                components::Button::new(&texts.button)
                    .kind(components::Kind::Primary)
                    .show(ui)
            })
            .inner;
        if available {
            now.push(button.id);
        }
        submit |= button.clicked();
        if let Some(reason) = &texts.reason {
            notice(ui, icons::INFO, reason, color(palette.text_muted));
        }
    }
    ui.add_space(SHAPE.space[1]);

    if stop {
        session.stop_commit();
    }
    if submit && session.commit().is_ok() {
        // Started in the form, with its button or from a field, the keyboard
        // stays in it: the control that had the focus is unavailable from now
        // on, so Stop takes it.
        view.commit_from_form = true;
        view.focus_commit_stop = true;
    }
    // The commit ended: the keyboard goes on in the summary. Behind a
    // dialog it waits until the dialog closed.
    let runs = matches!(session.action(), Some(WriteAction::Commit { .. }));
    if std::mem::replace(&mut view.commit_ran, runs) && !runs {
        view.focus_commit_stop = false;
        if std::mem::take(&mut view.commit_from_form) && session.dialog().is_none() {
            view.focus_commit_summary = true;
        }
    }
    if std::mem::take(&mut view.focus_commit_summary) && !runs {
        go = Some(summary_id);
    }
    if let Some(target) = go {
        ui.memory_mut(|memory| memory.request_focus(target));
    }
    // egui leaves Tab to the form while one of its controls has the focus,
    // and the fields keep the focus on Escape and their arrow keys.
    if let Some(id) = focused.filter(|id| now.contains(id)) {
        let field = id == summary_id || id == description_id;
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                id,
                EventFilter {
                    tab: true,
                    horizontal_arrows: field,
                    vertical_arrows: field,
                    escape: field,
                },
            );
        });
    }
    let height = ui.cursor().top() - top;
    ui.data_mut(|data| {
        data.insert_temp(Id::new(STOPS), now);
        data.insert_temp(Id::new(HEIGHT), height);
    });
}
