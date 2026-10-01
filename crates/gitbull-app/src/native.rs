//! Running the application in a native window with eframe.

use std::sync::mpsc::Receiver;

use eframe::egui::{self, Pos2, Vec2, ViewportBuilder};
use gitbull_core::settings::{Settings, WindowGeometry};

use crate::app::App;
use crate::fonts::{self, Fallback};

/// The window size on the first start.
const FIRST_SIZE: [f32; 2] = [1280.0, 800.0];

/// The window as the settings remember it.
pub fn viewport(settings: &Settings) -> ViewportBuilder {
    let (size, position) = match settings.window {
        Some(window) => ([window.width, window.height], window.position),
        None => (FIRST_SIZE, None),
    };
    let mut viewport = ViewportBuilder::default()
        .with_title("git-bull")
        .with_app_id("git-bull")
        .with_inner_size(Vec2::from(size))
        .with_min_inner_size([640.0, 400.0])
        .with_drag_and_drop(true);
    if let Some([x, y]) = position {
        viewport = viewport.with_position(Pos2::new(x, y));
    }
    viewport
}

/// The geometry to remember, from what the window reports. The position is
/// absent where the system does not reveal it, as under Wayland.
///
/// egui-winit reports the window in points of the zoom factor `zoom`, while
/// [`viewport`] opens the window in logical pixels before any zoom applies;
/// the geometry is therefore stored without the zoom (design, decision 7).
pub fn geometry(info: &egui::ViewportInfo, content_size: Vec2, zoom: f32) -> WindowGeometry {
    let size = info.inner_rect.map_or(content_size, |rect| rect.size()) * zoom;
    WindowGeometry {
        width: size.x,
        height: size.y,
        position: info
            .outer_rect
            .map(|rect| [rect.min.x * zoom, rect.min.y * zoom]),
    }
}

/// eframe's view of the application.
pub struct NativeApp {
    pub app: App,
    /// Fallback fonts while they are still being searched for.
    pub fonts: Option<Receiver<Vec<Fallback>>>,
}

/// Searches the system's fonts in the background; scanning them can take a
/// moment, and the window should not wait for it.
pub fn find_fonts_in_background(notify: impl Fn() + Send + 'static) -> Receiver<Vec<Fallback>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if sender.send(fonts::system_fallbacks()).is_ok() {
            notify();
        }
    });
    receiver
}

impl eframe::App for NativeApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (info, content) = ctx.input(|input| (input.viewport().clone(), input.content_rect()));
        if let Some(found) = self.fonts.as_ref().and_then(|fonts| fonts.try_recv().ok()) {
            fonts::install(ctx, &found);
            self.fonts = None;
        }
        self.app
            .record_window(geometry(&info, content.size(), ctx.zoom_factor()));
        self.app.logic();
        if let Some(due) = self.app.save_due_in() {
            ctx.request_repaint_after(due);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::ui::show(&mut self.app, ui);
    }

    fn on_exit(&mut self) {
        self.app.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Rect, pos2, vec2};

    #[test]
    fn remembered_size_and_position_open_the_window_there() {
        let settings = Settings {
            window: Some(WindowGeometry {
                width: 1000.0,
                height: 700.0,
                position: Some([40.0, 60.0]),
            }),
            ..Settings::default()
        };
        let viewport = viewport(&settings);
        assert_eq!(viewport.inner_size, Some(vec2(1000.0, 700.0)));
        assert_eq!(viewport.position, Some(pos2(40.0, 60.0)));
    }

    #[test]
    fn remembered_size_without_position_leaves_the_position_to_the_system() {
        let settings = Settings {
            window: Some(WindowGeometry {
                width: 1000.0,
                height: 700.0,
                position: None,
            }),
            ..Settings::default()
        };
        let viewport = viewport(&settings);
        assert_eq!(viewport.inner_size, Some(vec2(1000.0, 700.0)));
        assert_eq!(viewport.position, None);
    }

    #[test]
    fn first_start_opens_a_window_of_the_default_size_anywhere() {
        let viewport = viewport(&Settings::default());
        assert_eq!(viewport.inner_size, Some(vec2(1280.0, 800.0)));
        assert_eq!(viewport.position, None);
        assert_eq!(viewport.drag_and_drop, Some(true));
    }

    #[test]
    fn window_geometry_comes_from_the_inner_size_and_outer_position() {
        let info = egui::ViewportInfo {
            inner_rect: Some(Rect::from_min_size(pos2(48.0, 90.0), vec2(1100.0, 720.0))),
            outer_rect: Some(Rect::from_min_size(pos2(40.0, 60.0), vec2(1116.0, 758.0))),
            ..Default::default()
        };
        assert_eq!(
            geometry(&info, vec2(1.0, 1.0), 1.0),
            WindowGeometry {
                width: 1100.0,
                height: 720.0,
                position: Some([40.0, 60.0]),
            }
        );
    }

    #[test]
    fn geometry_reported_at_a_larger_interface_size_gives_back_the_same_window() {
        // A window of 1200 by 750 logical pixels at 30, 60, as egui-winit
        // reports it in the points of the zoom factor 1.5.
        let zoom = 1.5;
        let info = egui::ViewportInfo {
            inner_rect: Some(Rect::from_min_size(
                pos2(38.0, 90.0) / zoom,
                vec2(1200.0, 750.0) / zoom,
            )),
            outer_rect: Some(Rect::from_min_size(
                pos2(30.0, 60.0) / zoom,
                vec2(1216.0, 788.0) / zoom,
            )),
            ..Default::default()
        };
        let settings = Settings {
            window: Some(geometry(&info, vec2(1.0, 1.0), zoom)),
            ..Settings::default()
        };
        let viewport = viewport(&settings);
        assert_eq!(viewport.inner_size, Some(vec2(1200.0, 750.0)));
        assert_eq!(viewport.position, Some(pos2(30.0, 60.0)));
    }

    #[test]
    fn unknown_window_position_is_not_remembered() {
        let info = egui::ViewportInfo::default();
        assert_eq!(
            geometry(&info, vec2(900.0, 600.0), 1.0),
            WindowGeometry {
                width: 900.0,
                height: 600.0,
                position: None,
            }
        );
    }
}
