# Proposal

## Why

Each package of the first milestone must start on a clean installation of
the oldest supported system, and the installation steps of the README must
work (capability `distribution`). So far only the steps on Windows were
checked; the other checks need a Mac, a Linux machine and clean
installations that are not at hand now. As tasks 9.3 and 9.4 of
`add-repository-viewer` they kept that change open, and with it
`fix-viewer-review-findings`, whose specs build on it. In a change of their
own they no longer block archiving the two, and they still come before the
release `v0.1.0`.

## What Changes

- A pre-release built on `master` after `dev` was merged into it, so that
  the checks cover the fixes of `fix-viewer-review-findings`.
- The installation steps of the README checked on macOS and on Linux, from
  task 9.3 of `add-repository-viewer`, where Windows was checked.
- Each package started on a clean installation of Windows 10, macOS 12 and
  Ubuntu 22.04 with Git 2.34 or newer, from task 9.4 of
  `add-repository-viewer`. Ubuntu is checked by hand like the others.
- Each result recorded in `docs/release-notes/0.1.0.md`, with who made the
  check and when. `v0.1.0` is tagged only once this change is complete.

No requirement changes: the checks confirm the requirements "Supported
systems" and "Unsigned packages are documented" of `distribution` as
`add-repository-viewer` adds them.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None; the change sets `skip_specs: true`.

## Impact

- **Documentation:** `docs/release-notes/0.1.0.md`, and the README where a
  check finds a step that is wrong or missing.
- **Code:** none planned. A check that fails is fixed before `v0.1.0`, in a
  change of its own when the fix changes behaviour, and the check is then
  repeated.
- **Release:** the tag `v0.1.0` on `master` waits for this change.
- **Hardware:** a Mac, a Linux machine, and clean installations of
  Windows 10, macOS 12 and Ubuntu 22.04, for example as virtual machines.
