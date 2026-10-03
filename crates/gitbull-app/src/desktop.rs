//! What git-bull asks of the desktop around its window: the time, and
//! showing a folder in the file manager (design of `repository-home`,
//! decision 8). Tests replace it to fix the time and record what was shown.

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::paths::System;

pub trait Desktop {
    /// The time now, in seconds since 1970.
    fn now(&self) -> i64;
    /// Shows `folder` in the file manager of the system, without opening
    /// anything inside it.
    fn reveal(&self, folder: &Path) -> io::Result<()>;
}

/// The desktop git-bull runs on.
pub struct SystemDesktop;

impl Desktop for SystemDesktop {
    fn now(&self) -> i64 {
        jiff::Timestamp::now().as_second()
    }

    fn reveal(&self, folder: &Path) -> io::Result<()> {
        spawn(reveal_command(System::current(), folder)?)
    }
}

/// A program to start and its arguments.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: &'static str,
    pub args: Vec<OsString>,
    /// The arguments go to the program as written, without the quotes and
    /// escapes the standard library adds on Windows.
    pub raw: bool,
}

/// The command that shows `folder` in the file manager of `system` from
/// its parent folder: Explorer selects it, which keeps it from entering a
/// folder whose name makes it a shell namespace, and takes the argument as
/// written, as it splits arguments at commas; the Finder reveals an
/// application bundle instead of starting it; Linux has no common way to
/// reveal, so `xdg-open` gets the folder, and nothing but a folder.
pub fn reveal_command(system: System, folder: &Path) -> io::Result<Launch> {
    match system {
        System::Windows => Ok(Launch {
            program: "explorer.exe",
            args: vec![format!("/select,\"{}\"", folder.display()).into()],
            raw: true,
        }),
        System::MacOs => Ok(Launch {
            program: "open",
            args: vec!["-R".into(), folder.as_os_str().to_owned()],
            raw: false,
        }),
        System::Linux => {
            if !folder.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} is not a folder", folder.display()),
                ));
            }
            Ok(Launch {
                program: "xdg-open",
                args: vec![folder.as_os_str().to_owned()],
                raw: false,
            })
        }
    }
}

/// Starts `launch` without a shell and without waiting for it; a thread
/// collects its exit, so that it leaves no zombie behind.
fn spawn(launch: Launch) -> io::Result<()> {
    let mut command = Command::new(launch.program);
    add_args(&mut command, &launch);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(windows)]
fn add_args(command: &mut Command, launch: &Launch) {
    use std::os::windows::process::CommandExt;
    for arg in &launch.args {
        match launch.raw {
            true => command.raw_arg(arg),
            false => command.arg(arg),
        };
    }
}

#[cfg(not(windows))]
fn add_args(command: &mut Command, launch: &Launch) {
    command.args(&launch.args);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn explorer_selects_a_folder_whose_name_has_a_comma_as_written() {
        let folder = PathBuf::from(r"C:\work\fix,reload");
        let launch = reveal_command(System::Windows, &folder).unwrap();
        assert_eq!(
            launch,
            Launch {
                program: "explorer.exe",
                args: vec![r#"/select,"C:\work\fix,reload""#.into()],
                raw: true,
            }
        );
    }

    #[test]
    fn explorer_selects_a_folder_named_like_a_shell_namespace() {
        let folder = PathBuf::from(r"C:\work\x.{645FF040-5081-101B-9F08-00AA002F954E}");
        let launch = reveal_command(System::Windows, &folder).unwrap();
        assert_eq!(
            launch.args,
            [OsString::from(
                r#"/select,"C:\work\x.{645FF040-5081-101B-9F08-00AA002F954E}""#
            )]
        );
    }

    #[test]
    fn the_finder_reveals_a_folder_named_like_an_application() {
        let folder = PathBuf::from("/work/Tool.app");
        let launch = reveal_command(System::MacOs, &folder).unwrap();
        assert_eq!(
            launch,
            Launch {
                program: "open",
                args: vec!["-R".into(), "/work/Tool.app".into()],
                raw: false,
            }
        );
    }

    #[test]
    fn the_finder_takes_a_comma_as_part_of_the_path() {
        let folder = PathBuf::from("/work/fix,reload");
        let launch = reveal_command(System::MacOs, &folder).unwrap();
        assert_eq!(launch.args, ["-R", "/work/fix,reload"]);
    }

    #[test]
    fn linux_opens_a_folder_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Tool.app");
        std::fs::create_dir(&folder).unwrap();
        let launch = reveal_command(System::Linux, &folder).unwrap();
        assert_eq!(launch.program, "xdg-open");
        assert_eq!(launch.args, [folder.as_os_str()]);

        let file = dir.path().join("run.sh");
        std::fs::write(&file, "").unwrap();
        assert!(reveal_command(System::Linux, &file).is_err());
        assert!(reveal_command(System::Linux, &dir.path().join("gone")).is_err());
    }

    #[test]
    fn linux_takes_a_comma_as_part_of_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("fix,reload");
        std::fs::create_dir(&folder).unwrap();
        let launch = reveal_command(System::Linux, &folder).unwrap();
        assert_eq!(launch.args, [folder.as_os_str()]);
    }
}
