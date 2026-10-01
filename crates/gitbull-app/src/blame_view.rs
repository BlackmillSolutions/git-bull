//! Blame: the content of a file as of a revision, with the commit that last
//! changed each block of lines in a margin, shown inside the tab (spec
//! `blame`).

use eframe::egui::accesskit::Role;
use eframe::egui::{
    Align2, Color32, FontId, Id, Rect, ScrollArea, Sense, Ui, WidgetInfo, WidgetType, pos2, vec2,
};
use fluent_bundle::FluentArgs;
use gitbull_core::blame::{BlameContent, BlameState};
use gitbull_core::workspace::Failure;
use gitbull_git::Error;
use gitbull_git::object_id::ObjectId;

use crate::app::App;
use crate::commit_list::{SHORT_HASH, color, local_date};
use crate::diff_view::{FONT_SIZE, ROW_HEIGHT, line_job};
use crate::file_history_view::header;
use crate::i18n::Msg;
use crate::theme::Palette;

/// The id of the area of the content, which takes the focus of the view.
pub const BLAME_AREA: &str = "blame-area";

/// The width of the margin with hash, author and date.
const MARGIN_WIDTH: f32 = 320.0;

/// How strongly a lane colour tints the band of a commit.
pub(crate) const BAND_TINT: f32 = 0.22;

/// The colour of the band of the lines of `commit`: the same for every
/// block of one commit.
pub(crate) fn band_color(commit: &ObjectId, palette: &Palette) -> Color32 {
    let lanes = &palette.lanes;
    let index = commit.as_bytes().first().copied().unwrap_or(0) as usize % lanes.len();
    color(lanes[index]).gamma_multiply(BAND_TINT)
}

/// The text of a margin entry.
struct Margin {
    commit: ObjectId,
    text: String,
}

fn failure_text(failure: &Failure) -> String {
    match failure {
        Failure::Git(error) => error.to_string(),
        Failure::Panic(message) => message.clone(),
    }
}

pub(crate) fn show(app: &mut App, ui: &mut Ui, palette: &Palette) {
    let text = |msg| app.texts.text(msg);
    let (back, binary, missing, loading) = (
        text(Msg::Back),
        text(Msg::BlameBinary),
        text(Msg::DiffMissingContent),
        text(Msg::RowLoading),
    );
    let (title, blame_failed) = {
        let blame = app
            .workspace()
            .and_then(|workspace| workspace.active())
            .and_then(|tab| tab.session())
            .and_then(|session| session.blame());
        let revision = blame
            .map(
                |blame| match ObjectId::from_hex(blame.revision().as_bytes()) {
                    Some(id) => id.short(SHORT_HASH),
                    None => blame.revision().to_owned(),
                },
            )
            .unwrap_or_default();
        let mut args = FluentArgs::new();
        args.set(
            "path",
            blame.map(|b| b.path().to_string()).unwrap_or_default(),
        );
        args.set("revision", revision);
        let title = app.texts.text_with(Msg::BlameTitle, Some(&args));
        let failed = blame.and_then(|blame| match blame.state() {
            BlameState::Failed(Failure::Git(Error::MissingContent { .. })) => Some(missing.clone()),
            BlameState::Failed(failure) => {
                let mut args = FluentArgs::new();
                args.set("error", failure_text(failure));
                Some(app.texts.text_with(Msg::BlameFailed, Some(&args)))
            }
            _ => None,
        });
        (title, failed)
    };
    let zone = app.time_zone.clone();
    if header(ui, &back, &title) {
        app.close_overlay();
        return;
    }
    let Some((session, _)) = app.active_view() else {
        return;
    };
    let Some(blame) = session.blame() else {
        return;
    };
    let lines = match blame.content() {
        BlameContent::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak(loading);
            });
            return;
        }
        BlameContent::Binary => {
            ui.label(binary);
            return;
        }
        BlameContent::Failed(Failure::Git(Error::MissingContent { .. })) => {
            ui.label(missing);
            return;
        }
        BlameContent::Failed(failure) => {
            ui.colored_label(color(palette.status_deleted), failure_text(failure));
            return;
        }
        BlameContent::Text(lines) => lines,
    };
    if let Some(failed) = blame_failed {
        ui.colored_label(color(palette.status_deleted), failed);
    }

    let font = FontId::monospace(FONT_SIZE);
    let digit = ui
        .painter()
        .layout_no_wrap("0".to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x;
    let number_width = lines.len().to_string().len().max(3) as f32 * digit + 12.0;
    let text_color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let mut chosen = None;
    ScrollArea::both()
        .id_salt(Id::new(BLAME_AREA))
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for index in range {
                let commit = blame.line_commit(index);
                // A block begins where the commit of the line above differs.
                let starts = index == 0 || blame.line_commit(index - 1) != commit;
                let margin = commit
                    .filter(|_| starts)
                    .and_then(|id| blame.commit(&id))
                    .map(|info| Margin {
                        commit: info.id,
                        text: format!(
                            "{}  {}  {}",
                            info.id.short(SHORT_HASH),
                            info.author,
                            local_date(info.time, &zone)
                        ),
                    });
                let spans = blame
                    .highlighting()
                    .and_then(|lines| lines.get(index))
                    .map(Vec::as_slice);
                let job = line_job(&lines[index], spans, &font, text_color);
                let galley = ui.painter().layout_job(job);
                let width = (MARGIN_WIDTH + number_width + galley.size().x + 12.0)
                    .max(ui.available_width());
                let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
                let margin_rect = Rect::from_min_size(rect.min, vec2(MARGIN_WIDTH, ROW_HEIGHT));
                if let Some(commit) = commit {
                    ui.painter()
                        .rect_filled(margin_rect, 0.0, band_color(&commit, palette));
                }
                if let Some(margin) = margin {
                    let response = ui.interact(
                        margin_rect,
                        Id::new(("blame-margin", index)),
                        Sense::click(),
                    );
                    response
                        .widget_info(|| WidgetInfo::labeled(WidgetType::Link, true, &margin.text));
                    let clipped = ui.painter().with_clip_rect(margin_rect);
                    clipped.text(
                        pos2(margin_rect.left() + 6.0, margin_rect.center().y),
                        Align2::LEFT_CENTER,
                        &margin.text,
                        FontId::proportional(FONT_SIZE),
                        text_color,
                    );
                    if response.clicked() {
                        chosen = Some(margin.commit);
                    }
                    response.on_hover_cursor(eframe::egui::CursorIcon::PointingHand);
                }
                let number = index + 1;
                let number_rect = Rect::from_min_size(
                    pos2(margin_rect.right(), rect.top()),
                    vec2(number_width, ROW_HEIGHT),
                );
                ui.painter().text(
                    pos2(number_rect.right() - 6.0, number_rect.center().y),
                    Align2::RIGHT_CENTER,
                    number.to_string(),
                    font.clone(),
                    weak,
                );
                let text_top = rect.center().y - galley.size().y / 2.0;
                ui.painter().galley(
                    pos2(number_rect.right() + 6.0, text_top),
                    galley,
                    text_color,
                );
                // Each line, for assistive technology.
                let line = ui.interact(
                    Rect::from_min_max(pos2(number_rect.left(), rect.top()), rect.max),
                    Id::new(("blame-line", index)),
                    Sense::hover(),
                );
                let label = format!("{number}: {}", lines[index]);
                line.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &label));
                ui.ctx()
                    .accesskit_node_builder(line.id, |node| node.set_role(Role::Code));
            }
        });
    if let Some(commit) = chosen {
        // The History view shows the commit, or tells why it cannot.
        app.show_view(gitbull_core::workspace::View::History);
        app.navigate_to_commit(commit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::LIGHT;
    use gitbull_testkit::fake_id;

    #[test]
    fn blocks_of_one_commit_share_their_colour() {
        let palette = &LIGHT;
        let (a, b) = (fake_id("a"), fake_id("b"));
        assert_eq!(band_color(&a, palette), band_color(&a, palette));
        assert_ne!(band_color(&a, palette), band_color(&b, palette));
    }
}
