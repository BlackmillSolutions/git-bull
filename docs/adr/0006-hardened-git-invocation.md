---
status: proposed
date: 2026-09-29
---

# Hardened Git invocation for untrusted repositories

Opening a downloaded repository to look at it must not run commands that the
repository brings along, and must not contact a remote. Git offers no switch
to ignore a repository's own configuration, so git-bull neutralises every
known way in which that configuration makes read-only commands execute
something.

The trust boundary is the configuration scope. Configuration from the system
and global scope belongs to the user and is honoured. Configuration from the
local and worktree scope, including files it includes, comes with the
repository and is not trusted. Hooks, attributes and the configuration of
submodules count as part of the repository.

An experiment with Git 2.55 on 2026-09-29 found these ways to execute a
command during the commands git-bull uses. Every rule in the table below
answers one of them.

| Rule | Stops |
|---|---|
| `-c core.fsmonitor=false` | The monitor hook, and the start of Git's built-in monitor daemon |
| `--no-ext-diff` and `--no-textconv` on every diff and blame command | External diff tools, diff drivers and text conversion, including smudge filters that text conversion triggers during blame |
| Neutralising repository filter drivers, see below | Clean, smudge and process filters during status, diff and blame |
| `-c log.showSignature=false` | The signature program, which otherwise runs during `git log` and `git stash list` |
| `GIT_NO_LAZY_FETCH=1` | Fetching missing objects in partial clones, which runs the transport command and contacts the remote |
| `-c core.hooksPath=<empty folder of git-bull>` | Hooks |
| `GIT_OPTIONAL_LOCKS=0` and `-c diff.autoRefreshIndex=false` | Writing the index; `git diff` writes it even with the first setting alone, and a hook then fires |
| `--ignore-submodules=dirty` on status and diff, `--submodule=short` on diff commands | Git running inside submodules, whose configuration the neutralising does not cover |
| `GIT_TERMINAL_PROMPT=0` | Interactive prompts |
| `GIT_LITERAL_PATHSPECS=1` | Paths read as patterns; otherwise the path `a[1].txt` also selects `a1.txt` |
| `--no-pager`, `--no-color`, `-c color.ui=false`, `--src-prefix=a/`, `--dst-prefix=b/`, `--untracked-files=all`, `LC_ALL=C` | Output shaped by configuration or locale, which would break parsing |
| `-c core.quotepath=false`, `-z` where available | Paths quoted or escaped instead of passed as raw bytes |

**Neutralising filter drivers.** Before the first status or diff of a
repository, git-bull reads `git config --list --show-scope --show-origin -z`.
Reading configuration executes nothing. For every filter driver with an entry
in the local or worktree scope, git-bull sets `clean`, `smudge` and `process`
to empty and `required` to false. The values are passed through
`GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_<n>` and `GIT_CONFIG_VALUE_<n>`, not
through `-c`: `-c` splits at the first `=`, so a driver named `a=b` cannot be
overridden with it. Driver names are case-sensitive and are copied exactly.
The list is read again on every refresh.

git-bull never bypasses Git's ownership check (`safe.directory`).

## Considered Options

- **Run Git with the configuration unchanged** matches what the terminal does.
  It was rejected because opening a repository to look at it is a weaker act
  of trust than working in it.
- **Ignore the repository's attributes** with `--attr-source` pointing at an
  empty tree. Rejected because `.git/info/attributes` still assigns filters;
  the experiment showed the filter running regardless.
- **Offer a "trust this repository" button** when Git refuses a repository.
  Rejected because it would train users to click through a security check.

## Consequences

- Files that a filter of the repository would normalise appear as modified in
  File status, and their diff shows the unfiltered content. Filters that the
  user installed in the global scope, such as Git LFS, keep working unless
  the repository redefines them.
- Diffs and blame never use external diff tools or text conversion, even
  when the user configured them. Formats that rely on text conversion show
  as binary.
- Changes inside the working copy of a submodule are not shown in the
  repository that contains it. The submodule can be opened in its own tab,
  where the same rules apply.
- In partial clones, content that is not present locally is not fetched.
  Diff and blame show a notice instead. git-bull makes no network connection.
- A repository can still make blame fail, for example with
  `blame.ignoreRevsFile` naming a missing file; no override was found. It
  shows as an error in the blame view.
- All invocations go through one function that applies these rules. Calling
  Git anywhere else is a defect.
- New Git versions can add new ways to execute commands. Integration tests
  set every known one to a marker command and fail when a marker is written.
  They run against the Git version installed in CI, and a new finding
  extends this record.
