# Spec Delta

## MODIFIED Requirements

### Requirement: Closing while a write action runs
While a write action runs in a tab (checking out, creating a branch or a tag,
staging or unstaging files, committing),
closing that tab, with its button or with Ctrl+W, and closing the window SHALL
first ask the user, and SHALL NOT stop the action without asking. The question
SHALL name the action and the tab and warn of what stopping it can leave behind:
a working copy that is half updated for a checkout, and files that were not
staged or unstaged, and a lock on the index, for staging and unstaging, and a
commit that was not made, with hooks that were stopped half way, for a commit. It SHALL offer Keep open, which is the default, and Close
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

#### Scenario: Closing a tab while files are staged
- **WHEN** staging runs in a tab and the user closes the tab
- **THEN** a question names the staging and the tab, says that files may stay as they were and that Git may leave a lock on the index, and does not mention the working copy

#### Scenario: Closing a tab while a commit runs
- **WHEN** a commit runs in a tab and the user closes the tab
- **THEN** a question names the commit and the tab, says that the commit may not be made and that its hooks are stopped, and offers Keep open and Close anyway
