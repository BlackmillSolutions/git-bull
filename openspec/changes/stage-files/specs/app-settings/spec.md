# Spec Delta

## MODIFIED Requirements

### Requirement: Git path while a write action runs
While a write action runs in any tab (checking out, creating a branch or a tag,
staging or unstaging files),
the settings dialog SHALL NOT apply another Git executable, because applying it
opens every tab again and would stop the action. It SHALL keep the previous value
and show a message that names the action and asks the user to wait for it. When no
write action runs, a valid path SHALL be applied as before.

#### Scenario: Another path during a checkout
- **WHEN** a checkout runs in a tab and the user enters the path of another valid Git executable in the settings dialog
- **THEN** the dialog shows a message that a checkout is running and the path is not applied
- **AND** the checkout runs on

#### Scenario: Path applied after the action
- **WHEN** the action has ended and the user applies the same path again
- **THEN** git-bull uses that executable from then on

#### Scenario: Another path while files are staged
- **WHEN** staging runs in a tab and the user enters the path of another valid Git executable in the settings dialog
- **THEN** the dialog shows a message that files are being staged and the path is not applied
