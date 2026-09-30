//! Finding the Git executable.
//!
//! Order: the path configured in the settings, then the executable search
//! path, then on Windows the default folders of Git for Windows. A configured
//! path that does not exist is an error; git-bull does not fall back to
//! another Git behind the user's back.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The operating system whose conventions apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    /// The operating system git-bull was built for.
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }
}

/// What locating Git needs to know about the system.
pub trait Probe {
    /// An environment variable.
    fn var(&self, key: &str) -> Option<OsString>;
    /// Whether `path` is an existing file.
    fn is_file(&self, path: &Path) -> bool;
    /// The active developer directory on macOS, as `xcode-select -p` reports
    /// it, or `None` when no developer tools are selected.
    fn developer_dir(&self) -> Option<PathBuf>;
}

/// The system git-bull runs on.
pub struct SystemProbe;

impl Probe for SystemProbe {
    fn var(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn developer_dir(&self) -> Option<PathBuf> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        // `xcode-select -p` only reports the selected folder; unlike the
        // `/usr/bin/git` shim it never offers to install anything.
        let output = std::process::Command::new("/usr/bin/xcode-select")
            .arg("-p")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let dir = String::from_utf8(output.stdout).ok()?;
        Some(PathBuf::from(dir.trim()))
    }
}

/// Why no Git executable could be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocateError {
    /// The path configured in the settings is not an existing file.
    ConfiguredMissing(PathBuf),
    /// No Git executable was found.
    NotFound,
}

/// Returns the Git executable to use.
pub fn locate_git(
    configured: Option<&Path>,
    os: Os,
    probe: &dyn Probe,
) -> Result<PathBuf, LocateError> {
    if let Some(path) = configured {
        return if probe.is_file(path) {
            Ok(path.to_owned())
        } else {
            Err(LocateError::ConfiguredMissing(path.to_owned()))
        };
    }

    let file_name = if os == Os::Windows { "git.exe" } else { "git" };
    let on_search_path = probe
        .var("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(file_name));

    on_search_path
        .chain(default_folders(os, probe))
        .find(|candidate| probe.is_file(candidate) && usable(candidate, os, probe))
        .ok_or(LocateError::NotFound)
}

/// Where the installers of Git for Windows put `git.exe`.
fn default_folders(os: Os, probe: &dyn Probe) -> Vec<PathBuf> {
    if os != Os::Windows {
        return Vec::new();
    }
    [
        ("ProgramFiles", r"Git\cmd\git.exe"),
        ("ProgramFiles(x86)", r"Git\cmd\git.exe"),
        ("LOCALAPPDATA", r"Programs\Git\cmd\git.exe"),
    ]
    .into_iter()
    .filter_map(|(var, rest)| probe.var(var).map(|base| PathBuf::from(base).join(rest)))
    .collect()
}

/// On macOS, `/usr/bin/git` is a shim that asks to install the developer
/// tools when they are missing. It is usable only when the selected developer
/// directory contains Git.
fn usable(candidate: &Path, os: Os, probe: &dyn Probe) -> bool {
    if os != Os::MacOs || candidate != Path::new("/usr/bin/git") {
        return true;
    }
    probe
        .developer_dir()
        .is_some_and(|dir| probe.is_file(&dir.join("usr/bin/git")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct FakeSystem {
        vars: HashMap<String, OsString>,
        files: HashSet<PathBuf>,
        developer_dir: Option<PathBuf>,
    }

    impl FakeSystem {
        fn var(mut self, key: &str, value: &str) -> Self {
            self.vars.insert(key.to_owned(), value.into());
            self
        }
        fn file(mut self, path: &str) -> Self {
            self.files.insert(PathBuf::from(path));
            self
        }
        #[cfg(unix)]
        fn developer_dir(mut self, path: &str) -> Self {
            self.developer_dir = Some(PathBuf::from(path));
            self
        }
    }

    impl Probe for FakeSystem {
        fn var(&self, key: &str) -> Option<OsString> {
            self.vars.get(key).cloned()
        }
        fn is_file(&self, path: &Path) -> bool {
            self.files.contains(path)
        }
        fn developer_dir(&self) -> Option<PathBuf> {
            self.developer_dir.clone()
        }
    }

    #[test]
    fn finds_the_git_installed_on_this_system() {
        let git = locate_git(None, Os::current(), &SystemProbe).expect("Git is installed");
        let output = std::process::Command::new(&git)
            .arg("--version")
            .output()
            .expect("the located Git runs");
        assert!(String::from_utf8_lossy(&output.stdout).starts_with("git version "));
    }

    #[cfg(windows)]
    mod windows {
        use super::*;

        fn locate(configured: Option<&str>, system: &FakeSystem) -> Result<PathBuf, LocateError> {
            locate_git(configured.map(Path::new), Os::Windows, system)
        }

        #[test]
        fn configured_path_wins_over_search_path() {
            let system = FakeSystem::default()
                .var("PATH", r"C:\Git\cmd")
                .file(r"C:\Git\cmd\git.exe")
                .file(r"D:\tools\git.exe");
            assert_eq!(
                locate(Some(r"D:\tools\git.exe"), &system),
                Ok(PathBuf::from(r"D:\tools\git.exe"))
            );
        }

        #[test]
        fn missing_configured_path_is_an_error_even_with_git_on_the_search_path() {
            let system = FakeSystem::default()
                .var("PATH", r"C:\Git\cmd")
                .file(r"C:\Git\cmd\git.exe");
            assert_eq!(
                locate(Some(r"D:\gone\git.exe"), &system),
                Err(LocateError::ConfiguredMissing(PathBuf::from(
                    r"D:\gone\git.exe"
                )))
            );
        }

        #[test]
        fn first_search_path_entry_with_git_wins() {
            let system = FakeSystem::default()
                .var("PATH", r"C:\a;C:\b;C:\c")
                .file(r"C:\b\git.exe")
                .file(r"C:\c\git.exe");
            assert_eq!(locate(None, &system), Ok(PathBuf::from(r"C:\b\git.exe")));
        }

        #[test]
        fn quoted_search_path_entry_is_searched() {
            let system = FakeSystem::default()
                .var("PATH", r#""C:\Program Files\Git\cmd""#)
                .file(r"C:\Program Files\Git\cmd\git.exe");
            assert_eq!(
                locate(None, &system),
                Ok(PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"))
            );
        }

        #[test]
        fn file_without_exe_extension_is_not_used() {
            let system = FakeSystem::default()
                .var("PATH", r"C:\cygwin\bin")
                .file(r"C:\cygwin\bin\git");
            assert_eq!(locate(None, &system), Err(LocateError::NotFound));
        }

        #[test]
        fn default_folder_is_used_when_search_path_has_no_git() {
            let system = FakeSystem::default()
                .var("PATH", r"C:\Windows\System32")
                .var("ProgramFiles", r"C:\Program Files")
                .file(r"C:\Program Files\Git\cmd\git.exe");
            assert_eq!(
                locate(None, &system),
                Ok(PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"))
            );
        }

        #[test]
        fn default_folder_for_32_bit_programs_is_used() {
            let system = FakeSystem::default()
                .var("ProgramFiles(x86)", r"C:\Program Files (x86)")
                .file(r"C:\Program Files (x86)\Git\cmd\git.exe");
            assert_eq!(
                locate(None, &system),
                Ok(PathBuf::from(r"C:\Program Files (x86)\Git\cmd\git.exe"))
            );
        }

        #[test]
        fn default_folder_of_a_per_user_installation_is_used() {
            let system = FakeSystem::default()
                .var("LOCALAPPDATA", r"C:\Users\ada\AppData\Local")
                .file(r"C:\Users\ada\AppData\Local\Programs\Git\cmd\git.exe");
            assert_eq!(
                locate(None, &system),
                Ok(PathBuf::from(
                    r"C:\Users\ada\AppData\Local\Programs\Git\cmd\git.exe"
                ))
            );
        }

        #[test]
        fn search_path_wins_over_default_folder() {
            let system = FakeSystem::default()
                .var("PATH", r"D:\portable-git\cmd")
                .var("ProgramFiles", r"C:\Program Files")
                .file(r"D:\portable-git\cmd\git.exe")
                .file(r"C:\Program Files\Git\cmd\git.exe");
            assert_eq!(
                locate(None, &system),
                Ok(PathBuf::from(r"D:\portable-git\cmd\git.exe"))
            );
        }

        #[test]
        fn nothing_found_is_not_found() {
            let system = FakeSystem::default()
                .var("PATH", r"C:\Windows\System32")
                .var("ProgramFiles", r"C:\Program Files");
            assert_eq!(locate(None, &system), Err(LocateError::NotFound));
        }
    }

    #[cfg(unix)]
    mod unix {
        use super::*;

        #[test]
        fn first_search_path_entry_with_git_wins() {
            let system = FakeSystem::default()
                .var("PATH", "/a:/b:/c")
                .file("/b/git")
                .file("/c/git");
            assert_eq!(
                locate_git(None, Os::Linux, &system),
                Ok(PathBuf::from("/b/git"))
            );
        }

        #[test]
        fn windows_default_folders_are_not_searched_on_linux() {
            let system = FakeSystem::default()
                .var("PATH", "/usr/bin")
                .var("ProgramFiles", "/pf")
                .file("/pf/Git/cmd/git.exe");
            assert_eq!(
                locate_git(None, Os::Linux, &system),
                Err(LocateError::NotFound)
            );
        }

        #[test]
        fn system_git_on_linux_needs_no_developer_tools() {
            let system = FakeSystem::default()
                .var("PATH", "/usr/bin")
                .file("/usr/bin/git");
            assert_eq!(
                locate_git(None, Os::Linux, &system),
                Ok(PathBuf::from("/usr/bin/git"))
            );
        }

        #[test]
        fn macos_shim_without_developer_tools_is_treated_as_missing() {
            let system = FakeSystem::default()
                .var("PATH", "/usr/bin:/bin")
                .file("/usr/bin/git");
            assert_eq!(
                locate_git(None, Os::MacOs, &system),
                Err(LocateError::NotFound)
            );
        }

        #[test]
        fn macos_shim_pointing_to_a_developer_dir_without_git_is_treated_as_missing() {
            let system = FakeSystem::default()
                .var("PATH", "/usr/bin")
                .file("/usr/bin/git")
                .developer_dir("/Library/Developer/CommandLineTools");
            assert_eq!(
                locate_git(None, Os::MacOs, &system),
                Err(LocateError::NotFound)
            );
        }

        #[test]
        fn macos_shim_with_developer_tools_is_used() {
            let system = FakeSystem::default()
                .var("PATH", "/usr/bin")
                .file("/usr/bin/git")
                .file("/Library/Developer/CommandLineTools/usr/bin/git")
                .developer_dir("/Library/Developer/CommandLineTools");
            assert_eq!(
                locate_git(None, Os::MacOs, &system),
                Ok(PathBuf::from("/usr/bin/git"))
            );
        }

        #[test]
        fn macos_skips_the_shim_and_uses_a_later_git() {
            let system = FakeSystem::default()
                .var("PATH", "/usr/bin:/opt/homebrew/bin")
                .file("/usr/bin/git")
                .file("/opt/homebrew/bin/git");
            assert_eq!(
                locate_git(None, Os::MacOs, &system),
                Ok(PathBuf::from("/opt/homebrew/bin/git"))
            );
        }
    }
}
