# Spec Delta

## Purpose

Settings keep the user's preferences and the state of the window across runs,
and control the theme and the language of the interface.

## ADDED Requirements

### Requirement: Persisted settings
git-bull SHALL persist the following in one file in the operating system's
configuration directory for applications: theme, language, the path to the
Git executable, recently opened repositories, open tabs and the active tab,
window size and position, divider positions and column widths.

#### Scenario: Layout survives a restart
- **WHEN** the user changes a divider position and a column width, closes git-bull and starts it again
- **THEN** the divider position and the column width are as the user left them

#### Scenario: Window geometry survives a restart
- **WHEN** the user resizes and moves the window, closes git-bull and starts it again
- **THEN** the window has the same size and position

### Requirement: Recently opened repositories
git-bull SHALL remember the 20 most recently opened repositories, most recent
first.

#### Scenario: List is capped
- **WHEN** 20 repositories are in the list and the user opens another one
- **THEN** the new repository is first in the list and the oldest entry is removed

#### Scenario: Reopening moves an entry to the top
- **WHEN** the user opens a repository that is already in the list
- **THEN** that entry moves to the first position and appears only once

### Requirement: Settings dialog
The settings dialog SHALL offer the theme, the language and the path to the
Git executable. All other settings SHALL be saved without user action.

#### Scenario: Valid Git path
- **WHEN** the user enters the path to a Git executable of a supported version
- **THEN** git-bull uses that executable from then on

#### Scenario: Invalid Git path
- **WHEN** the user enters a path that is not a Git executable of a supported version
- **THEN** git-bull shows a message explaining why and keeps the previous value

### Requirement: Invalid settings file
When the settings file cannot be read or is invalid, git-bull SHALL rename it
with the suffix `.bak`, start with default settings and report this once in
the status bar.

#### Scenario: Corrupted file
- **WHEN** git-bull starts and the settings file contains invalid content
- **THEN** the file is renamed with the suffix `.bak`
- **AND** git-bull starts with default settings
- **AND** the status bar reports that the settings were reset

### Requirement: Theme
git-bull SHALL offer a light and a dark theme. By default the theme SHALL
follow the setting of the operating system. The theme switch in the toolbar
SHALL override it. The theme SHALL apply to every area, including syntax
highlighting and the colours of the commit graph.

#### Scenario: Follow the system
- **WHEN** the theme setting is "system" and the operating system uses a dark appearance
- **THEN** git-bull uses the dark theme

#### Scenario: System appearance changes
- **WHEN** the theme setting is "system" and the operating system switches to a light appearance while git-bull is running
- **THEN** git-bull switches to the light theme

#### Scenario: Manual override
- **WHEN** the user switches the theme in the toolbar, closes git-bull and starts it again
- **THEN** git-bull uses the theme the user chose

#### Scenario: Graph colours are distinguishable
- **WHEN** either theme is active
- **THEN** neighbouring lines in the commit graph have colours that can be told apart against the background

### Requirement: Interface language
git-bull SHALL ship with English as its interface language. Every
user-visible text of the application SHALL come from translation resources,
so that a language can be added without changing program code. Output
produced by Git SHALL be shown as Git produced it.

#### Scenario: Default language
- **WHEN** the user starts git-bull for the first time
- **THEN** the interface is in English

#### Scenario: Additional language
- **WHEN** a translation resource for another language is added and that language is selected
- **THEN** every text of the interface appears in that language

#### Scenario: Git output is not translated
- **WHEN** an error message contains output from Git
- **THEN** that output appears exactly as Git produced it
