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

/// The colour vision the palettes are made for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColourVision {
    #[default]
    Standard,
    /// Protanopia and deuteranopia.
    RedGreen,
    /// Tritanopia.
    BlueYellow,
}

/// How large the whole interface is drawn, on top of the scaling of the
/// operating system. Stored as the number of percent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum InterfaceSize {
    #[default]
    Percent100,
    Percent115,
    Percent130,
    Percent150,
}

impl InterfaceSize {
    /// Every size, from the smallest to the largest.
    pub const ALL: [InterfaceSize; 4] = [
        InterfaceSize::Percent100,
        InterfaceSize::Percent115,
        InterfaceSize::Percent130,
        InterfaceSize::Percent150,
    ];

    /// The next larger size; the largest stays.
    pub fn larger(self) -> InterfaceSize {
        let index = Self::ALL.iter().position(|size| *size == self).unwrap_or(0);
        Self::ALL[(index + 1).min(Self::ALL.len() - 1)]
    }

    /// The next smaller size; the smallest stays.
    pub fn smaller(self) -> InterfaceSize {
        let index = Self::ALL.iter().position(|size| *size == self).unwrap_or(0);
        Self::ALL[index.saturating_sub(1)]
    }

    /// How much larger than 100 % the interface is drawn.
    pub fn factor(self) -> f32 {
        f32::from(self.percent()) / 100.0
    }

    pub fn percent(self) -> u16 {
        match self {
            InterfaceSize::Percent100 => 100,
            InterfaceSize::Percent115 => 115,
            InterfaceSize::Percent130 => 130,
            InterfaceSize::Percent150 => 150,
        }
    }
}

impl From<InterfaceSize> for u16 {
    fn from(size: InterfaceSize) -> u16 {
        size.percent()
    }
}

impl TryFrom<u16> for InterfaceSize {
    type Error = String;

    fn try_from(percent: u16) -> Result<InterfaceSize, String> {
        InterfaceSize::ALL
            .into_iter()
            .find(|size| size.percent() == percent)
            .ok_or_else(|| format!("no interface size of {percent} %"))
    }
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
    pub commit_details_height: Option<f32>,
    pub path_column: Option<f32>,
}

/// The worktrees the home tab last found in a repository, so that it shows
/// them before it has read them again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownWorktrees {
    pub repository: PathBuf,
    /// Its further worktrees, by their folders.
    pub worktrees: Vec<PathBuf>,
}

/// The base branch the user set for a repository (spec
/// `repository-manager`, requirement "Base branch").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryBase {
    /// The canonical path of the repository, by which the home tab tells
    /// repositories apart, whichever of its paths lists it.
    pub repository: PathBuf,
    /// A local branch, by its short name such as `dev`.
    pub branch: String,
}

/// Everything git-bull remembers between runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(deserialize_with = "or_default")]
    pub theme: ThemeSetting,
    #[serde(deserialize_with = "or_default")]
    pub colour_vision: ColourVision,
    #[serde(deserialize_with = "or_default")]
    pub interface_size: InterfaceSize,
    /// Whether the window keeps the system's title bar instead of git-bull's
    /// own; it takes effect at the next start.
    #[serde(deserialize_with = "or_default")]
    pub system_title_bar: bool,
    /// Whether the diff shows spaces, tabs and line endings.
    #[serde(deserialize_with = "or_default")]
    pub show_invisibles: bool,
    /// Whether file lists show a tree of folders instead of full paths.
    #[serde(deserialize_with = "or_default")]
    pub file_tree: bool,
    /// A language tag such as `en-US`.
    pub language: String,
    /// The Git executable chosen by the user, if any.
    pub git_path: Option<PathBuf>,
    /// Recently opened repositories, most recent first, each by its main
    /// worktree or the Git folder of a bare repository.
    pub recent: Vec<PathBuf>,
    /// Repositories the user pinned in the home tab, in the order pinned.
    #[serde(deserialize_with = "or_default")]
    pub pinned: Vec<PathBuf>,
    /// Repositories open in tabs when git-bull was closed, in tab order.
    pub tabs: Vec<PathBuf>,
    /// The tab shown when git-bull was closed; `None` is the home tab.
    pub active_tab: Option<usize>,
    pub window: Option<WindowGeometry>,
    pub layout: Layout,
    /// The worktrees last found in the pinned and recent repositories.
    #[serde(deserialize_with = "or_default")]
    pub worktrees: Vec<KnownWorktrees>,
    /// The base branches the user set for repositories.
    #[serde(deserialize_with = "or_default")]
    pub bases: Vec<RepositoryBase>,
}

impl Settings {
    /// How many recently opened repositories are remembered.
    pub const RECENT_LIMIT: usize = 20;

    /// Puts `repository` first in the list of recently opened repositories.
    pub fn remember(&mut self, repository: PathBuf) {
        self.recent.retain(|known| *known != repository);
        self.recent.insert(0, repository);
        self.recent.truncate(Self::RECENT_LIMIT);
    }

    /// Pins `repository`, last among the pinned ones.
    pub fn pin(&mut self, repository: PathBuf) {
        if !self.pinned.contains(&repository) {
            self.pinned.push(repository);
        }
    }

    pub fn unpin(&mut self, repository: &Path) {
        self.pinned.retain(|known| known != repository);
    }

    /// Forgets `paths`, a repository with every path it was known by, its
    /// canonical path among them, from the recent and the pinned
    /// repositories, the worktrees remembered and the base set for it.
    pub fn forget(&mut self, paths: &[PathBuf]) {
        self.recent.retain(|known| !paths.contains(known));
        self.pinned.retain(|known| !paths.contains(known));
        self.worktrees
            .retain(|known| !paths.contains(&known.repository));
        self.bases.retain(|base| !paths.contains(&base.repository));
    }

    /// The base the user set for the repository whose canonical path is
    /// `repository`.
    pub fn base_of(&self, repository: &Path) -> Option<&str> {
        self.bases
            .iter()
            .find(|base| base.repository == repository)
            .map(|base| base.branch.as_str())
    }

    /// Sets the base of the repository whose canonical path is
    /// `repository` to `branch`, or lets it be detected again with `None`.
    /// Returns whether anything changed.
    pub fn set_base(&mut self, repository: &Path, branch: Option<String>) -> bool {
        let before = self.bases.clone();
        self.bases.retain(|base| base.repository != repository);
        if let Some(branch) = branch {
            self.bases.push(RepositoryBase {
                repository: repository.to_owned(),
                branch,
            });
        }
        self.bases != before
    }

    /// Remembers that `repository` has `worktrees`, if it is pinned or
    /// recent, and drops what is remembered of repositories that are
    /// neither any more. Returns whether anything changed.
    pub fn remember_worktrees(&mut self, repository: PathBuf, worktrees: Vec<PathBuf>) -> bool {
        let before = self.worktrees.clone();
        let (recent, pinned) = (&self.recent, &self.pinned);
        self.worktrees.retain(|known| {
            recent.contains(&known.repository) || pinned.contains(&known.repository)
        });
        if recent.contains(&repository) || pinned.contains(&repository) {
            match self
                .worktrees
                .iter_mut()
                .find(|known| known.repository == repository)
            {
                Some(known) => known.worktrees = worktrees,
                None => self.worktrees.push(KnownWorktrees {
                    repository,
                    worktrees,
                }),
            }
        }
        self.worktrees != before
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: ThemeSetting::System,
            colour_vision: ColourVision::Standard,
            interface_size: InterfaceSize::Percent100,
            system_title_bar: false,
            show_invisibles: false,
            file_tree: false,
            language: "en-US".to_owned(),
            git_path: None,
            recent: Vec::new(),
            pinned: Vec::new(),
            tabs: Vec::new(),
            active_tab: None,
            window: None,
            layout: Layout::default(),
            worktrees: Vec::new(),
            bases: Vec::new(),
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
/// recent or pinned, and such a Git is located anew. The active tab stays the one it
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
    let worktrees = settings
        .worktrees
        .iter()
        .filter(|known| valid(&known.repository))
        .map(|known| KnownWorktrees {
            repository: known.repository.clone(),
            worktrees: kept(&known.worktrees),
        })
        .collect();
    let bases = settings
        .bases
        .iter()
        .filter(|base| valid(&base.repository))
        .cloned()
        .collect();
    Settings {
        git_path: settings.git_path.clone().filter(|path| valid(path)),
        recent: kept(&settings.recent),
        pinned: kept(&settings.pinned),
        worktrees,
        bases,
        active_tab: active_tab.filter(|_| !tabs.is_empty()),
        tabs,
        ..settings.clone()
    }
}

/// Reads a choice, or its default when the value is one this version does
/// not know, such as one of a later version, or has the wrong type. The
/// other settings are kept, and the file is not treated as invalid.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(value.try_into().unwrap_or_default())
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
            colour_vision: ColourVision::Standard,
            interface_size: InterfaceSize::Percent100,
            system_title_bar: false,
            show_invisibles: false,
            file_tree: false,
            language: "de-DE".to_owned(),
            git_path: Some(PathBuf::from("/opt/git/bin/git")),
            recent: vec![
                PathBuf::from("/work/git-bull"),
                PathBuf::from("/work/linux"),
            ],
            pinned: Vec::new(),
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
            worktrees: Vec::new(),
            bases: Vec::new(),
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

    fn write(file: &SettingsFile, text: &str) {
        std::fs::create_dir_all(file.path().parent().unwrap()).unwrap();
        std::fs::write(file.path(), text).unwrap();
    }

    #[test]
    fn colour_vision_and_interface_size_survive_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            colour_vision: ColourVision::RedGreen,
            interface_size: InterfaceSize::Percent115,
            ..example()
        };
        file.save(&settings).unwrap();

        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("colour_vision = \"red_green\""), "{text}");
        assert!(text.contains("interface_size = 115"), "{text}");
        assert_eq!(file.load().settings, settings);
    }

    #[test]
    fn interface_sizes_step_up_and_down_and_stop_at_the_ends() {
        use InterfaceSize::*;
        assert_eq!(Percent100.larger(), Percent115);
        assert_eq!(Percent115.larger(), Percent130);
        assert_eq!(Percent130.larger(), Percent150);
        assert_eq!(Percent150.larger(), Percent150);
        assert_eq!(Percent150.smaller(), Percent130);
        assert_eq!(Percent115.smaller(), Percent100);
        assert_eq!(Percent100.smaller(), Percent100);
    }

    #[test]
    fn an_interface_size_scales_by_its_percent() {
        for (size, factor) in [
            (InterfaceSize::Percent100, 1.0),
            (InterfaceSize::Percent115, 1.15),
            (InterfaceSize::Percent130, 1.3),
            (InterfaceSize::Percent150, 1.5),
        ] {
            assert!((size.factor() - factor).abs() < 1e-6, "{size:?}");
        }
    }

    #[test]
    fn every_colour_vision_and_interface_size_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        for (text, vision) in [
            ("standard", ColourVision::Standard),
            ("red_green", ColourVision::RedGreen),
            ("blue_yellow", ColourVision::BlueYellow),
        ] {
            write(&file, &format!("colour_vision = \"{text}\"\n"));
            assert_eq!(file.load().settings.colour_vision, vision, "{text}");
        }
        for (number, size) in [
            (100, InterfaceSize::Percent100),
            (115, InterfaceSize::Percent115),
            (130, InterfaceSize::Percent130),
            (150, InterfaceSize::Percent150),
        ] {
            write(&file, &format!("interface_size = {number}\n"));
            assert_eq!(file.load().settings.interface_size, size, "{number}");
        }
    }

    #[test]
    fn a_file_of_the_first_milestone_loads_with_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        write(
            &file,
            "theme = \"dark\"\n\
             language = \"de-DE\"\n\
             git_path = \"/opt/git/bin/git\"\n\
             recent = [\"/work/git-bull\", \"/work/linux\"]\n\
             tabs = [\"/work/git-bull\"]\n\
             active_tab = 0\n\
             \n\
             [window]\n\
             width = 1280.0\n\
             height = 800.0\n\
             position = [40.0, 60.0]\n\
             \n\
             [layout]\n\
             sidebar_width = 220.0\n\
             details_height = 300.0\n",
        );

        let loaded = file.load();

        assert!(!loaded.reset);
        assert_eq!(loaded.settings.colour_vision, ColourVision::Standard);
        assert_eq!(loaded.settings.interface_size, InterfaceSize::Percent100);
        assert_eq!(loaded.settings, example());
    }

    #[test]
    fn a_file_without_the_title_bar_setting_uses_the_own_title_bar() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        write(
            &file,
            "theme = \"dark\"\n\
             colour_vision = \"blue_yellow\"\n\
             tabs = [\"/work/git-bull\"]\n",
        );

        let loaded = file.load();

        assert!(!loaded.reset);
        assert!(!loaded.settings.system_title_bar);
        assert_eq!(loaded.settings.theme, ThemeSetting::Dark);
        assert_eq!(loaded.settings.colour_vision, ColourVision::BlueYellow);
        assert_eq!(loaded.settings.tabs, [PathBuf::from("/work/git-bull")]);
    }

    #[test]
    fn the_title_bar_setting_survives_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            system_title_bar: true,
            ..example()
        };
        file.save(&settings).unwrap();

        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("system_title_bar = true"), "{text}");
        assert_eq!(file.load().settings, settings);
    }

    #[test]
    fn a_file_without_the_setting_for_invisible_characters_hides_them() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        write(
            &file,
            "theme = \"dark\"\n\
             system_title_bar = true\n\
             tabs = [\"/work/git-bull\"]\n",
        );

        let loaded = file.load();

        assert!(!loaded.reset);
        assert!(!loaded.settings.show_invisibles);
        assert_eq!(loaded.settings.theme, ThemeSetting::Dark);
        assert!(loaded.settings.system_title_bar);
        assert_eq!(loaded.settings.tabs, [PathBuf::from("/work/git-bull")]);
    }

    #[test]
    fn the_setting_for_invisible_characters_survives_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            show_invisibles: true,
            ..example()
        };
        file.save(&settings).unwrap();

        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("show_invisibles = true"), "{text}");
        assert_eq!(file.load().settings, settings);
    }

    #[test]
    fn a_file_without_the_setting_for_trees_shows_flat_lists() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        write(
            &file,
            "theme = \"dark\"\n\
             show_invisibles = true\n\
             tabs = [\"/work/git-bull\"]\n",
        );

        let loaded = file.load();

        assert!(!loaded.reset);
        assert!(!loaded.settings.file_tree);
        assert_eq!(loaded.settings.theme, ThemeSetting::Dark);
        assert!(loaded.settings.show_invisibles);
        assert_eq!(loaded.settings.tabs, [PathBuf::from("/work/git-bull")]);
    }

    #[test]
    fn the_setting_for_trees_survives_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            file_tree: true,
            ..example()
        };
        file.save(&settings).unwrap();

        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("file_tree = true"), "{text}");
        assert_eq!(file.load().settings, settings);
    }

    #[test]
    fn a_layout_without_the_commit_details_and_the_path_column_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        write(
            &file,
            "[layout]\n\
             sidebar_width = 220.0\n\
             details_height = 300.0\n\
             commit_panel_width = 420.0\n\
             graph_column = 96.0\n\
             date_column = 140.0\n\
             author_column = 180.0\n\
             hash_column = 90.0\n",
        );

        let loaded = file.load();

        assert!(!loaded.reset);
        assert_eq!(
            loaded.settings.layout,
            Layout {
                sidebar_width: Some(220.0),
                details_height: Some(300.0),
                commit_panel_width: Some(420.0),
                graph_column: Some(96.0),
                date_column: Some(140.0),
                author_column: Some(180.0),
                hash_column: Some(90.0),
                commit_details_height: None,
                path_column: None,
            }
        );
    }

    #[test]
    fn the_commit_details_and_the_path_column_survive_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let settings = Settings {
            layout: Layout {
                commit_details_height: Some(150.0),
                path_column: Some(240.0),
                ..example().layout
            },
            ..example()
        };
        file.save(&settings).unwrap();

        let text = std::fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("commit_details_height = 150.0"), "{text}");
        assert!(text.contains("path_column = 240.0"), "{text}");
        assert_eq!(file.load().settings, settings);
    }

    #[test]
    fn an_unknown_value_takes_its_default_alone_and_leaves_the_file_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        for unknown in [
            "theme = \"sepia\"",
            "theme = 3",
            "colour_vision = \"monochrome\"",
            "colour_vision = true",
            "interface_size = 120",
            "interface_size = \"large\"",
            "system_title_bar = \"yes\"",
            "system_title_bar = 1",
            "show_invisibles = \"yes\"",
        ] {
            let text = format!("{unknown}\nlanguage = \"de-DE\"\nactive_tab = 2\n");
            write(&file, &text);

            let loaded = file.load();

            assert!(!loaded.reset, "{unknown}");
            assert_eq!(
                loaded.settings,
                Settings {
                    language: "de-DE".to_owned(),
                    active_tab: Some(2),
                    ..Settings::default()
                },
                "{unknown}"
            );
            assert_eq!(std::fs::read_to_string(file.path()).unwrap(), text);
        }
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

    fn known(repository: &str, worktrees: &[&str]) -> KnownWorktrees {
        KnownWorktrees {
            repository: PathBuf::from(repository),
            worktrees: worktrees.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn a_file_without_pinned_repositories_and_worktrees_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        write(&file, "theme = \"dark\"\nrecent = [\"/work/git-bull\"]\n");

        let loaded = file.load();

        assert!(!loaded.reset);
        assert!(loaded.settings.pinned.is_empty());
        assert!(loaded.settings.worktrees.is_empty());
        assert_eq!(loaded.settings.theme, ThemeSetting::Dark);
        assert_eq!(loaded.settings.recent, [PathBuf::from("/work/git-bull")]);
    }

    #[test]
    fn pinned_repositories_and_worktrees_survive_a_save_and_a_load() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let mut settings = example();
        settings.pin(PathBuf::from("/work/billing"));
        settings.pin(PathBuf::from("/work/git-bull"));
        assert!(settings.remember_worktrees(
            PathBuf::from("/work/git-bull"),
            vec![PathBuf::from("/work/git-bull/.claude/worktrees/fix")],
        ));
        file.save(&settings).unwrap();
        assert_eq!(file.load().settings, settings);
    }

    #[test]
    fn the_base_of_a_repository_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        let mut settings = example();
        assert!(settings.set_base(Path::new("/work/git-bull"), Some("dev".to_owned())));
        file.save(&settings).unwrap();
        let loaded = file.load().settings;
        assert_eq!(loaded, settings);
        assert_eq!(loaded.base_of(Path::new("/work/git-bull")), Some("dev"));
    }

    #[test]
    fn a_file_without_bases_detects_every_base_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let file = file_in(&dir);
        write(
            &file,
            "theme = \"dark\"
recent = [\"/work/git-bull\"]
",
        );
        let loaded = file.load();
        assert!(!loaded.reset);
        assert!(loaded.settings.bases.is_empty());
        assert_eq!(loaded.settings.theme, ThemeSetting::Dark);
    }

    #[test]
    fn a_base_is_kept_once_under_the_canonical_path() {
        let mut settings = Settings::default();
        let canonical = Path::new("/work/git-bull");
        settings.set_base(canonical, Some("main".to_owned()));
        assert!(settings.set_base(canonical, Some("dev".to_owned())));
        assert!(!settings.set_base(canonical, Some("dev".to_owned())));
        assert_eq!(settings.bases.len(), 1);
        assert_eq!(settings.base_of(canonical), Some("dev"));
        assert!(settings.set_base(canonical, None));
        assert_eq!(settings.base_of(canonical), None);
    }

    #[test]
    fn a_removed_repository_forgets_its_base() {
        let mut settings = Settings::default();
        settings.remember(PathBuf::from("/work/App"));
        settings.set_base(Path::new("/work/app"), Some("dev".to_owned()));
        settings.set_base(Path::new("/work/other"), Some("main".to_owned()));
        // Every path of the repository, its canonical one among them.
        settings.forget(&[PathBuf::from("/work/App"), PathBuf::from("/work/app")]);
        assert_eq!(settings.base_of(Path::new("/work/app")), None);
        assert_eq!(settings.base_of(Path::new("/work/other")), Some("main"));
    }

    #[test]
    fn a_pinned_repository_outlasts_the_limit_of_recent_ones() {
        let mut settings = Settings::default();
        settings.remember(PathBuf::from("/work/pinned"));
        settings.pin(PathBuf::from("/work/pinned"));
        for n in 0..Settings::RECENT_LIMIT {
            settings.remember(PathBuf::from(format!("/work/other-{n}")));
        }
        assert!(!settings.recent.contains(&PathBuf::from("/work/pinned")));
        assert_eq!(settings.pinned, [PathBuf::from("/work/pinned")]);
    }

    #[test]
    fn pinning_keeps_the_order_and_unpinning_removes() {
        let mut settings = Settings::default();
        for path in ["/work/b", "/work/a", "/work/b"] {
            settings.pin(PathBuf::from(path));
        }
        assert_eq!(
            settings.pinned,
            [PathBuf::from("/work/b"), PathBuf::from("/work/a")]
        );
        settings.unpin(Path::new("/work/b"));
        assert_eq!(settings.pinned, [PathBuf::from("/work/a")]);
    }

    #[test]
    fn a_removed_repository_is_neither_pinned_nor_recent() {
        let mut settings = Settings::default();
        settings.remember(PathBuf::from("/work/app"));
        settings.remember(PathBuf::from("/work/other"));
        settings.pin(PathBuf::from("/work/app"));
        settings.forget(&[PathBuf::from("/work/app")]);
        assert_eq!(settings.recent, [PathBuf::from("/work/other")]);
        assert!(settings.pinned.is_empty());
    }

    #[test]
    fn a_repository_known_through_its_worktrees_is_forgotten_whole() {
        let mut settings = Settings::default();
        // An earlier version recorded worktrees among the recent ones.
        for path in [
            "/work/app",
            "/work/other",
            "/work/app-fix",
            "/work/app-docs",
        ] {
            settings.remember(PathBuf::from(path));
        }
        settings.remember_worktrees(
            PathBuf::from("/work/app"),
            vec![
                PathBuf::from("/work/app-fix"),
                PathBuf::from("/work/app-docs"),
            ],
        );
        settings.forget(&[
            PathBuf::from("/work/app"),
            PathBuf::from("/work/app-fix"),
            PathBuf::from("/work/app-docs"),
        ]);
        assert_eq!(settings.recent, [PathBuf::from("/work/other")]);
        assert!(settings.worktrees.is_empty());
    }

    #[test]
    fn worktrees_are_remembered_only_for_known_repositories_and_report_a_change() {
        let mut settings = Settings::default();
        settings.remember(PathBuf::from("/work/app"));
        let found = vec![PathBuf::from("/work/app-fix")];
        assert!(settings.remember_worktrees(PathBuf::from("/work/app"), found.clone()));
        assert!(!settings.remember_worktrees(PathBuf::from("/work/app"), found));
        assert!(!settings.remember_worktrees(PathBuf::from("/work/unknown"), Vec::new()));
        assert_eq!(settings.worktrees, [known("/work/app", &["/work/app-fix"])]);
        // A repository that fell out of the recent ones loses them.
        for n in 0..Settings::RECENT_LIMIT {
            settings.remember(PathBuf::from(format!("/work/other-{n}")));
        }
        assert!(settings.remember_worktrees(PathBuf::from("/work/other-0"), Vec::new()));
        assert_eq!(settings.worktrees, [known("/work/other-0", &[])]);
    }

    #[cfg(unix)]
    #[test]
    fn pinned_paths_and_worktrees_that_are_not_valid_utf8_are_left_out() {
        use std::os::unix::ffi::OsStrExt;
        let invalid = PathBuf::from(std::ffi::OsStr::from_bytes(b"/work/\xff"));
        let mut settings = Settings::default();
        settings.remember(PathBuf::from("/work/app"));
        settings.pin(invalid.clone());
        settings.remember_worktrees(
            PathBuf::from("/work/app"),
            vec![invalid, PathBuf::from("/work/app-fix")],
        );
        let stored = storable(&settings);
        assert!(stored.pinned.is_empty());
        assert_eq!(stored.worktrees, [known("/work/app", &["/work/app-fix"])]);
    }
}
