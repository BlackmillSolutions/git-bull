//! Builds the application with a fake backend for UI tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use gitbull_app::app::{App, GitChecker, GitStatus, Parts, Picker};
use gitbull_app::theme::ThemeFollower;
use gitbull_app::ui;
use gitbull_core::git_setup::GitCheck;
use gitbull_core::settings::{Loaded, Settings, SettingsFile};
use gitbull_core::workspace::TabState;
use gitbull_git::Error;
use gitbull_git::locate::LocateError;
use gitbull_git::version::GitVersion;
use gitbull_testkit::FakeBackend;
use tempfile::TempDir;

pub const GIT_VERSION: GitVersion = GitVersion {
    major: 2,
    minor: 55,
    patch: 0,
};

pub fn path(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

/// A picker that answers with fixed paths, or cancels.
pub struct FixedPicker {
    pub folder: Option<PathBuf>,
    pub git: Option<PathBuf>,
}

impl Picker for FixedPicker {
    fn pick_folder(&self) -> Option<PathBuf> {
        self.folder.clone()
    }

    fn pick_git(&self) -> Option<PathBuf> {
        self.git.clone()
    }
}

/// A checker for which Git is always usable, with `backend`.
pub fn usable_git(backend: FakeBackend) -> GitChecker {
    let backend: Arc<dyn gitbull_git::Backend> = Arc::new(backend);
    Box::new(move |_| {
        (
            GitStatus::Ready {
                version: GIT_VERSION,
            },
            Some(Arc::clone(&backend)),
        )
    })
}

/// An application whose settings live in a temporary folder.
pub struct TestApp {
    pub dir: TempDir,
    pub app: App,
}

/// How a test application is set up.
#[derive(Default)]
pub struct Setup {
    pub settings: Settings,
    pub backend: FakeBackend,
    pub picker: Option<PathBuf>,
    pub picked_git: Option<PathBuf>,
    pub open_at_start: Option<PathBuf>,
    /// Replaces the always usable Git.
    pub checker: Option<GitChecker>,
}

pub fn build(setup: Setup) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(Parts {
        settings_file: SettingsFile::new(dir.path().join("settings.toml")),
        loaded: Loaded {
            settings: setup.settings,
            reset: false,
        },
        checker: setup.checker.unwrap_or_else(|| usable_git(setup.backend)),
        notify: Arc::new(|| {}),
        theme: ThemeFollower::new(true, None),
        picker: Box::new(FixedPicker {
            folder: setup.picker,
            git: setup.picked_git,
        }),
        open_at_start: setup.open_at_start,
    });
    TestApp { dir, app }
}

/// An application with `settings` and the given repositories.
pub fn app(settings: Settings, backend: FakeBackend) -> TestApp {
    build(Setup {
        settings,
        backend,
        ..Setup::default()
    })
}

/// An application with one repository open at `work/git-bull`.
pub fn app_with_open_repository(mut settings: Settings) -> TestApp {
    settings.tabs = vec![path(&["work", "git-bull"])];
    settings.active_tab = Some(0);
    let mut test = app(
        settings,
        FakeBackend::default().with_repository(path(&["work", "git-bull"])),
    );
    settle(&mut test.app);
    test
}

/// Runs the application's logic until no tab is opening any more.
pub fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.logic();
        if !opening(app) {
            return;
        }
        assert!(Instant::now() < deadline, "tabs did not finish opening");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn opening(app: &App) -> bool {
    app.workspace().is_some_and(|workspace| {
        workspace
            .tabs()
            .iter()
            .any(|tab| matches!(tab.state(), TabState::Opening))
    })
}

/// Steps the window until no tab is opening any more.
///
/// While a tab opens, its spinner keeps asking for frames, so the window
/// is stepped frame by frame: `Harness::run` gives up on a UI that does not
/// settle within a few frames, which a slow machine hits.
pub fn settle_window(harness: &mut Harness<'_, App>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if !opening(harness.state()) {
            harness.run();
            return;
        }
        assert!(Instant::now() < deadline, "tabs did not finish opening");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The titles of the open tabs.
pub fn tab_titles(app: &App) -> Vec<String> {
    app.workspace()
        .map(|workspace| workspace.tabs().iter().map(|tab| tab.title()).collect())
        .unwrap_or_default()
}

/// The title of the active tab.
pub fn active_title(app: &App) -> Option<String> {
    app.workspace()
        .and_then(|workspace| workspace.active())
        .map(|tab| tab.title())
}

/// A folder dropped onto the window.
#[derive(Debug)]
pub struct Dropped(pub PathBuf);

impl eframe::egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Err("a folder has no bytes".to_owned())
    }
}

/// A window of 1280 × 800 logical pixels showing `app`.
pub fn window(app: App) -> Harness<'static, App> {
    window_on(eframe::egui::os::OperatingSystem::from_target_os(), app)
}

/// Like [`window`], with egui behaving as on `os`.
pub fn window_on(os: eframe::egui::os::OperatingSystem, app: App) -> Harness<'static, App> {
    Harness::builder()
        .with_size((1280.0, 800.0))
        .with_os(os)
        .build_ui_state(
            |ui, app: &mut App| {
                app.logic();
                ui::show(app, ui);
            },
            app,
        )
}

/// A checker whose answer depends on the path it is asked about and can be
/// changed while the test runs.
#[derive(Clone)]
pub struct Scripted(Arc<Mutex<Script>>);

type Script = Box<dyn Fn(Option<&Path>) -> Answer + Send>;

pub enum Answer {
    Usable,
    Missing,
    TooOld,
    NotGit,
}

impl Scripted {
    pub fn new(answer: impl Fn(Option<&Path>) -> Answer + Send + 'static) -> Scripted {
        Scripted(Arc::new(Mutex::new(Box::new(answer))))
    }

    pub fn answer_with(&self, answer: impl Fn(Option<&Path>) -> Answer + Send + 'static) {
        *self.0.lock().unwrap() = Box::new(answer);
    }

    pub fn checker(&self) -> GitChecker {
        let script = self.clone();
        Box::new(move |path| {
            let path_buf = path
                .map(Path::to_owned)
                .unwrap_or_else(|| PathBuf::from("git"));
            match (script.0.lock().unwrap())(path) {
                Answer::Usable => (
                    GitStatus::Ready {
                        version: GIT_VERSION,
                    },
                    Some(Arc::new(FakeBackend::default())),
                ),
                Answer::Missing => (
                    GitStatus::Problem(GitCheck::NotFound(LocateError::NotFound)),
                    None,
                ),
                Answer::TooOld => (
                    GitStatus::Problem(GitCheck::TooOld {
                        path: path_buf,
                        version: GitVersion {
                            major: 2,
                            minor: 30,
                            patch: 0,
                        },
                    }),
                    None,
                ),
                Answer::NotGit => (
                    GitStatus::Problem(GitCheck::Unusable {
                        path: path_buf,
                        error: Error::Parse {
                            command: "git --version".into(),
                            message: "not a Git version".into(),
                            bytes: b"Hello".to_vec(),
                        },
                    }),
                    None,
                ),
            }
        })
    }
}
