---
status: proposed
date: 2026-09-29
---

# Hardened Git invocation for untrusted repositories

A repository's own configuration can name commands that Git executes, for
example a file-system monitor hook, an external diff tool or a text
conversion filter. Opening a downloaded repository in a viewer must not run
such commands. Every Git invocation therefore disables them, and git-bull
never bypasses Git's ownership check (`safe.directory`).

| Rule | Purpose |
|---|---|
| `-c core.fsmonitor=false` | Never execute a repository-configured monitor hook |
| `--no-ext-diff`, `--no-textconv` on diff commands | Never execute repository-configured diff helpers |
| `GIT_OPTIONAL_LOCKS=0` | Background reads never take repository locks |
| `GIT_TERMINAL_PROMPT=0` | No interactive prompts |
| `--no-pager`, `-c color.ui=false`, `LC_ALL=C` | Stable output, independent of user configuration and locale |
| `-c core.quotepath=false`, `-z` where available | Paths as raw bytes |

## Considered Options

- **Run Git with the user's and the repository's configuration unchanged**
  matches what the terminal does. It was rejected because opening a
  repository to look at it is a weaker act of trust than working in it.
- **Offer a "trust this repository" button** when Git refuses a repository
  was rejected. It would train users to click through a security check.

## Consequences

- Diffs never use external diff tools or text conversion, even when the user
  configured them deliberately. Binary formats that rely on text conversion
  show as binary.
- When Git refuses a repository because of its ownership, git-bull shows
  Git's message with an explanation and offers no bypass.
- All invocations go through one function that applies these rules. Calling
  Git anywhere else is a defect.
