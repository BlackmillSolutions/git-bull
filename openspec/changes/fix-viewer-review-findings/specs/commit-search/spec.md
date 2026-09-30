# Spec Delta

## MODIFIED Requirements

### Requirement: Search by hash
In the mode hash, git-bull SHALL select the commit with the given hash and
scroll to it. Abbreviated hashes of at least four characters SHALL be
accepted. The search SHALL finish however many objects of the repository
share the abbreviated hash.

#### Scenario: Abbreviated hash
- **WHEN** the user searches for an abbreviated hash that identifies one commit
- **THEN** that commit is selected and visible in the commit list

#### Scenario: Unknown hash
- **WHEN** the user searches for a hash that matches no commit
- **THEN** git-bull shows a message that no commit was found

#### Scenario: Ambiguous hash
- **WHEN** the user searches for an abbreviated hash that matches several commits
- **THEN** git-bull shows a message that the hash is ambiguous

#### Scenario: Prefix shared by many objects
- **WHEN** the user searches for four characters that begin the names of more than 2,000 objects of the repository
- **THEN** the search finishes, with the message that the hash is ambiguous or, when only one of those objects is a commit, with that commit selected

#### Scenario: Commit is hidden by the branch filter
- **WHEN** the user searches for the hash of a commit that is reachable from a branch but not part of the filtered graph
- **THEN** git-bull shows a notice that the commit is hidden by the branch filter and offers to show all branches

#### Scenario: Commit is not part of any branch
- **WHEN** the user searches for the hash of a commit that no branch, tag or remote branch leads to
- **THEN** git-bull shows a message that the commit exists but is not part of the displayed history
