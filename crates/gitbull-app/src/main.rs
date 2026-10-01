//! The git-bull desktop application.

// No console window next to the application on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use gitbull_app::app::{App, Parts, SystemPicker};
use gitbull_app::native::{NativeApp, find_fonts_in_background, viewport};
use gitbull_app::paths::{AppPaths, System};
use gitbull_app::theme::ThemeFollower;
use gitbull_core::git_setup::check_git;
use gitbull_core::settings::SettingsFile;
use gitbull_git::log::CommandLog;

/// The command log rotates at this size.
const LOG_LIMIT: u64 = 1024 * 1024;

fn main() -> eframe::Result {
    let paths = AppPaths::resolve(System::current(), |key| std::env::var_os(key))
        .expect("the system names a home folder");
    // Without the folder Git would look for hooks in a missing path, which
    // also runs none; creating it is best effort.
    let _ = std::fs::create_dir_all(&paths.empty_hooks);

    let settings_file = SettingsFile::new(paths.settings.clone());
    let loaded = settings_file.load();
    let log = Arc::new(CommandLog::new(paths.log.clone(), LOG_LIMIT));
    let hooks = paths.empty_hooks.clone();
    let checker = Box::new(move |configured: Option<&std::path::Path>| {
        App::git_parts(check_git(configured, hooks.clone(), Some(Arc::clone(&log))))
    });
    let theme = if cfg!(target_os = "linux") {
        ThemeFollower::new(false, linux_appearance())
    } else {
        ThemeFollower::new(true, None)
    };

    // `git-bull <path>` opens that repository.
    let open_at_start = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    let options = eframe::NativeOptions {
        viewport: viewport(&loaded.settings),
        ..Default::default()
    };
    eframe::run_native(
        "git-bull",
        options,
        Box::new(move |creation| {
            creation
                .egui_ctx
                .set_fonts(gitbull_app::fonts::definitions());
            let context = creation.egui_ctx.clone();
            let fonts_context = creation.egui_ctx.clone();
            let fonts = find_fonts_in_background(move || fonts_context.request_repaint());
            let notify = Arc::new(move || context.request_repaint());
            let app = App::new(Parts {
                settings_file,
                loaded,
                checker,
                notify,
                theme,
                picker: Box::new(SystemPicker),
                open_at_start,
                time_zone: jiff::tz::TimeZone::system(),
            });
            Ok(Box::new(NativeApp {
                app,
                fonts: Some(fonts),
            }))
        }),
    )
}

#[cfg(target_os = "linux")]
fn linux_appearance() -> Option<gitbull_app::theme::Appearance> {
    gitbull_app::theme::linux_desktop_appearance()
}

#[cfg(not(target_os = "linux"))]
fn linux_appearance() -> Option<gitbull_app::theme::Appearance> {
    None
}
