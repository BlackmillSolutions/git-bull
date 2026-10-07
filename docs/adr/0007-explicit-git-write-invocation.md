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
