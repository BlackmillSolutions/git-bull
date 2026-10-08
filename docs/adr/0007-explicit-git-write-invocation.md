---
status: accepted
date: 2026-10-08
---

# Explicit Git write invocation

Opening a repository for browsing applies [ADR 0006](0006-hardened-git-invocation.md).
An explicit user action that writes to it needs ordinary Git behaviour:
checkout filters, commit hooks and configured signing must run. There is no
additional repository trust dialog, persisted trust list or mutable trusted
mode on the shared Git instance.

`Git::write(WriteHooks::Run)` returns a borrowed `WriteInvocation`. The action
caller selects this policy explicitly. It is an auditable permission boundary,
not an authentication token or command sandbox. Existing raw read interfaces
keep their signatures and protections; commit-graph generation stays hardened.
This foundation adds no checkout, staging or commit Backend method or UI.

Both policies share command construction in `invoke.rs`, keeping the executable,
working directory, Windows no-console handling, literal pathspecs, pager and
colour suppression, raw paths, locale and noninteractive prompts. They clear
inherited repository redirection and reset command configuration count. Neither
adds `safe.directory` or bypasses Git's ownership check.

Writes honour effective configuration from all scopes, including includes and
worktree configuration. They do not add browsing's empty hook path, fsmonitor,
signature or index-refresh overrides or neutralising filter overrides. They
remove inherited `GIT_OPTIONAL_LOCKS` and `GIT_NO_LAZY_FETCH`, permitting ordinary
locks and lazy fetching needed by the requested write. Required filters and
signers may fail; the caller must report the failure rather than silently retry
without them. No shell startup files or shell-generated Git command are used.

The immutable per-invocation policy cannot relax a simultaneous refresh or
remember a choice for a later action. Choosing it belongs to future action
callers; browsing and background reads remain protected.

## Process lifecycle and outcomes

The writer's `spawn` delegates to the shared `spawn_prepared` and `Process`,
reusing stdin/stdout, the live stderr watcher, 64 KiB retained stderr and the
once-only command log. A worker must drain stdout before waiting. Git can route
hook stdout onto its own stderr; preserve that routing and forward watcher
output even when a post-commit hook fails but Git succeeds. Failed writes use
`CommandFailed` with Git's actual status and stderr, without interpreting
hook-controlled missing-content phrases. Reads retain their existing classifier.

Every tracked Unix subprocess has its own process group. Cancellation signals
that owned positive PGID with SIGKILL before reaping. Non-reaping `waitid`
observation retains the exited leader while pipes are drained; the shared
lifecycle lock disarms signalling before final reaping and caches the status.
Late cancellation cannot target a reused PID.

Every tracked Windows subprocess is created suspended, placed in its own job
object and then resumed, so no descendant exists before the job owns it.
Cancellation terminates the job, which includes descendants whose parent has
already exited. `taskkill /T /F` was not enough: it walks the tree from the
launcher, and on the first Windows CI run it missed a hook's background child
whose parent was gone. That child kept the stderr pipe open, to which Git
routes hook output, so waiting never returned. The job has no kill-on-close
limit, so a normal completion leaves detached Git helpers such as the
fsmonitor daemon alone. The exited leader keeps its job while pipes are
drained, as the Unix leader stays unreaped. If no job can be created or
assigned, cancellation falls back to `taskkill /T /F` before terminating the
active launcher; if the process cannot be resumed, it is killed and the spawn
fails.

Stop and Drop return without waiting
on native cleanup; stderr joining holds no lifecycle lock. Ordinary foreground
hook/filter descendants are included; deliberately escaping descendants are
outside this process-lifecycle guarantee. Native signalling errors are retained
as I/O failures rather than silently reported as successful cleanup.

Writes are not retried, rolled back or repeated without hooks. A rejecting
post-checkout hook can leave HEAD on the selected branch with a nonzero Git
status. A failing post-commit hook can accompany Git success and a new commit.
Future action callers own write lifetime across view changes and refresh actual
repository state after success, failure or cancellation; [ADR 0008](0008-write-actions-belong-to-the-session.md)
decides how. This foundation does not connect write lifetime to browsing's
selection cancellation.

## One commit without hooks

`WriteHooks::SkipCommitHooks` requires literal `commit` as the first argument;
amend is supported. Other commands, empty argument lists and leading global
options fail with InvalidInput before command execution or logging. The policy
points `core.hooksPath` at the application-owned empty folder and disables
`core.fsmonitor`, whose configured hook runs independently of that folder.

This skips pre-commit, prepare-commit-msg, commit-msg, post-commit and other
hooks Git dispatches during the commit, including reference-transaction and
post-index-change. `--no-verify` skips only pre-commit and commit-msg, leaving
prepare-commit-msg and post-commit active, so it cannot implement this choice.
Filters and signing remain active. Both hook overrides belong to the current
command only; the next ordinary write honours hooks and fsmonitor again. The
future commit change owns the UI option; no setting remembers this choice.
