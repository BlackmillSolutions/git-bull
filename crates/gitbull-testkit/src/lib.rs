//! Test helpers for git-bull.
//!
//! [`TestRepo`] creates a real Git repository in a temporary folder. Commits
//! get fixed authors and dates, the user's own Git configuration is shut
//! out, and Git maintains nothing in the background, so tests behave the
//! same on every machine. [`FakeBackend`] answers
//! like Git without running it.

mod fake;
pub mod generator;

pub use fake::{FakeBackend, FakeWrite, Gate, HistoryFeed, LiveRepo, Probe, commit_line, fake_id};

use std::path::{Path, PathBuf};
use std::process::Command;

use gitbull_git::version::GitVersion;
use tempfile::TempDir;

/// The author and committer of every commit made through [`TestRepo`].
pub const AUTHOR_NAME: &str = "Ada Lovelace";
pub const AUTHOR_EMAIL: &str = "ada@example.com";

/// 2026-01-01T12:00:00Z, the date of the first commit; each further commit
/// is one minute later.
const FIRST_COMMIT_EPOCH: u64 = 1_767_268_800;

/// The version of the Git that the tests run, from the search path.
pub fn git_version() -> GitVersion {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .expect("Git runs");
    GitVersion::parse(&String::from_utf8_lossy(&output.stdout)).expect("a Git version")
}

/// A Git repository in a temporary folder, deleted when dropped.
pub struct TestRepo {
    _root: TempDir,
    work: PathBuf,
    global_config: PathBuf,
    commits: u64,
}

impl TestRepo {
    /// An empty repository with the branch `main`.
    pub fn new() -> TestRepo {
        TestRepo::init(&[])
    }

    /// A bare repository; [`TestRepo::path`] is its Git folder.
    pub fn bare() -> TestRepo {
        TestRepo::init(&["--bare"])
    }

    /// An empty repository that uses SHA-256 object names.
    pub fn sha256() -> TestRepo {
        TestRepo::init(&["--object-format=sha256"])
    }

    /// A clone of `source` that contains only its last `depth` commits.
    pub fn shallow_clone(source: &TestRepo, depth: u32) -> TestRepo {
        let clone = TestRepo::empty_folder();
        // `--depth` is ignored for plain local paths, so a file URL is used.
        let url = file_url(source.path());
        let mut command =
            clone.command(&["clone", "--quiet", "--depth", &depth.to_string(), &url, "."]);
        command.current_dir(&clone.work);
        run(command);
        clone
    }

    /// A partial clone of `source` without file contents; Git would fetch
    /// them from `source` on demand.
    pub fn partial_clone(source: &TestRepo) -> TestRepo {
        source.config("uploadpack.allowFilter", "true");
        let clone = TestRepo::empty_folder();
        let url = source.url();
        let mut command = clone.command(&[
            "clone",
            "--quiet",
            "--no-checkout",
            "--filter=blob:none",
            &url,
            ".",
        ]);
        command.current_dir(&clone.work);
        run(command);
        clone
    }

    /// A URL for this repository that Git accepts for clones and submodules.
    pub fn url(&self) -> String {
        file_url(&self.work)
    }

    /// Like [`TestRepo::git`], but returns Git's error output instead of
    /// panicking when Git fails.
    pub fn try_git(&self, args: &[&str]) -> Result<String, String> {
        let output = self.command(args).output().expect("Git runs");
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    }

    /// Runs `git <args>` with `input` on its standard input.
    pub fn git_with_input(&self, args: &[&str], input: &str) -> String {
        self.git_with_bytes(args, input.as_bytes())
    }

    /// Like [`TestRepo::git_with_input`], with input that need not be
    /// UTF-8, such as paths in other encodings.
    pub fn git_with_bytes(&self, args: &[&str], input: &[u8]) -> String {
        use std::io::Write;
        let mut child = self
            .command(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("Git runs");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input)
            .expect("input written");
        let output = child.wait_with_output().expect("Git finishes");
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).expect("UTF-8 output")
    }

    fn init(args: &[&str]) -> TestRepo {
        let repo = TestRepo::empty_folder();
        let mut init = vec!["init", "--quiet", "--initial-branch=main"];
        init.extend_from_slice(args);
        repo.git(&init);
        repo
    }

    fn empty_folder() -> TestRepo {
        let root = tempfile::tempdir().expect("temporary folder");
        let work = root.path().join("repo");
        // Outside the working tree, so that it never shows up as a change.
        // A commit would otherwise start Git's maintenance, which Git 2.47
        // runs detached and which may still take its lock in the object
        // folder while a test compares that folder.
        let global_config = root.path().join("test-gitconfig");
        std::fs::create_dir(&work).expect("working tree");
        std::fs::write(
            &global_config,
            "[maintenance]\n\tauto = false\n[gc]\n\tauto = 0\n",
        )
        .expect("Git configuration of the tests");
        TestRepo {
            _root: root,
            work,
            global_config,
            commits: 0,
        }
    }

    /// The working tree.
    pub fn path(&self) -> &Path {
        &self.work
    }

    /// Runs `git <args>` in the repository and returns its output. Panics
    /// when Git fails.
    pub fn git(&self, args: &[&str]) -> String {
        run(self.command(args))
    }

    /// Writes `contents` to `path`, relative to the working tree.
    pub fn write(&self, path: &str, contents: &str) {
        let file = self.work.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("parent folders");
        }
        std::fs::write(file, contents).expect("file written");
    }

    /// Writes `contents` to `path`, relative to the Git folder, for example
    /// `info/attributes`.
    pub fn write_git_file(&self, path: &str, contents: &str) {
        let git_dir = self.git(&["rev-parse", "--absolute-git-dir"]);
        let file = Path::new(git_dir.trim()).join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("parent folders");
        }
        std::fs::write(file, contents).expect("file written");
    }

    /// Sets `key` in the repository's own configuration.
    pub fn config(&self, key: &str, value: &str) {
        self.git(&["config", "--local", key, value]);
    }

    /// Moves the modification time of `path` forward without changing its
    /// content, so that Git has to read the file again.
    pub fn touch(&self, path: &str) {
        let file = std::fs::File::options()
            .write(true)
            .open(self.work.join(path))
            .expect("file to touch");
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        file.set_modified(later).expect("modification time");
    }

    /// Stages everything and commits it; returns the full hash.
    pub fn commit(&mut self, message: &str) -> String {
        self.git(&["add", "--all"]);
        let date = format!("{} +0000", FIRST_COMMIT_EPOCH + 60 * self.commits);
        let mut command = self.command(&["commit", "--quiet", "--message", message]);
        command
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date);
        run(command);
        self.commits += 1;
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    /// Creates `count` commits on `main` with `git fast-import`, messages
    /// "Commit 1" to "Commit <count>"; fast enough for thousands.
    pub fn import_commits(&self, count: usize) {
        let mut stream = String::new();
        for n in 1..=count {
            let message = format!("Commit {n}\n");
            stream.push_str(&format!(
                "commit refs/heads/main\nmark :{n}\ncommitter {AUTHOR_NAME} <{AUTHOR_EMAIL}> {} +0000\ndata {}\n{message}",
                FIRST_COMMIT_EPOCH + n as u64,
                message.len()
            ));
            if n > 1 {
                stream.push_str(&format!("from :{}\n", n - 1));
            }
            stream.push('\n');
        }
        self.git_with_input(&["fast-import", "--quiet"], &stream);
    }

    /// Merges `branches` into the current branch with a merge commit, an
    /// octopus merge for more than one; returns the full hash.
    pub fn merge(&mut self, message: &str, branches: &[&str]) -> String {
        let date = format!("{} +0000", FIRST_COMMIT_EPOCH + 60 * self.commits);
        let mut args = vec!["merge", "--quiet", "--no-ff", "--message", message];
        args.extend(branches);
        let mut command = self.command(&args);
        command
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date);
        run(command);
        self.commits += 1;
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command
            .args(args)
            .current_dir(&self.work)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.global_config)
            .env("GIT_AUTHOR_NAME", AUTHOR_NAME)
            .env("GIT_AUTHOR_EMAIL", AUTHOR_EMAIL)
            .env("GIT_COMMITTER_NAME", AUTHOR_NAME)
            .env("GIT_COMMITTER_EMAIL", AUTHOR_EMAIL)
            .env("LC_ALL", "C");
        for key in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"] {
            command.env_remove(key);
        }
        command
    }
}

impl Default for TestRepo {
    fn default() -> TestRepo {
        TestRepo::new()
    }
}

/// A file that shell commands configured in a test repository append to,
/// to show that Git executed them.
pub struct Marker {
    _dir: TempDir,
    path: PathBuf,
}

impl Marker {
    pub fn new() -> Marker {
        let dir = tempfile::tempdir().expect("temporary folder");
        let path = dir.path().join("marker.log");
        Marker { _dir: dir, path }
    }

    /// A shell command that appends `label` to the marker and passes its
    /// input through, as a filter must.
    pub fn filter_command(&self, label: &str) -> String {
        format!("echo {label} >> '{}'; cat", self.shell_path())
    }

    /// A shell command that only appends `label` to the marker.
    pub fn command(&self, label: &str) -> String {
        format!("echo {label} >> '{}'", self.shell_path())
    }

    /// An executable script that appends `label` to the marker and then runs
    /// `rest`, a shell snippet. Returns its path with forward slashes, as Git
    /// expects it in configuration on every platform.
    pub fn script(&self, label: &str, rest: &str) -> String {
        let path = self.path.with_file_name(format!("{label}.sh"));
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho {label} >> '{}'\n{rest}\n",
                self.shell_path()
            ),
        )
        .expect("script written");
        make_executable(&path);
        path.to_string_lossy().replace('\\', "/")
    }

    /// Writes an executable hook named `name` into `hooks_dir`.
    pub fn hook(&self, hooks_dir: &Path, name: &str) {
        std::fs::create_dir_all(hooks_dir).expect("hooks folder");
        let path = hooks_dir.join(name);
        std::fs::write(
            &path,
            format!("#!/bin/sh\necho hook:{name} >> '{}'\n", self.shell_path()),
        )
        .expect("hook written");
        make_executable(&path);
    }

    /// The labels written so far, one per execution.
    pub fn labels(&self) -> Vec<String> {
        std::fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().to_owned())
            .collect()
    }

    /// Git runs configured commands through a POSIX shell, also on Windows,
    /// where that shell expects forward slashes.
    fn shell_path(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
}

impl Default for Marker {
    fn default() -> Marker {
        Marker::new()
    }
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .expect("executable bit");
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// `file:///C:/path` on Windows, `file:///path` elsewhere.
fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

fn run(mut command: Command) -> String {
    let output = command.output().expect("Git runs");
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 output")
}
