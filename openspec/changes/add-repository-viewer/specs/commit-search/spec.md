# Spec Delta

## Purpose

Commit search finds commits by hash, message, author or file path, and keeps
the interface usable while a search runs on a very large history.

## ADDED Requirements

### Requirement: Search modes
The search SHALL offer the modes hash, message, author and file path. The
mode SHALL be chosen next to the search field. In the modes message and
author, the text SHALL be matched literally and without regard to case.

#### Scenario: Search by message
- **WHEN** the user searches for `lane` in the mode message
- **THEN** the matches are the commits whose message contains `lane` in any letter case

#### Scenario: Search by author
- **WHEN** the user searches for `novak` in the mode author
- **THEN** the matches are the commits whose author name or e-mail address contains `novak` in any letter case

#### Scenario: Search by file path
- **WHEN** the user searches for `src/graph/lanes.rs` in the mode file path
- **THEN** the matches are the commits that changed that file

#### Scenario: Special characters are literal
- **WHEN** the user searches for `a.b` in the mode message
- **THEN** a commit whose message contains `axb` is not a match

### Requirement: Search by hash
In the mode hash, git-bull SHALL select the commit with the given hash and
scroll to it. Abbreviated hashes SHALL be accepted.

#### Scenario: Abbreviated hash
- **WHEN** the user searches for an abbreviated hash that identifies one commit
- **THEN** that commit is selected and visible in the commit list

#### Scenario: Unknown hash
- **WHEN** the user searches for a hash that matches no commit
- **THEN** git-bull shows a message that no commit was found

#### Scenario: Ambiguous hash
- **WHEN** the user searches for an abbreviated hash that matches several commits
- **THEN** git-bull shows a message that the hash is ambiguous

### Requirement: Search scope
A search SHALL cover the commits selected by the branch filter.

#### Scenario: Filter is set to the current branch
- **WHEN** the branch filter is "Current branch" and the user searches by message
- **THEN** only commits reachable from HEAD are matches

### Requirement: Progressive results
Matches SHALL appear while the search is running. They SHALL be marked in the
commit graph and listed in the Search view. Next and Previous SHALL move
between matches.

#### Scenario: Matches arrive continuously
- **WHEN** a search runs on a large history
- **THEN** the first matches appear before the search has finished

#### Scenario: Matches are marked
- **WHEN** a search has matches
- **THEN** the rows of the matching commits are marked in the commit list
- **AND** the Search view lists the matching commits

#### Scenario: Move to the next match
- **WHEN** the user chooses Next
- **THEN** the next match is selected and visible in the commit list

#### Scenario: No matches
- **WHEN** a search finishes without a match
- **THEN** the Search view shows a note that nothing was found

### Requirement: Match that is not loaded
When the user moves to a match whose commit has not been loaded yet, git-bull
SHALL select it as soon as it has loaded.

#### Scenario: Match beyond the loaded history
- **WHEN** the user selects a match whose commit has not been loaded yet
- **THEN** the commit is selected and scrolled to as soon as it has loaded

### Requirement: Input handling
git-bull SHALL start a search 300 ms after the last change to the search
text. New input SHALL cancel the running search.

#### Scenario: Typing quickly
- **WHEN** the user types several characters with less than 300 ms between them
- **THEN** one search starts, for the complete text

#### Scenario: New input during a search
- **WHEN** the user changes the search text while a search is running
- **THEN** the running search stops and its matches are discarded
- **AND** a search for the new text starts

#### Scenario: Search text is cleared
- **WHEN** the user clears the search field
- **THEN** the running search stops and all marks disappear

### Requirement: Responsiveness during a search
The interface SHALL stay responsive while a search runs, however long the
search takes.

#### Scenario: Full-text search on a very large history
- **WHEN** a search by message runs on a history with more than one million commits
- **THEN** the user can scroll, select commits and view diffs while the search runs
