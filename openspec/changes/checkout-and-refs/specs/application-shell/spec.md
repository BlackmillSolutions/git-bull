# Spec Delta

## MODIFIED Requirements

### Requirement: Toolbar shows working actions only
The toolbar SHALL offer Open, Refresh, Branch, the search field, the theme
switch and Settings. Open SHALL show the home tab with the keyboard focus in
its filter, and the search field and Branch SHALL be shown while a repository
tab is shown. Branch SHALL open the dialog "Create branch" at the starting point
of requirement "Starting points" of `reference-creation`, and SHALL be
unavailable while a write action runs in the tab and in a repository without a
commit. It MUST NOT show actions that the application cannot perform. Open,
Refresh and Branch SHALL show an icon and their label; the theme switch and
Settings SHALL show an icon, with a tooltip that names them.

#### Scenario: Toolbar content
- **WHEN** a repository tab is shown
- **THEN** the toolbar offers Open, Refresh, Branch, the search field, the theme switch and Settings
- **AND** it shows no action for commit, pull, push or stash

#### Scenario: Icons and labels
- **WHEN** the main window is shown
- **THEN** Open, Refresh and Branch show an icon and their label, and the theme switch and Settings show an icon that names them in a tooltip

#### Scenario: Toolbar of the home tab
- **WHEN** the home tab is shown
- **THEN** the toolbar offers Open, Refresh, the theme switch and Settings, and no search field and no Branch

#### Scenario: Branch opens the dialog
- **WHEN** a repository tab is shown and the user chooses Branch
- **THEN** the dialog "Create branch" opens

#### Scenario: Branch while an action runs
- **WHEN** a checkout runs in the tab
- **THEN** Branch is unavailable

## ADDED Requirements

### Requirement: Closing while a write action runs
While a write action runs in a tab (checking out, creating a branch or a tag),
closing that tab, with its button or with Ctrl+W, and closing the window SHALL
first ask the user, and SHALL NOT stop the action without asking. The question
SHALL name the action and the tab and warn that stopping it can leave the working
copy half updated. It SHALL offer Keep open, which is the default, and Close
anyway. Close anyway SHALL close the tab or the window and stop the action. This
replaces, for write actions, the stopping of all background work of a closed tab
(requirement "Repository tabs"). Closing a tab or the window in which no write
action runs SHALL NOT ask anything.

#### Scenario: Closing a tab during a checkout
- **WHEN** a checkout runs in a tab and the user closes the tab
- **THEN** a question names the checkout and the tab and offers Keep open and Close anyway
- **AND** the tab stays open and the checkout continues until the user chooses

#### Scenario: Keeping the tab open
- **WHEN** the question is shown and the user chooses Keep open
- **THEN** the tab stays open and the checkout runs to its end

#### Scenario: Closing anyway
- **WHEN** the question is shown and the user chooses Close anyway
- **THEN** the tab closes and the checkout is stopped

#### Scenario: Closing the window during an action
- **WHEN** a checkout runs in one of three open tabs and the user closes the window
- **THEN** the same question appears and the window stays open until the user chooses

#### Scenario: Ctrl+W during an action
- **WHEN** a checkout runs in the active tab and the user presses Ctrl+W
- **THEN** the question appears and the tab stays open

#### Scenario: No action runs
- **WHEN** no write action runs and the user closes a tab
- **THEN** the tab closes without a question
