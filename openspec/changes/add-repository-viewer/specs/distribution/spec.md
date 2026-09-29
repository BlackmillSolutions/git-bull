# Spec Delta

## Purpose

Distribution defines how git-bull reaches its users: which release packages
exist, under which licence, and what they contain.

## ADDED Requirements

### Requirement: Release packages
For every release, the project SHALL publish the packages below. Publishing
SHALL be triggered by a version tag.

| Platform | Package |
|---|---|
| Windows x64 | ZIP archive containing `git-bull.exe` |
| macOS arm64 | Archive containing the application bundle |
| macOS x64 | Archive containing the application bundle |
| Linux x64 | AppImage |
| Linux x64 | `tar.gz` archive |

#### Scenario: Version tag is pushed
- **WHEN** a version tag is pushed
- **THEN** a release with all five packages is published

#### Scenario: Building a package fails
- **WHEN** building one of the packages fails
- **THEN** no release is published

### Requirement: Packages run on a clean system
Every package SHALL start on a clean installation of its platform that has
Git 2.34 or newer. No further software SHALL need to be installed.

#### Scenario: Windows
- **WHEN** the user unpacks the ZIP archive on a clean Windows installation with Git and starts `git-bull.exe`
- **THEN** the main window appears

#### Scenario: macOS
- **WHEN** the user unpacks the archive on a clean macOS installation with Git and opens the application
- **THEN** the main window appears

#### Scenario: Linux
- **WHEN** the user makes the AppImage executable on a clean Linux installation with Git and starts it
- **THEN** the main window appears

### Requirement: Unsigned packages are documented
Packages SHALL be unsigned in this milestone. The README SHALL explain, for
each platform, which warning the operating system shows and how to proceed.

#### Scenario: Warning on first start
- **WHEN** the operating system warns about an unverified application on first start
- **THEN** the README contains the steps to start git-bull on that platform

### Requirement: Licence
git-bull SHALL be licensed under MIT OR Apache-2.0. The repository and every
package SHALL contain the texts of both licences.

#### Scenario: Repository
- **WHEN** a user inspects the root of the repository
- **THEN** the files `LICENSE-MIT` and `LICENSE-APACHE` are present

#### Scenario: Package
- **WHEN** a user unpacks a release package
- **THEN** it contains the texts of both licences

### Requirement: Third-party notices
Every package SHALL contain a notices file that lists every third-party
component it includes, with its licence.

#### Scenario: Notices are complete
- **WHEN** a release package is built
- **THEN** its notices file lists every dependency contained in the package together with its licence text

### Requirement: Dependency licences
The project MUST NOT include a dependency whose licence is incompatible with
distribution under MIT OR Apache-2.0. Verification SHALL fail when such a
dependency is added.

#### Scenario: Incompatible dependency is added
- **WHEN** a change adds a dependency that is available under GPL-3.0 only
- **THEN** the automated checks for that change fail and name the dependency
