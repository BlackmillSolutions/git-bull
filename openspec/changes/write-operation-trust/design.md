# Design

## Context

See `proposal.md` for motivation and `specs/git-integration/spec.md` for the
contract. The exploration selected ordinary hooks and filters for explicitly
requested writes, without a repository trust dialog.

- `Git::command` in `crates/gitbull-git/src/invoke.rs` applies the viewer's
  configuration and environment rules to every command. Filter overrides
  are supplied by callers through `ConfigOverride`.
- `Git::spawn_with` builds the command and creates `Process`. All asynchronous
  readers, commit-graph generation and quarantine use this path.
- `Process` already exposes stdout and stdin pipes, drains stderr on another
  thread, offers `StderrWatcher`, retains at most 64 KiB of stderr, logs the
  outcome and supports cancellation. Its `wait` classifies error text using
  `Error::failed`, which recognises missing content for protected reads.
- `gitbull-testkit::TestRepo` and `Marker` already provide real repositories,
  linked-repository setup through Git commands, executable hooks, filters,
  failing scripts and markers. Reuse them.
- No write methods exist in `Backend`, and no repository trust state exists
  in settings. This change does not introduce either.

## Goals / Non-Goals

**Goals:** an explicit write capability, immutable per-invocation policy,
reuse of the process lifecycle and preservation of existing read behaviour.

**Non-Goals:** a new global mode, guessed permissions based on command names,
a command queue, a shell loader or a second subprocess implementation.
Action UI, caller-owned refresh and write serialisation belong to the later
checkout, staging and commit changes.

## Decisions

### 1. A writer is an explicit borrowed value

Add `crates/gitbull-git/src/write.rs` with public `WriteHooks` and
`WriteInvocation<'a>`, and export them in `lib.rs`. Add
`Git::write(&self, hooks: WriteHooks) -> WriteInvocation<'_>`. The writer
borrows the Git executable, empty hooks folder and log; it does not change
the `Git` instance or any stored repository setting.

The writer offers these interfaces; this is an API outline, not code to paste:

- `command(&self, repo: &Path, args: I) -> Result<Command, Error>`.
- `spawn(&self, repo: &Path, args: I, stdin: bool,
  watcher: Option<StderrWatcher>) -> Result<Process, Error>`.

Both use the existing generic argument bounds `I: IntoIterator<Item = S>`
and `S: AsRef<OsStr>`. Neither accepts neutralising `ConfigOverride`s.
Existing `Git` methods keep their names, signatures and read policy.

An explicit type makes the call site auditable without a mutable
`trusted` flag that could affect a simultaneous refresh. Choosing a writer
is the application caller's responsibility when implementing a user action;
it is not an authentication token or a general command sandbox. The legacy
raw read methods do not gain a subcommand whitelist. The existing explicit
commit-graph write stays on its hardened path because it needs no ordinary
hook or filter execution.

Alternative: a flag on the shared `Git`. Rejected because one tab's write
could relax another tab's read. Inferring policy from raw arguments is also
rejected: the same command family can serve browsing or a requested write.

### 2. Share construction, keep the read defaults exact

Extract a crate-private command constructor in `invoke.rs` selected by a
crate-private execution policy. `Git::command` selects Read; the writer
selects Write with its `WriteHooks` value. Preserve the existing read
argument ordering and overrides, including the tests that check them.

| Rule | Read | Write |
|---|---|---|
| Executable, requested working directory, Windows no-console flag | Existing | Same |
| No pager, no colour, raw path output, `LC_ALL=C` | Existing | Same |
| `GIT_TERMINAL_PROMPT=0`, literal pathspecs | Existing | Same |
| Clear `REDIRECTING_VARIABLES`, reset inherited command configuration count | Existing | Same |
| `core.fsmonitor=false`, `log.showSignature=false`, `diff.autoRefreshIndex=false` | Existing | Do not add |
| Empty `core.hooksPath` | Always | Only SkipCommitHooks |
| Neutralising filter overrides | Supplied by existing callers | Never supplied |
| `GIT_OPTIONAL_LOCKS=0`, `GIT_NO_LAZY_FETCH=1` | Existing | Explicitly remove these inherited variables |

Effective Git configuration, including `core.hooksPath`, filters and signing,
is otherwise honoured for writes. Application execution stays noninteractive;
required filters or signing programs that fail produce a failure. Do not
disable them or quietly retry. The ordinary network activity of a requested
write is permitted; no network action is added to the UI by this change.

Keep ownership checks intact: neither profile adds `safe.directory` or
modifies Git configuration. Argument lists continue to use `Command`, not
a generated shell command. Do not load shell startup files to discover a
new environment as part of this change.

Alternative: duplicate `Command` construction in `write.rs`. Rejected
because repository redirection, logging and Windows handling would drift.

### 3. Skip all commit hooks for exactly one invocation

`WriteHooks` has `Run` and `SkipCommitHooks`. The latter accepts only an
argument list whose first argument is the literal subcommand `commit`;
an empty list or another command returns `Error::Io` with
`io::ErrorKind::InvalidInput` before spawning Git. Define the message as
`skipping hooks is only supported for git commit`. Ordinary callers pass
the subcommand first; arbitrary global options before it are not part of
the skip interface. Amend is a commit argument and remains supported.

SkipCommitHooks points `core.hooksPath` at the same application-owned empty
folder already used for reads. This disables pre-commit, prepare-commit-msg,
commit-msg, post-commit and other hooks Git dispatches through that hook
directory during the commit. It does not neutralise filters, the file-system
monitor configuration or signing. It is not stored between invocations.

`--no-verify` is insufficient. A scratch probe with Git 2.53.0 on
2026-10-07 observed all four standard hooks for an ordinary commit,
prepare-commit-msg and post-commit for `--no-verify`, and no hooks with an
empty hooks path. This matches the scope documented for GitKraken's skip
option. The future commit change supplies the UI choice; this change adds
and tests the execution mode.

### 4. Reuse Process and classify write errors by their exit status

Extract the child-start part of `Git::spawn_with` into a crate-private
`spawn_prepared` helper: it accepts the prepared `Command`, argument vector,
stdin choice, stderr watcher and a process failure policy. It owns the
existing spawn-error mapping, timing and `Process::new` call. The current
`spawn_with` still applies its extra environment, then delegates with the
read failure policy. The writer delegates with the write failure policy.

Add a crate-private Read/Write failure policy to `Process::new` and store it
in `Process`. The single existing constructor call in `invoke.rs` is updated.
`Process::wait` continues to use `Error::failed` for reads, but creates
`Error::CommandFailed { command, code, stderr }` directly for failed writes.
A hook controls its own text; phrases such as `lazy fetching disabled`
cannot turn a failed write into the viewer's missing-content notice. No
public error variant or existing consumer match needs to change.

Reuse stdin/stdout pipes, `StderrWatcher`, `Canceller`, Drop and `CommandLog`.
The calling worker drains stdout while stderr is drained on its existing
thread. It keeps any stdout it needs before calling `wait`, including on
failure. There is no new output console or result model. A later action
must forward those streams to its UI; this change proves both are available
before completion. Do not wait before draining stdout, which could deadlock
a hook that writes enough output to fill the pipe.

Do not assume a hook's own stdout appears on Git's stdout: the Git 2.53.0
probe routed a pre-commit hook's stdout and stderr together onto Git's
stderr. Preserve Git's stream routing. Test live hook output through the
stderr watcher, and test live Git stdout separately with the existing
`cat-file --batch` pipe technique; that fixture exercises the writer's
process I/O, not a proposed user-facing action.

Writes are not retried, rolled back or automatically repeated without hooks.
Only their action owner explicitly cancels them; ordinary read-selection
cancellation must not own a write process. Later action changes must keep
the operation alive across view changes and refresh actual repository state
after success, failure or cancellation. That lifecycle integration is not
implemented by this foundation.

The probe also observed `git switch -c topic` returning 1 after a
post-checkout script returned 7, while HEAD already pointed to `topic`.
Preserve Git's actual status, not the hook script's presumed code, and do
not infer that a nonzero exit means no change. A failed post-commit hook can
likewise leave an existing commit; interpret Git's result and actual state.
The probe observed a post-commit hook returning 7 while Git returned 0 and
HEAD contained the new commit. Report the hook output without inventing a
failed operation from text when Git returned success.

### 5. Real Git tests and the existing fixtures

Add command-policy tests in `crates/gitbull-git/tests/write_command.rs` and
a real-Git integration test binary `crates/gitbull-git/tests/write.rs`.
Reuse `TestRepo` and `Marker`.
The integration binary has one top-level test with named helper scenarios,
following `tests/filters_global.rs`: set a temporary minimal global config
and `GIT_CONFIG_NOSYSTEM=1` once before any threads start. Do not add other
top-level tests or mutate the environment while a child/thread runs.

Set repository-local author identity and disable test commit signing except
in the signing scenario. Keep maintenance disabled in the minimal global
configuration, as `TestRepo` does. Hook folders and marker files are outside
staged paths. Use Git's bundled POSIX shell via the existing Marker helpers;
do not depend on a user's shell configuration or on a real GPG key or LFS
server.

Cover default/local/global/worktree hook paths, all standard commit hooks,
skipping then running hooks, invalid skip requests, transformed staged
content, a required filter failure, signing invocation, protected reads
after a write, interleaved read/write command construction, redirected
environment, output on both pipes, write failure classification, logging
and explicit cancellation. For missing content use `TestRepo::partial_clone`
and its local source; no internet service is needed.

The Git 2.53.0 probe verified `checkout HEAD -- file.txt` against a local
partial clone created with `--no-checkout`: a protected `cat-file` failed
with missing content, while ordinary checkout fetched and wrote the blob.
Use that explicit tree/path form in the test; an index-only `checkout -- .`
has no tracked entries to restore in a no-checkout clone.

Use the existing ownership fixture technique from
`tests/repository_refused.rs`, with Git's test-only ownership variable and
an empty global/system configuration. Keep that case isolated or use a
prepared write Command's per-command environment. Do not change ownership
or require administrator privileges.

## Risks / Trade-offs

- Repository programs can change files or contact the network during a
  requested write -> this is the selected ordinary-Git behaviour; browsing
  and background reads retain their safeguards.
- A post-action hook can fail after a change has happened -> return the
  actual status and output; later callers refresh state instead of retrying.
- Stdout can fill its pipe while a hook waits -> the caller drains stdout
  concurrently with the existing stderr reader, with bounded test gates.
- Reusing Process could change a reader's error mapping -> explicit failure
  policy defaults at read call sites and regression coverage of all readers.

## Migration Plan

Add the writer and shared helpers without changing any existing public read
method signature or application caller. Update the three requirement blocks
and add the write requirements through the delta spec when implementation
is completed and verified. Add ADR 0007 for the explicit-action boundary and
narrow ADR 0006's invocation-wide wording to protected reads.

No settings or data migration is needed. A rollback removes the unused
write API and retains the old read constructor. The later checkout, staging
and commit plans use this foundation after it is implemented; none is
silently folded into this change.

References: [Git hooks](https://git-scm.com/docs/githooks),
[Git filters](https://git-scm.com/docs/gitattributes), and
[GitKraken hook skipping](https://help.gitkraken.com/gitkraken-desktop/githooks/).
