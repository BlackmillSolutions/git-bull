# Tasks

Every check uses the packages of task 1.1 and records its result, who made
it and when in the table "Verification" of `docs/release-notes/0.1.0.md`. A
step of the README that turns out wrong or missing is fixed in the README
by the task that found it, and the check is repeated.

## 1. Packages to check

- [x] 1.1 Once `dev` is merged into `master` (pull request #2), tag `v0.1.0-rc.2` on `master`; verify that the release workflow publishes the five packages as a pre-release

## 2. Installation steps of the README

- [ ] 2.1 Follow the installation steps of the README for macOS with the package for the Mac at hand (Apple silicon or Intel), including the first start without a developer signature; verify git-bull opens a repository, and record the result
- [ ] 2.2 Follow the installation steps of the README for Linux, once with the AppImage and once with the `tar.gz` archive; verify git-bull opens a repository through the folder dialog, and record the result

## 3. Start on clean installations

- [ ] 3.1 On a clean installation of Windows 10 with Git for Windows 2.34 or newer and nothing else, unpack the Windows package and start `git-bull.exe`; verify the main window appears, and record the result
- [ ] 3.2 On a clean installation of macOS 12 with Git from the Command Line Tools and nothing else, start the package for its processor as the README describes; verify the main window appears, and record the result
- [ ] 3.3 On a clean installation of Ubuntu 22.04 with Git 2.34 or newer and the software the README lists, make the AppImage executable and start it; verify the main window appears, and record the result

## 4. Final check

- [ ] 4.1 Verify that every row of the table "Verification" in `docs/release-notes/0.1.0.md` names a result, who made the check and when, and that `openspec validate verify-release-0-1-0 --strict` succeeds
