# Proposal

## Why

The top of the window spends three rows before the history begins: the
system's title bar, which shows only "git-bull", the tab bar and the
toolbar. Tools of the same kind put the tabs into the title bar and give
that row back to the history. The tabs also keep the order in which the
repositories were opened: a user who works in a few repositories cannot put
them where they want them. Both were left out of `ui-design-system` as a
later change of milestone M2.

## What Changes

- On Windows and Linux, git-bull draws its own title bar instead of the
  system's: the tabs, the button for a new tab, and the buttons to
  minimize, maximize or restore, and close the window. The free space of
  the title bar moves the window when dragged and maximizes or restores it
  on a double click, and the edges of the window resize it; all of this
  also works while the settings dialog is open.
- On macOS, the system keeps its buttons to close, minimize and zoom the
  window, and the tabs move into the title bar beside them.
- The toolbar stays a row of its own below the title bar, unchanged.
- Tabs can be reordered by dragging them with the mouse, and the active
  tab moves one place to the left or right with Ctrl+Shift+Page Up and
  Ctrl+Shift+Page Down. The order is remembered like the tabs themselves.
- When there are more tabs than fit, they get narrower down to a minimum
  width, then the row of tabs scrolls; the window buttons and some free
  space to drag the window always stay.
- A new setting "Use the system title bar" in the settings dialog brings
  back the system's title bar with the tab bar as a row of its own, for
  example for tiling window managers on Linux. It takes effect at the next
  start.

Out of scope: dragging a tab out of the window into a window of its own,
the system menu of the title bar (Windows still opens it with Alt+Space),
the layouts that Windows 11 shows on the maximize button, the other
changes of milestone M2 (graph, command palette, repository manager,
comforts of the diff), and moving the toolbar into the title bar.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `application-shell`: the requirement "Main window areas" names the title
  bar with the tabs instead of a tab bar; "Repository tabs" adds the order
  of the tabs and reordering them; "Keyboard operation" adds the shortcuts
  that move a tab; a new requirement "Title bar" describes the title bar on
  each platform.
- `app-settings`: "Persisted settings" and "Settings dialog" add the
  setting for the system title bar.

## Impact

- `crates/gitbull-app/src/native.rs`: the window without the system's title
  bar on Windows and Linux, and with a transparent title bar on macOS,
  unless the setting asks for the system's.
- `crates/gitbull-app/src/ui.rs`: the title bar in place of the tab bar,
  with dragging and resizing the window, the window buttons, dragging tabs
  and the new shortcuts; the setting in the settings dialog.
- `crates/gitbull-core/src/workspace.rs`: moving a tab to another place.
- `crates/gitbull-core/src/settings.rs`: the setting for the system title
  bar; older settings files read without it.
- `crates/gitbull-app/src/icons.rs` and `crates/gitbull-app/i18n/en-US.ftl`:
  icons and names of the window buttons and of the new setting.
- Tests of the window, the tabs, the shortcuts and the settings, and the
  snapshots of the window, which change with the title bar.
- No new dependency: eframe offers what the title bar needs.
