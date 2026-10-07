use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use gitbull_git::{ConfigOverride, Git, WriteHooks};

fn git() -> Git {
    Git::new(PathBuf::from("git"), PathBuf::from("/empty-hooks"))
}

fn arguments(command: &Command) -> Vec<OsString> {
    command.get_args().map(OsStr::to_owned).collect()
}

fn environment(command: &Command) -> Vec<(OsString, Option<OsString>)> {
    command
        .get_envs()
        .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
        .collect()
}

fn env<'a>(command: &'a Command, key: &str) -> Option<Option<&'a OsStr>> {
    command.get_envs().find(|(k, _)| *k == key).map(|(_, v)| v)
}

#[test]
fn ordinary_write_preserves_git_configuration() {
    let git = git();
    let command = git
        .write(WriteHooks::Run)
        .command(Path::new("/repo"), ["commit", "-m", "test"])
        .unwrap();
    assert_eq!(
        arguments(&command),
        [
            "--no-pager",
            "-c",
            "color.ui=false",
            "-c",
            "core.quotepath=false",
            "commit",
            "-m",
            "test"
        ]
        .map(OsString::from)
    );
    assert_eq!(command.get_current_dir(), Some(Path::new("/repo")));
    for key in ["GIT_OPTIONAL_LOCKS", "GIT_NO_LAZY_FETCH"] {
        assert_eq!(env(&command, key), Some(None));
    }
    for (key, value) in [
        ("GIT_LITERAL_PATHSPECS", "1"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("LC_ALL", "C"),
        ("GIT_CONFIG_COUNT", "0"),
    ] {
        assert_eq!(env(&command, key), Some(Some(OsStr::new(value))));
    }
}

#[test]
fn write_clears_repository_redirection() {
    let git = git();
    let command = git
        .write(WriteHooks::Run)
        .command(Path::new("/repo"), ["add", "."])
        .unwrap();
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_NAMESPACE",
        "GIT_CONFIG_PARAMETERS",
        "GIT_EXTERNAL_DIFF",
    ] {
        assert_eq!(env(&command, key), Some(None), "{key}");
    }
}

#[test]
fn constructing_a_writer_keeps_read_defaults() {
    let git = git();
    let overrides = [ConfigOverride {
        key: "filter.demo.clean".into(),
        value: String::new(),
    }];
    let before = git.command(Path::new("/repo"), &overrides, ["status"]);
    let writer = git.write(WriteHooks::Run);
    writer
        .command(Path::new("/other"), ["checkout", "topic"])
        .unwrap();
    let after = git.command(Path::new("/repo"), &overrides, ["status"]);
    assert_eq!(arguments(&before), arguments(&after));
    assert_eq!(environment(&before), environment(&after));
}
