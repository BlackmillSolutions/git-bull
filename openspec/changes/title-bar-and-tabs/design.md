# Design

## Context

See `proposal.md` for why. `ui::show` in `crates/gitbull-app/src/ui.rs`
draws two top panels below the system's title bar: `tab_bar`, with one
`tab_button` per tab of the workspace and the button for a new tab, and
`toolbar`. `native::viewport` builds the window from the settings: title,
size, position, minimum size, drag and drop. The workspace in
`crates/gitbull-core/src/workspace.rs` keeps the tabs in a `Vec` in the
order they were opened; `App` writes their roots in that order into
`Settings::tabs` whenever it changes (`app.rs`), so any new order is saved
and restored without further work. Actions of the window go through
`ui::Action` and `ui::apply`, shortcuts through `ui::shortcuts`.

These facts were checked for egui 0.36.2, eframe 0.36.2 and winit 0.30.13
while writing this design:

- `ViewportBuilder::with_decorations(false)` removes the system's title bar
  and frame. egui-winit then asks Windows for the drop shadow of a window
  without a frame by itself.
- winit on Windows makes the whole of such a window its client area
  (`WM_NCCALCSIZE`), so the system offers no border to resize it. winit
  never answers `WM_NCHITTEST` with `HTMAXBUTTON`, so Windows 11 cannot show
  its snap layouts on a maximize button that git-bull draws.
- `ViewportCommand::StartDrag` hands the move to the system: on Windows its
  own move loop, which still snaps the window to the edges of the screen;
  on X11 and Wayland the window manager's move. `BeginResize(direction)`
  does the same for resizing. `Minimized`, `Maximized` and `Close` do what
  their names say, and `ViewportInfo::maximized` reports the state.
  `Close` takes the same path as the system's close button, so
  `NativeApp::on_exit` saves the settings as before.
- On macOS, `with_fullsize_content_view(true)`, `with_titlebar_shown(false)`
  and `with_title_shown(false)` make the title bar transparent, keep the
  system's buttons at its left and let egui draw beneath them. egui-winit
  applies these only when it creates the window; no command changes them
  later.
- On Wayland, winit draws no decorations of its own when they are off, and
  asks the compositor for none.
- egui_kittest's `Harness::output` returns the `FullOutput` of the last
  frame, whose output for the root viewport lists the commands sent, and a
  test can set `maximized` in the `ViewportInfo` of its raw input.
- `Settings` reads with `#[serde(default)]` and without
  `deny_unknown_fields`: a file without the new setting reads with its
  default, and an older git-bull ignores the new setting in a newer file.
- Phosphor has the icons the window buttons need: `MINUS`, `SQUARE`, `COPY`
  for a maximized window, and `X`.

## Goals / Non-Goals

**Goals:**

- One function draws the row of tabs for both the own title bar and the
  tab bar below the system's, so that tabs behave the same in either.
- Window behaviour that the system provides stays with the system: moving,
  resizing, snapping and maximizing go through viewport commands rather
  than through git-bull moving the window itself.

**Non-Goals:**

- Native hit testing on Windows to bring back snap layouts: it needs code
  against the Windows API next to winit's own window procedure.
- Animating tabs while they move into place, beyond following the pointer.
- A title or an application icon in the title bar: the active tab names
  the repository, and the system shows the icon in the taskbar or dock.

## Decisions

### 1. The window per platform

`native::viewport` takes the setting into account. Without the setting, on
Windows and Linux it turns decorations off; on macOS it keeps them and
turns on the full-size content view with a transparent title bar and no
title. With the setting, the window is built as today on every platform.

`App` keeps the value of the setting with which the window was built, and
the interface draws from that value, not from the setting: the window and
the interface then always agree, and a change in the dialog takes effect at
the next start, as the specs require. Changing the window at once would
work on Windows and Linux with `ViewportCommand::Decorations`, but not on
macOS (see Context), and the window's outer size would jump by the height
of the title bar.

Alternatives considered:

- Own buttons on macOS as well: the user chose to keep the system's.
- The system's title bar everywhere with tabs below, as today: the reason
  for the change.

### 2. One title bar panel

The panel `tab_bar` becomes `title_bar`. It has three parts, from the left:

1. On macOS, an empty inset for the system's buttons.
2. The row of tabs with the button for a new tab (decision 4).
3. Free space for moving the window, and on Windows and Linux the window
   buttons at the right.

The free space is one response with `Sense::click_and_drag` behind the tabs
and buttons: a drag that starts on it sends `StartDrag`, and a double click
sends `Maximized` with the opposite of `ViewportInfo::maximized`. This is
the approach of egui's example `custom_window_frame`. At least 48 points of
it stay free whatever the number of tabs, so that the window can always be
moved.

The panel is as high as a tab plus the padding of the design system, and on
macOS at least as high as the system's title bar, 28 points, so that the
system's buttons sit inside it. The inset on macOS is a fixed 72 points,
the width of the three buttons with their margins; macOS does not scale
them with the interface size, so the inset is divided by egui's zoom factor.

With the system's title bar, the same function draws only part 2, as the
tab bar is drawn today.

### 3. Window buttons and resizing

The window buttons are Minimize (`MINUS`), Maximize (`SQUARE`) or Restore
(`COPY`) by `ViewportInfo::maximized`, and Close window (`X`). They are as
high as the title bar and 46 points wide, the proportions of Windows; Close
window shows a red surface under the pointer, as Windows and many Linux
themes do. Each has a tooltip and a name for assistive technology, as the
requirement "Controls" of `visual-design` asks of icon buttons, through a
variant of `components::icon_button` with the larger size.

On Windows and Linux, while the window is not maximized, a band of 4 points
along each edge, and 12 points along each side of a corner, resizes the
window: the pointer shows the matching resize cursor there, and a press
sends `BeginResize` with that direction. The bands are one interactive area
in egui's foreground order, so that a press on them reaches neither the
panels nor the scrollbars beneath. 4 points keep most of a scrollbar at the
edge of the window usable.

Alternatives considered:

- Resizing through the system with native hit testing on Windows: see
  Non-Goals; it would also leave X11 and Wayland to do separately.
- No resizing at the edges, only by maximizing: the window could no longer
  be resized by hand.

### 4. The row of tabs

Each tab is as wide as its title needs, at most 240 points. When the tabs
do not fit the space left for them, all become narrower alike down to 96
points, and their titles end in "…" with the full title as tooltip. Beyond
that, the row sits in a horizontal `ScrollArea` without a visible
scrollbar: the wheel over it scrolls it sideways, and a tab that becomes
active scrolls into view. The button for a new tab follows the last tab.

### 5. Reordering tabs

A tab senses clicks and drags. A click activates it as today. A drag
activates it as well; while it lasts, the tab follows the pointer
horizontally within the row, and the others make room as its centre passes
theirs. When the drag ends, `Action::MoveTab(id, index)` moves the tab
there with `Workspace::move_tab`. Escape ends a drag without moving the
tab; egui aborts the drag itself. A tab dragged to the edge of a row that
scrolls scrolls it.

Ctrl+Shift+Page Up and Ctrl+Shift+Page Down send `Action::MoveActive(-1)`
and `(1)`, which move the active tab one place with
`Workspace::move_active`; at either end nothing changes. They use Ctrl on
every platform, like switching tabs, so `ui::shortcuts` matches them with
`Modifiers::CTRL | Modifiers::SHIFT` as it matches Ctrl+Shift+Tab. The
lists take Page Up and Page Down only without modifiers, so they do not
take these keys.

`Workspace::move_tab` removes the tab and inserts it at the index, which is
clamped to the tabs; the active tab and the state of every tab stay. A tab
still opening moves like any other.

Alternatives considered:

- egui's `dnd_drag_source` and `dnd_drop_zone`: they move a payload
  between drop zones and draw no room for the tab while it moves.

### 6. The setting

`Settings` gets `system_title_bar: bool`, read like the other settings
with `or_default`, so that a wrong value falls back to `false` without
invalidating the file. The settings dialog shows it in "Appearance" as a
checkbox "Use the system title bar". While its value differs from the one
the window was built with, the dialog shows below it that the change takes
effect when git-bull starts next.

`components` gets a `checkbox` with the hover state, focus ring and click
target the requirement "Controls" asks for, as the design system did for
the other controls.

### 7. Texts

The names of the window buttons, the setting and its note come from
`crates/gitbull-app/i18n/en-US.ftl`, as all texts do.

### 8. Tests

- Unit tests of `Workspace::move_tab` and `move_active`, of
  `native::viewport` for each platform with and without the setting, and
  of reading a settings file without the setting.
- UI tests of the window with egui_kittest, which read the commands sent
  from `Harness::output`: a drag on the free space sends `StartDrag`, a
  double click `Maximized(true)`, Minimize, Maximize, Restore and Close
  window their commands, and a press on the right edge `BeginResize` to the
  east; a maximized window names its button Restore and offers no band to
  resize. Further UI tests drag a tab past another, move the active tab
  with the keyboard, open 30 tabs in the smallest window, and check the
  title bar on macOS, with the system's title bar, and in the settings
  dialog.
- The snapshots of the window change with the title bar; a new snapshot
  shows the title bar as on macOS.
- A manual check on Windows by the user: moving, snapping to the edges of
  the screen and with Win+arrow keys, double clicking, the window buttons,
  resizing at every edge and corner, Alt+Space, dragging tabs, many tabs,
  and the setting.

## Risks / Trade-offs

- [No snap layouts on the maximize button on Windows 11] → Dragging the
  window to the edges of the screen, Win+arrow keys and Win+Z still snap
  it, and the setting brings back the system's title bar with its button.
- [The band for resizing covers 4 points of a scrollbar at the edge of the
  window] → The rest of the scrollbar stays usable, and dragging its thumb
  is not affected once it started.
- [egui misses the release of the button after the system's move or resize
  loop] → The manual check clicks right after moving and resizing; if a
  platform shows it, the title bar ends the drag on the next pointer event.
- [macOS and Linux cannot be checked by hand here] → The UI tests run on
  all three platforms in CI; the setting brings back the system's title bar
  on each; a manual check follows when a Mac or a Linux desktop is at hand.
- [Compositors that insist on drawing their own decorations] → git-bull
  then shows two title bars, and the setting removes its own.
- [The system's buttons on macOS sit higher than the centre of the tabs]
  → The title bar is at least as high as the system's, so they stay inside
  it; their exact position is fixed by macOS.

## Migration Plan

Nothing to migrate: the new setting defaults to the own title bar, and an
older git-bull ignores it. Reverting the change brings back the system's
title bar and the tab bar; the order of the tabs stays as the user left
it.
