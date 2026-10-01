//! Where git-bull keeps its files, following each system's conventions.
//!
//! | File | Windows | macOS | Linux |
//! |---|---|---|---|
//! | Settings | `%APPDATA%\git-bull` | `~/Library/Application Support/git-bull` | `$XDG_CONFIG_HOME/git-bull` |
//! | Log, hooks folder | `%LOCALAPPDATA%\git-bull` | `~/Library/Application Support/git-bull` | `$XDG_STATE_HOME/git-bull` |

use std::ffi::OsString;
use std::path::PathBuf;

/// The files git-bull uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    pub settings: PathBuf,
    pub log: PathBuf,
    /// An empty folder that Git is pointed at instead of the repository's
    /// hooks (ADR 0006).
    pub empty_hooks: PathBuf,
}

/// The operating systems with different conventions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Windows,
    MacOs,
    Linux,
}

impl System {
    pub fn current() -> System {
        if cfg!(windows) {
            System::Windows
        } else if cfg!(target_os = "macos") {
            System::MacOs
        } else {
            System::Linux
        }
    }
}

impl AppPaths {
    /// The paths for `system`, reading environment variables through `var`.
    /// `None` when the system gives no home or application folder.
    pub fn resolve(system: System, var: impl Fn(&str) -> Option<OsString>) -> Option<AppPaths> {
        // Empty values count as unset, as the XDG specification requires.
        let dir = |key: &str| var(key).filter(|v| !v.is_empty()).map(PathBuf::from);
        let (config, state) = match system {
            System::Windows => (dir("APPDATA")?, dir("LOCALAPPDATA")?),
            System::MacOs => {
                let support = dir("HOME")?.join("Library").join("Application Support");
                (support.clone(), support)
            }
            System::Linux => {
                let home = dir("HOME");
                let config =
                    dir("XDG_CONFIG_HOME").or_else(|| Some(home.clone()?.join(".config")))?;
                let state =
                    dir("XDG_STATE_HOME").or_else(|| Some(home?.join(".local").join("state")))?;
                (config, state)
            }
        };
        let (config, state) = (config.join("git-bull"), state.join("git-bull"));
        Some(AppPaths {
            settings: config.join("settings.toml"),
            log: state.join("git-bull.log"),
            empty_hooks: state.join("empty-hooks"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |key| map.get(key).cloned()
    }

    fn joined(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    #[test]
    fn windows_keeps_settings_roaming_and_the_rest_local() {
        let paths = AppPaths::resolve(
            System::Windows,
            env(&[("APPDATA", "roaming"), ("LOCALAPPDATA", "local")]),
        )
        .unwrap();
        assert_eq!(
            paths.settings,
            joined(&["roaming", "git-bull", "settings.toml"])
        );
        assert_eq!(paths.log, joined(&["local", "git-bull", "git-bull.log"]));
        assert_eq!(
            paths.empty_hooks,
            joined(&["local", "git-bull", "empty-hooks"])
        );
    }

    #[test]
    fn macos_uses_application_support() {
        let paths = AppPaths::resolve(System::MacOs, env(&[("HOME", "home")])).unwrap();
        let base = ["home", "Library", "Application Support", "git-bull"];
        assert_eq!(
            paths.settings,
            joined(&[&base[..], &["settings.toml"]].concat())
        );
        assert_eq!(paths.log, joined(&[&base[..], &["git-bull.log"]].concat()));
    }

    #[test]
    fn linux_follows_the_xdg_variables() {
        let paths = AppPaths::resolve(
            System::Linux,
            env(&[
                ("HOME", "home"),
                ("XDG_CONFIG_HOME", "cfg"),
                ("XDG_STATE_HOME", "state"),
            ]),
        )
        .unwrap();
        assert_eq!(
            paths.settings,
            joined(&["cfg", "git-bull", "settings.toml"])
        );
        assert_eq!(paths.log, joined(&["state", "git-bull", "git-bull.log"]));
    }

    #[test]
    fn linux_without_xdg_variables_uses_the_home_folder_defaults() {
        let paths = AppPaths::resolve(System::Linux, env(&[("HOME", "home")])).unwrap();
        assert_eq!(
            paths.settings,
            joined(&["home", ".config", "git-bull", "settings.toml"])
        );
        assert_eq!(
            paths.empty_hooks,
            joined(&["home", ".local", "state", "git-bull", "empty-hooks"])
        );
    }

    #[test]
    fn empty_xdg_variable_is_ignored_as_the_specification_requires() {
        let paths = AppPaths::resolve(
            System::Linux,
            env(&[("HOME", "home"), ("XDG_CONFIG_HOME", "")]),
        )
        .unwrap();
        assert_eq!(
            paths.settings,
            joined(&["home", ".config", "git-bull", "settings.toml"])
        );
    }

    #[test]
    fn missing_home_gives_no_paths() {
        assert_eq!(AppPaths::resolve(System::Linux, env(&[])), None);
        assert_eq!(AppPaths::resolve(System::Windows, env(&[])), None);
    }
}
