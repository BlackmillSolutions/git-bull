//! Settings that survive a restart, stored in one TOML file.
//!
//! Writing goes through a temporary file that then replaces the original,
//! so that a crash or a second git-bull saving at the same time never
//! leaves a half-written file. A file that cannot be read is renamed with
//! the suffix `.bak` and replaced by defaults.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Light, dark, or whatever the operating system uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSetting {
    #[default]
    System,
    Light,
    Dark,
}

/// Size and position of the main window, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub width: f32,
    pub height: f32,
    /// Absent where the system does not reveal it, as under Wayland.
    pub position: Option<[f32; 2]>,
}

/// Divider positions and column widths, in logical pixels. Absent values
/// leave the choice to the UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub sidebar_width: Option<f32>,
    pub details_height: Option<f32>,
    pub commit_panel_width: Option<f32>,
    pub graph_column: Option<f32>,
    pub date_column: Option<f32>,
    pub author_column: Option<f32>,
    pub hash_column: Option<f32>,
}

/// Everything git-bull remembers between runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeSetting,
    /// A language tag such as `en-US`.
    pub language: String,
    /// The Git executable chosen by the user, if any.
    pub git_path: Option<PathBuf>,
    /// Recently opened repositories, most recent first.
    pub recent: Vec<PathBuf>,
    /// Repositories open in tabs when git-bull was closed, in tab order.
    pub tabs: Vec<PathBuf>,
    pub active_tab: Option<usize>,
    pub window: Option<WindowGeometry>,
    pub layout: Layout,
}

impl Settings {
    /// How many recently opened repositories are remembered.
    pub const RECENT_LIMIT: usize = 20;

    /// Puts `root` first in the list of recently opened repositories.
    pub fn remember(&mut self, root: PathBuf) {
        self.recent.retain(|known| *known != root);
        self.recent.insert(0, root);
        self.recent.truncate(Self::RECENT_LIMIT);
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: ThemeSetting::System,
            language: "en-US".to_owned(),
            git_path: None,
            recent: Vec::new(),
            tabs: Vec::new(),
            active_tab: None,
            window: None,
            layout: Layout::default(),
        }
    }
}

/// The result of loading settings.
#[derive(Debug, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// The file could not be read and was renamed with the suffix `.bak`;
    /// the UI reports this once.
    pub reset: bool,
}

/// The file the settings live in.
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: PathBuf) -> SettingsFile {
        SettingsFile { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the settings; defaults when there is no file yet.
    pub fn load(&self) -> Loaded {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Loaded {
                    settings: Settings::default(),
                    reset: false,
                };
            }
            Err(_) => return self.reset(),
        };
        match toml::from_str(&text) {
            Ok(settings) => Loaded {
                settings,
                reset: false,
            },
            Err(_) => self.reset(),
        }
    }

    /// Writes the settings, replacing the file in one step. Paths that are
    /// not valid UTF-8 are left out; see [`storable`].
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        let text = toml::to_string_pretty(&storable(settings)).map_err(io::Error::other)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Unique per save, so that saves running at the same time never
        // write into each other's temporary file.
        static SAVES: AtomicU64 = AtomicU64::new(0);
        let temporary = with_suffix(
            &self.path,
            &format!(
                ".{}-{}.tmp",
                std::process::id(),
                SAVES.fetch_add(1, Ordering::Relaxed)
            ),
        );
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, &self.path).inspect_err(|_| {
            let _ = std::fs::remove_file(&temporary);
        })
    }

    fn reset(&self) -> Loaded {
        let backup = with_suffix(&self.path, ".bak");
        let _ = std::fs::remove_file(&backup);
        let _ = std::fs::rename(&self.path, &backup);
        Loaded {
            settings: Settings::default(),
            reset: true,
        }
    }
}

/// `settings` without the paths that are not valid UTF-8, which the file
/// cannot hold: such a repository is neither restored nor listed as
/// recent, and such a Git is located anew. The active tab stays the one it
/// was, or becomes the first when it is left out.
fn storable(settings: &Settings) -> Settings {
    fn valid(path: &Path) -> bool {
        path.to_str().is_some()
    }
    let kept = |paths: &[PathBuf]| -> Vec<PathBuf> {
        paths.iter().filter(|path| valid(path)).cloned().collect()
    };
    let tabs = kept(&settings.tabs);
    let left_out_before = |index: usize| {
        let before = &settings.tabs[..index.min(settings.tabs.len())];
        before.iter().filter(|path| !valid(path)).count()
    };
    let active_tab = settings
        .active_tab
        .map(|index| match settings.tabs.get(index) {
            Some(path) if !valid(path) => 0,
            _ => index - left_out_before(index),
        });
    Settings {
        git_path: settings.git_path.clone().filter(|path| valid(path)),
        recent: kept(&settings.recent),
        active_tab: active_tab.filter(|_| !tabs.is_empty()),
        tabs,
        ..settings.clone()
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_in(dir: &tempfile::TempDir) -> SettingsFile {
        SettingsFile::new(dir.path().join("config").join("settings.toml"))
    }

    fn example() -> Settings {
        Settings {
            theme: ThemeSetting::Dark,
            language: "de-DE".to_owned(),
            git_path: Some(PathBuf::from("/opt/git/bin/git")),
            recent: vec![
                PathBuf::from("/work/git-bull"),
                PathBuf::from("/work/linux"),
            ],
            tabs: vec![PathBuf::from("/work/git-bull")],
            active_tab: Some(0),
            window: Some(WindowGeometry {
                width: 1280.0,
                height: 800.0,
                position: Some([40.0, 60.0]),
            }),
            layout: Layout {
                sidebar_width: Some(220.0),
                details_height: Some(300.0),
                ..Layout::default()
            },
        }
    }

    fn repo(n: usize) -> PathBuf {
        PathBuf::from(format!("repo-{n}"))
    }

    #[test]
    fn newly_opened_repository_comes_first() {
        let mut settings = Settings::default();
        settings.remember(repo(1));
        settings.remember(repo(2));
        assert_eq!(settings.recent, [repo(2), repo(1)]);
    }

    #[test]
    fn list_keeps_the_twenty_most_recent() {
        let mut settings = Settings::default();
        for n in 1..=21 {
            settings.remember(repo(n));
        }
        assert_eq!(settings.recent.len(), 20);
        assert_eq!(settings.recent.first(), Some(&repo(21)));
        assert_eq!(settings.recent.last(), Some(&repo(2)));
    }

    #[test]
    fn reopening_moves_the_entry_to_the_top_once() {
        let mut settings = Settings::default();
        for n in 1..=3 {
            settings.remember(repo(n));
        }
        settings.remember(repo(1));
        assert_eq!(settings.recent, [repo(1), repo(3), repo(2)]);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            file_in(&dir).load(),
            Loaded {
                settings: Settings::default(),
                reset: false
            }
        );
    }

    #[test]
    fn saved_settings_load_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        file.save(&example()).unwrap();
        assert_eq!(
            file.load(),
            Loaded {
                settings: example(),
                reset: false
            }
        );
    }

    #[test]
    fn missing_entries_take_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
        std::fs::write(file.path(), "theme = \"light\"\n").unwrap();
        let loaded = file.load();
        assert_eq!(loaded.settings.theme, ThemeSetting::Light);
        assert_eq!(loaded.settings.language, "en-US");
        assert!(!loaded.reset);
    }

    #[test]
    fn invalid_file_is_kept_as_backup_and_replaced_by_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
        std::fs::write(file.path(), "theme = [unclosed").unwrap();

        let loaded = file.load();

        assert_eq!(
            loaded,
            Loaded {
                settings: Settings::default(),
                reset: true
            }
        );
        assert!(!file.path().exists());
        let backup = file.path().with_file_name("settings.toml.bak");
        assert_eq!(
            std::fs::read_to_string(backup).unwrap(),
            "theme = [unclosed"
        );
    }

    #[test]
    fn older_backup_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
        let backup = file.path().with_file_name("settings.toml.bak");
        std::fs::write(&backup, "old backup").unwrap();
        std::fs::write(file.path(), "not = [valid").unwrap();

        assert!(file.load().reset);
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "not = [valid");
    }

    #[test]
    fn two_instances_saving_in_turn_leave_a_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut first = example();
        first.language = "first".to_owned();
        let mut second = example();
        second.language = "second".to_owned();

        std::thread::scope(|scope| {
            for settings in [&first, &second] {
                let path = path.clone();
                scope.spawn(move || {
                    let file = SettingsFile::new(path);
                    for _ in 0..100 {
                        file.save(settings).unwrap();
                    }
                });
            }
        });

        let loaded = SettingsFile::new(path).load();
        assert!(!loaded.reset, "the file was damaged");
        assert!(loaded.settings == first || loaded.settings == second);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["settings.toml"], "temporary files remain");
    }

    /// A path to `name` in a folder whose name is not valid UTF-8.
    fn not_utf8(name: &str) -> PathBuf {
        #[cfg(unix)]
        let folder = {
            use std::os::unix::ffi::OsStrExt;
            std::ffi::OsStr::from_bytes(b"/work/caf\xe9").to_owned()
        };
        #[cfg(windows)]
        let folder = {
            use std::os::windows::ffi::OsStringExt;
            let mut wide: Vec<u16> = r"C:\work\caf".encode_utf16().collect();
            // A lone surrogate, which no UTF-8 can express.
            wide.push(0xD800);
            std::ffi::OsString::from_wide(&wide)
        };
        PathBuf::from(folder).join(name)
    }

    #[test]
    fn paths_that_are_not_utf8_are_left_out_and_the_rest_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            git_path: Some(not_utf8("git")),
            recent: vec![repo(1), not_utf8("one"), repo(2)],
            tabs: vec![not_utf8("one"), repo(1), not_utf8("two"), repo(2)],
            active_tab: Some(3),
            ..example()
        };
        file.save(&settings).unwrap();
        assert_eq!(
            file.load().settings,
            Settings {
                git_path: None,
                recent: vec![repo(1), repo(2)],
                tabs: vec![repo(1), repo(2)],
                active_tab: Some(1),
                ..example()
            }
        );
    }

    #[test]
    fn an_active_tab_that_is_left_out_makes_the_first_tab_active() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            tabs: vec![repo(1), not_utf8("one"), repo(2)],
            active_tab: Some(1),
            ..example()
        };
        file.save(&settings).unwrap();
        let loaded = file.load().settings;
        assert_eq!(loaded.tabs, [repo(1), repo(2)]);
        assert_eq!(loaded.active_tab, Some(0));
    }

    #[test]
    fn without_tabs_left_no_tab_is_active() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            tabs: vec![not_utf8("one")],
            active_tab: Some(0),
            ..example()
        };
        file.save(&settings).unwrap();
        let loaded = file.load().settings;
        assert!(loaded.tabs.is_empty());
        assert_eq!(loaded.active_tab, None);
    }
}
