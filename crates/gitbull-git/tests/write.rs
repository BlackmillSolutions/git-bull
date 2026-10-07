//! One test owns process-wide configuration before any worker starts.
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gitbull_git::filters::neutralised_filters;
use gitbull_git::locate::{Os, SystemProbe, locate_git};
use gitbull_git::log::CommandLog;
use gitbull_git::{Error, Git, WriteHooks};
use gitbull_testkit::{Marker, TestRepo};

#[test]
fn write_invocations_preserve_git_behaviour() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("gitconfig");
    fs::write(&global, "[maintenance]\n auto = false\n[gc]\n auto = 0\n").unwrap();
    // SAFETY: this integration binary has one test and no threads have started.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let executable = locate_git(None, Os::current(), &SystemProbe).unwrap();
    let empty_hooks = root.path().join("empty-hooks");
    fs::create_dir(&empty_hooks).unwrap();
    let git = Git::new(executable, empty_hooks);

    ordinary_commit_hooks(&git);
    configured_hook_paths(&git, &global);
    worktree_hook_configuration(&git);
    clean_filter_transforms_staged_content(&git);
    required_filter_failure_is_not_retried(&git);
    configured_signer_is_not_disabled(&git);
    reads_remain_protected_after_writes(&git);
    cancellation_stops_hooks_and_children(&git);
    failed_hook_preserves_output_and_status(&git);
    hook_text_does_not_become_missing_content(&git);
    post_checkout_failure_keeps_the_changed_branch(&git);
    post_commit_failure_keeps_git_success(&git);
    write_output_arrives_before_completion(&git);
}

fn repository() -> TestRepo {
    let mut repo = TestRepo::new();
    repo.config("user.name", "Writer Test");
    repo.config("user.email", "writer@example.com");
    repo.config("commit.gpgsign", "false");
    repo.config("maintenance.auto", "false");
    repo.config("gc.auto", "0");
    repo.write("file.txt", "raw\n");
    repo.commit("Initial");
    repo
}

fn write(git: &Git, path: &Path, args: &[&str]) -> Result<Vec<u8>, Error> {
    let mut process = git.write(WriteHooks::Run).spawn(path, args, false, None)?;
    let mut output = Vec::new();
    process
        .take_stdout()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    process.wait()?;
    Ok(output)
}

fn commit(git: &Git, path: &Path) {
    write(git, path, &["commit", "--allow-empty", "-m", "Write"]).unwrap();
}

fn ordinary_commit_hooks(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    for hook in [
        "pre-commit",
        "prepare-commit-msg",
        "commit-msg",
        "post-commit",
    ] {
        marker.hook(&repo.path().join(".git/hooks"), hook);
    }
    commit(git, repo.path());
    assert_eq!(
        marker.labels(),
        [
            "hook:pre-commit",
            "hook:prepare-commit-msg",
            "hook:commit-msg",
            "hook:post-commit"
        ]
    );
}

fn configured_hook_paths(git: &Git, global: &Path) {
    for scope in ["local", "global", "include"] {
        let repo = repository();
        let marker = Marker::new();
        let dir = tempfile::tempdir().unwrap();
        let hooks = dir.path().join("hooks");
        marker.hook(&hooks, "pre-commit");
        let config = format!("[core]\n hooksPath = \"{}\"\n", shell_path(&hooks));
        match scope {
            "local" => repo.config("core.hooksPath", &shell_path(&hooks)),
            "global" => fs::write(global, &config).unwrap(),
            "include" => {
                let include = dir.path().join("included.config");
                fs::write(&include, config).unwrap();
                repo.config("include.path", &shell_path(&include));
            }
            _ => unreachable!(),
        }
        commit(git, repo.path());
        assert_eq!(marker.labels(), ["hook:pre-commit"], "{scope}");
        fs::write(global, "[maintenance]\n auto = false\n[gc]\n auto = 0\n").unwrap();
    }
}

fn worktree_hook_configuration(git: &Git) {
    let repo = repository();
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("linked");
    repo.git(&["worktree", "add", "-b", "linked", &shell_path(&worktree)]);
    repo.config("extensions.worktreeConfig", "true");
    let marker = Marker::new();
    let hooks = root.path().join("hooks");
    marker.hook(&hooks, "pre-commit");
    write(
        git,
        &worktree,
        &[
            "config",
            "--worktree",
            "core.hooksPath",
            &shell_path(&hooks),
        ],
    )
    .unwrap();
    commit(git, &worktree);
    assert_eq!(marker.labels(), ["hook:pre-commit"]);
}

fn install_filter(repo: &TestRepo, marker: &Marker, body: &str) {
    let script = marker.script("clean", body);
    repo.config("filter.test.clean", &format!("'{script}'"));
    repo.config("filter.test.required", "true");
    repo.write_git_file("info/attributes", "file.txt filter=test\n");
}

fn clean_filter_transforms_staged_content(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    install_filter(&repo, &marker, "sed 's/raw/clean/g'");
    repo.write("file.txt", "raw changed\n");
    write(git, repo.path(), &["add", "file.txt"]).unwrap();
    assert_eq!(repo.git(&["show", ":file.txt"]), "clean changed\n");
    assert!(!marker.labels().is_empty());
}

fn required_filter_failure_is_not_retried(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    install_filter(&repo, &marker, "echo required-filter-rejected >&2; exit 7");
    repo.write("file.txt", "raw changed\n");
    let error = write(git, repo.path(), &["add", "file.txt"]).unwrap_err();
    assert!(
        matches!(error, Error::CommandFailed { stderr, .. } if stderr.contains("required-filter-rejected"))
    );
    assert_eq!(marker.labels(), ["clean"]);
    assert_eq!(repo.git(&["show", ":file.txt"]), "raw\n");
}

fn configured_signer_is_not_disabled(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    repo.config(
        "gpg.program",
        &marker.script("signer", "echo signer-rejected >&2; exit 1"),
    );
    repo.config("commit.gpgsign", "true");
    let head = repo.git(&["rev-parse", "HEAD"]);
    assert!(matches!(
        write(
            git,
            repo.path(),
            &["commit", "--allow-empty", "-m", "Signed"]
        ),
        Err(Error::CommandFailed { .. })
    ));
    assert_eq!(marker.labels(), ["signer"]);
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
}

fn reads_remain_protected_after_writes(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    install_filter(&repo, &marker, "cat");
    repo.write("file.txt", "raw changed\n");
    write(git, repo.path(), &["add", "file.txt"]).unwrap();
    let before = marker.labels();
    assert!(!before.is_empty());
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    repo.touch("file.txt");
    let overrides = neutralised_filters(git, repo.path()).unwrap();
    git.run(repo.path(), &overrides, ["status", "--porcelain=v2", "-z"])
        .unwrap();
    assert_eq!(marker.labels(), before);
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
}

fn shell_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Releases blocked scripts and joins collectors even during an assertion panic.
struct Cleanup {
    release: std::path::PathBuf,
    collectors: Vec<JoinHandle<()>>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, "release");
        for thread in self.collectors.drain(..) {
            let _ = thread.join();
        }
    }
}

fn wait_ready(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "fixture did not become ready: {path:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn gate(release: &Path) -> String {
    format!(
        "i=0; while test ! -f '{}' && test \"$i\" -lt 100; do sleep 0.1; i=$((i+1)); done",
        shell_path(release)
    )
}

fn cancellation_stops_hooks_and_children(git: &Git) {
    for drop_process in [false, true] {
        let repo = repository();
        let root = tempfile::tempdir().unwrap();
        let release = root.path().join("release");
        let hook_ready = root.path().join("hook-ready");
        let child_ready = root.path().join("child-ready");
        let hook_after = root.path().join("hook-after");
        let child_after = root.path().join("child-after");
        let marker = Marker::new();
        let child = marker.script(
            "nested-child",
            &format!(
                "touch '{}'; {}; touch '{}'",
                shell_path(&child_ready),
                gate(&release),
                shell_path(&child_after)
            ),
        );
        let hook = marker.script(
            "hook",
            &format!(
                "'{}' &\ntouch '{}'; {}; wait; touch '{}'",
                child,
                shell_path(&hook_ready),
                gate(&release),
                shell_path(&hook_after)
            ),
        );
        fs::copy(hook, repo.path().join(".git/hooks/pre-commit")).unwrap();
        let mut process = git
            .write(WriteHooks::Run)
            .spawn(
                repo.path(),
                ["commit", "--allow-empty", "-m", "Cancelled"],
                false,
                None,
            )
            .unwrap();
        let canceller = process.canceller();
        let mut stdout = process.take_stdout().unwrap();
        let (output_tx, output_rx) = mpsc::channel();
        let collector = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).unwrap();
            let _ = output_tx.send(bytes);
        });
        let mut cleanup = Cleanup {
            release: release.clone(),
            collectors: vec![collector],
        };
        wait_ready(&hook_ready);
        wait_ready(&child_ready);
        let (result_tx, result_rx) = mpsc::channel();
        if drop_process {
            drop(process);
        } else {
            canceller.cancel();
            cleanup.collectors.push(std::thread::spawn(move || {
                let _ = result_tx.send(process.wait());
            }));
        }
        let closed = output_rx.recv_timeout(Duration::from_secs(2)).is_ok();
        let cancelled = drop_process
            || matches!(
                result_rx.recv_timeout(Duration::from_secs(2)),
                Ok(Err(Error::Cancelled))
            );
        // Release and join before reporting failures: the emergency timeout
        // must never be mistaken for successful process-tree cancellation.
        drop(cleanup);
        assert!(
            closed,
            "descendants retained stdout after cancellation (drop={drop_process})"
        );
        assert!(cancelled, "wait did not promptly report cancellation");
        assert!(!hook_after.exists(), "hook continued after cancellation");
        assert!(
            !child_after.exists(),
            "nested child continued after cancellation"
        );
    }
}

fn scripted_hook(repo: &TestRepo, marker: &Marker, name: &str, body: &str) {
    fs::copy(
        marker.script(name, body),
        repo.path().join(".git/hooks").join(name),
    )
    .unwrap();
}

fn write_observed(git: &Git, path: &Path, args: &[&str]) -> (Result<Vec<u8>, Error>, String) {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&bytes);
    let mut process = git
        .write(WriteHooks::Run)
        .spawn(
            path,
            args,
            false,
            Some(Box::new(move |chunk| {
                observed.lock().unwrap().extend_from_slice(chunk)
            })),
        )
        .unwrap();
    let mut output = Vec::new();
    process
        .take_stdout()
        .unwrap()
        .read_to_end(&mut output)
        .unwrap();
    let result = process.wait().map(|_| output);
    let stderr = String::from_utf8_lossy(&bytes.lock().unwrap()).into_owned();
    (result, stderr)
}

fn failed_hook_preserves_output_and_status(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    scripted_hook(
        &repo,
        &marker,
        "pre-commit",
        "echo hook-stdout; echo hook-stderr >&2; exit 7",
    );
    let head = repo.git(&["rev-parse", "HEAD"]);
    let root = tempfile::tempdir().unwrap();
    let logfile = root.path().join("commands.log");
    let logged = git
        .clone()
        .with_log(Arc::new(CommandLog::new(logfile.clone(), 1024 * 1024)));
    let (result, stderr) = write_observed(
        &logged,
        repo.path(),
        &["commit", "--allow-empty", "-m", "Rejected"],
    );
    assert!(matches!(result, Err(Error::CommandFailed { code: Some(code), .. }) if code != 0));
    assert!(
        stderr.contains("hook-stdout") && stderr.contains("hook-stderr"),
        "{stderr}"
    );
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(marker.labels(), ["pre-commit"]);
    let log = fs::read_to_string(logfile).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains("exit 1  git commit"), "{log}");
    // Keep the watcher's full output while failure messages retain only 64 KiB.
    scripted_hook(
        &repo,
        &marker,
        "pre-commit",
        "i=0; while test \"$i\" -lt 9000; do echo 123456789 >&2; i=$((i+1)); done; exit 1",
    );
    let (result, observed) = write_observed(
        git,
        repo.path(),
        &["commit", "--allow-empty", "-m", "Large output"],
    );
    match result {
        Err(Error::CommandFailed { stderr, .. }) => assert_eq!(stderr.len(), 64 * 1024),
        other => panic!("{other:?}"),
    }
    assert!(observed.len() > 64 * 1024);
}

fn hook_text_does_not_become_missing_content(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    scripted_hook(
        &repo,
        &marker,
        "pre-commit",
        "echo 'lazy fetching disabled; from promisor remote' >&2; exit 1",
    );
    assert!(
        matches!(write(git, repo.path(), &["commit", "--allow-empty", "-m", "Reject"]), Err(Error::CommandFailed { stderr, .. }) if stderr.contains("lazy fetching disabled") && stderr.contains("from promisor remote"))
    );
}

fn post_checkout_failure_keeps_the_changed_branch(git: &Git) {
    let repo = repository();
    repo.git(&["branch", "topic"]);
    let marker = Marker::new();
    scripted_hook(
        &repo,
        &marker,
        "post-checkout",
        "echo post-checkout-rejected >&2; exit 7",
    );
    let (result, stderr) = write_observed(git, repo.path(), &["checkout", "topic"]);
    assert!(matches!(result, Err(Error::CommandFailed { code: Some(code), .. }) if code != 0));
    assert_eq!(
        repo.git(&["symbolic-ref", "--short", "HEAD"]).trim(),
        "topic"
    );
    assert!(stderr.contains("post-checkout-rejected"));
    assert_eq!(marker.labels(), ["post-checkout"]);
}

fn post_commit_failure_keeps_git_success(git: &Git) {
    let repo = repository();
    let head = repo.git(&["rev-parse", "HEAD"]);
    let marker = Marker::new();
    scripted_hook(
        &repo,
        &marker,
        "post-commit",
        "echo post-commit-rejected >&2; exit 7",
    );
    let (result, stderr) = write_observed(
        git,
        repo.path(),
        &["commit", "--allow-empty", "-m", "Committed"],
    );
    result.unwrap();
    assert_ne!(repo.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]).trim(), "Committed");
    assert!(stderr.contains("post-commit-rejected"));
}

fn write_output_arrives_before_completion(git: &Git) {
    let repo = repository();
    let marker = Marker::new();
    let root = tempfile::tempdir().unwrap();
    let ready = root.path().join("ready");
    let release = root.path().join("release");
    scripted_hook(
        &repo,
        &marker,
        "pre-commit",
        &format!(
            "echo early-hook-output; touch '{}'; {}",
            shell_path(&ready),
            gate(&release)
        ),
    );
    let (tx, rx) = mpsc::channel();
    let mut process = git
        .write(WriteHooks::Run)
        .spawn(
            repo.path(),
            ["commit", "--allow-empty", "-m", "Live"],
            false,
            Some(Box::new(move |chunk| {
                let _ = tx.send(chunk.to_vec());
            })),
        )
        .unwrap();
    let mut stdout = process.take_stdout().unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let collector = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        let _ = done_tx.send(process.wait());
    });
    let cleanup = Cleanup {
        release: release.clone(),
        collectors: vec![collector],
    };
    wait_ready(&ready);
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains("early-hook-output") {
        seen.extend(
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("hook output before release"),
        );
    }
    assert!(done_rx.try_recv().is_err(), "hook still waits at its gate");
    fs::write(&release, "release").unwrap();
    done_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    drop(cleanup);

    // Git stdout is also available before completion, independently of hook
    // routing. Batch input keeps Git alive after its first output record.
    let mut process = git
        .write(WriteHooks::Run)
        .spawn(repo.path(), ["cat-file", "--batch"], true, None)
        .unwrap();
    let mut stdin = process.take_stdin().unwrap();
    let stdout = process.take_stdout().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let _ = tx.send(line);
        let mut remaining = Vec::new();
        stdout.read_to_end(&mut remaining).unwrap();
    });
    writeln!(stdin, "not-a-real-object").unwrap();
    stdin.flush().unwrap();
    let early = rx.recv_timeout(Duration::from_secs(2));
    drop(stdin);
    reader.join().unwrap();
    process.wait().unwrap();
    assert_eq!(early.unwrap(), "not-a-real-object missing\n");
}
