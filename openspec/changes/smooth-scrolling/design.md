# Design

## Context

See `proposal.md` for why. The long lists of git-bull are one widget,
`VirtualList` in `crates/gitbull-app/src/virtual_list.rs` (ADR 0005). Its
`ListState` keeps the scroll position as a row index plus an offset in
64-bit floats. While the pointer is over a list, `VirtualList::show` takes
egui's `smooth_scroll_delta.y` and moves the list by it at once with
`scroll_by`. Keys go through `apply`, which calls `reveal`; the scrollbar
and `reveal` move the list with `scroll_to`. `reveal` scrolls only when the
row is not wholly in view.

These facts were checked for egui 0.36.2, eframe 0.36.2 and winit 0.30.13
while writing this design:

- egui turns each `Event::MouseWheel` into points: `Point` as it is, `Line`
  times `InputOptions::line_scroll_speed` (40 on native platforms), `Page`
  times the height of the viewport. With the zoom modifier (Ctrl, or Cmd on
  macOS) the input zooms instead of scrolling; with the horizontal modifier
  (Shift) it scrolls horizontally. Both modifiers are fields of
  `InputOptions`.
- egui passes input on at once when it comes inside a touchpad gesture,
  between the phases `Start` and `End`, or as `Point` steps shorter than
  8 points. Everything else it smooths, 90 % within 0.1 s, each burst on
  its own. The smoothing has no option, and `WheelState` is private.
- egui passes its smoothed copy of the input on in `smooth_scroll_delta`
  over the passes after the events, for about 0.3 s after a burst of
  681 points, and whichever scroll area is under the pointer then takes it.
  The end of a gesture clears what is left.
- winit on Windows reports both the mouse wheel and the touchpad as `Line`
  with the phase `Move` only; the touchpad sends fractional lines. All 815
  wheel events the probe recorded were of this kind. The two cannot be told
  apart. On X11, winit reports both as `Line` with `Move` as well.
- winit on macOS reports a touchpad as `Point` with the phases `Start`,
  `Move` and `End`, and the kinetic movement after it as a gesture of its
  own. On Wayland it reports a touchpad as `Point` from `Start` to `End`,
  and a mouse wheel as `Line` with `Move`.
- `InputState::stable_dt` is the time of the last frame, or a prediction of
  it after an idle period; egui advises `stable_dt.min(0.1)` for
  animations.
- egui_kittest steps the time by 0.25 s per frame, and `Harness::run`
  panics after 4 frames that still ask for another; `HarnessBuilder`'s
  `with_step_dt` and `with_max_steps` change both.
- eframe chooses the graphics adapter with
  `WgpuSetupCreateNew::power_preference`, which is `HighPerformance` unless
  the environment variable `WGPU_POWER_PREF` says `low`, `high` or `none`.
  `NativeOptions::wgpu_options` sets it, and eframe re-exports `wgpu`.
  Where wgpu draws with OpenGL, there is only the adapter the system gives
  the context, and the power preference has no effect.
- On the user's laptop, wgpu chose an NVIDIA RTX 3050 through Vulkan by
  default and the integrated Intel Iris Xe with `WGPU_POWER_PREF=low`. The
  probe drew about 30 and 60 frames per second on the laptop's display.

## Goals / Non-Goals

**Goals:**

- One motion for all six lists, in `ListState`, so that each list gets it
  without changes to its view.
- A motion that does not depend on the frame rate, so that a slow frame
  neither speeds the list up nor slows it down.

**Non-Goals:**

- Moving the diff and blame, which use egui's `ScrollArea`, to the spring.
- An option for the stiffness or the speed of the wheel.

## Decisions

### 1. A critically damped spring in `ListState`

`ListState` gets two fields: `pending`, the distance the list still has to
move, in points and positive towards later rows, and `velocity`, in points
per second. Wheel input adds to `pending`. Each frame moves the list by one
step of a critically damped spring that brings `pending` to 0:

```
e = −pending, v = velocity, w = STIFFNESS, d = exp(−w·dt)
c = v + w·e
e′ = (e + c·dt)·d
v′ = (v − w·c·dt)·d
move the list by e′ − e; pending = −e′; velocity = v′
```

This is the exact solution of the spring over `dt`, so the list reaches the
same place at 30, 60 or 4 frames per second, and no frame time makes it
unstable. A burst of input moves the target, which changes the acceleration
of the list but not its velocity; the velocity carries over into the next
burst.

`STIFFNESS` is 8 per second, the value the user chose in the probe. From
rest, the list covers 90 % of a move in 0.5 s and rests after 0.8 s for one
notch of 40 points, after 1.2 s for 681 points. At 60 frames per second a
burst changes the distance moved per frame by at most 1.4 % of the burst;
egui's smoothing moves 32 % of it in the first frame.

The list rests when `pending` is below 0.5 points and the velocity below
5 points per second: the rest of `pending` is moved at once, and both become
0. A motion that reaches the first or the last row stops there: the
position is clamped as today, and `pending` and `velocity` become 0, so the
list does not bounce. Input beyond an end is dropped when it arrives, so
that `pending` never reaches past the first or the last row. `pending` is
relative to the position, so rows added at the end while the history loads
change nothing, and the clamp handles rows that a refresh removed.

A step that would carry the list past its target ends at the target
instead: the list moves by the rest of `pending`, and both become 0. Input
in one direction never brings a critically damped spring past its target;
input against a fast motion can, when it leaves the target close ahead.
Scrolling back by 150 points 0.3 s after a burst of 681 points would carry
the list about 16 points past its target and back. Input against the motion
that puts the target behind the list keeps the velocity, so the list goes
on briefly before it turns; the manual check, which scrolls quickly up and
down, shows whether that feels right. If it does not, a follow-up drops the
velocity on input against the motion.

The spring runs with `stable_dt`, and one step covers at most 0.1 s, as
egui advises: after a stalled frame the list continues from where it was
instead of jumping. While `pending` or `velocity` is not 0, the list asks
for another frame.

A list that was not drawn in the previous pass, because its tab or its view
was hidden, ends a motion at its target when it is drawn again: `ListState`
keeps the pass it last stepped in. Otherwise a list would go on moving after
the user came back to its tab.

Alternatives considered:

- egui's smoothing, as today: each burst starts with a third of it in one
  frame, which is the stutter the user saw.
- The raw events without smoothing: the list jumps by whole bursts; in the
  probe it stuttered more than with egui's smoothing.
- A slower exponential smoothing: it still sets the velocity anew with each
  burst, only lower.
- A spring stepped with explicit Euler integration: its result depends on
  the frame rate, and it becomes unstable for long frames.

### 2. Wheel input read from egui's events

The lists read the `Event::MouseWheel` events of the frame themselves and
sort each of them as egui does:

- With the zoom modifier, or with the horizontal modifier and without the
  vertical one, the event does not scroll a list.
- Its vertical points are `Point` as they are, `Line` times
  `line_scroll_speed`, and `Page` times the height of the list.
- Inside a touchpad gesture, from `Start` to `End` or `Cancel`, and for
  `Point` steps shorter than 8 points, the points move the list at once.
- Everything else goes to the spring.

A gesture belongs to the input device, not to a list, so whether one is
under way is kept in egui's temporary data and updated from all events once
per pass, as `components::focus_visible` keeps the focus-visible state.
`ui::show` reads this wheel state at the start of every pass, as it reads
the focus-visible state, so that no start or end of a gesture is missed
while no list is drawn. The function returns the points to move at once and
the points for the spring.

A list takes both only while its response is hovered, as today, so that a
menu, a popup or the modal settings dialog above it keeps the input; it then
sets egui's `smooth_scroll_delta.y` to 0, as it does today. The spring steps
in every pass in which the list is drawn, hovered or not, so that a motion
still ends at its target when the pointer leaves the list.

egui still smooths the events a list took and passes them on over the next
passes (see Context). Once the pointer has left the list, a scroll area
under it, such as the diff, would take them and scroll by input that the
list has already moved by. The wheel state therefore also keeps `owed`, the
points that lists took and egui has not passed on yet, in the units of
`smooth_scroll_delta.y`: a list adds what it took and subtracts what it set
to 0, and `ui::show` takes up to `owed`, in its direction, from
`smooth_scroll_delta.y` at the start of each pass, before any area reads it.
`owed` never goes past 0. The end of a gesture, which clears egui's
smoothing, sets it to 0, and so does a rest of less than one point, which
egui passes on at once.

Alternatives considered:

- The spring behind egui's `smooth_scroll_delta`: the input would pass two
  filters, and the list would lag behind both.
- Changing egui's smoothing: it has no option, and its state is private.
- Setting `smooth_scroll_delta.y` to 0 for as long as egui reports
  `is_scrolling`: the diff would lose its own input as well while the user
  scrolls it, because egui counts that as the same scrolling.

### 3. Keys, the scrollbar and jumps end the motion

`scroll_to`, which the scrollbar uses, `apply`, which the keys use, and
`reveal`, which a jump to a row uses, set `pending` and `velocity` to 0,
also when the row is already in view and nothing scrolls. The list goes
where the user asked and does not move on. `scroll_by` stays a move at once
and ends the motion as well. The spring moves the list through an inner
function that keeps them.

Alternative considered: animating the keys too. Page Down would then show
the new selection only after half a second, and `reveal` would no longer
show a row in the frame it was asked for, which the search and the sidebar
rely on.

### 4. The integrated graphics adapter

`main.rs` sets `NativeOptions::wgpu_options` to `WgpuSetup::CreateNew`
with the defaults of `WgpuSetupCreateNew::without_display_handle()`, which
eframe completes with the display handle, and with the power preference
from `PowerPreference::from_env()`, or `LowPower` without it. A small
function takes the value from the environment and returns the preference,
so that a unit test covers both cases without changing the environment of
the test process.

Where wgpu draws with OpenGL, the preference has no effect (see Context).
The requirement therefore asks for the power-saving adapter, and promises
the integrated one only where the system lets an application choose.

git-bull draws a few thousand rectangles and glyphs per frame, which an
integrated adapter draws in well under a frame; on a laptop it also draws
on the display it is wired to.

Alternatives considered:

- Leaving it to `WGPU_POWER_PREF`: every user of a laptop with two adapters
  would have to know about it.
- Choosing the adapter by its type with `native_adapter_selector`: more
  code for the same result, since wgpu already maps `LowPower` to the
  preference of each backend.
- A setting in the dialog: no user has asked for the dedicated adapter, and
  the environment variable still offers it.

### 5. Tests

- Unit tests in `virtual_list.rs` for the spring, with fixed frame times,
  including input against a fast motion and a stalled frame, and for the
  sorting of wheel events and `owed`.
- UI tests in `tests/virtual_list.rs` for the scenarios of the requirement
  "Smooth scrolling of lists", with a harness that steps at 60 frames per
  second. The existing tests that turn the wheel run until the list rests.
  Further UI tests turn the wheel over the commit list while the settings
  dialog is open, and switch tabs during a motion.
- A pass of the benchmark `scrolling` with the mouse wheel alone. In its
  existing passes, Page Down ends each motion of the spring a frame after
  the wheel started it, so the frames in which the spring moves would go
  unmeasured.
- A unit test of the power preference with and without a value from the
  environment.
- A manual check on Windows with the touchpad and a mouse wheel, and in
  Task Manager, which adapter git-bull draws with.

## Risks / Trade-offs

- [A mouse wheel feels slower] → One notch reaches 90 % of its 40 points in
  0.5 s instead of 0.1 s. On Windows a mouse wheel and a touchpad cannot be
  told apart, so both get the spring. The manual check includes a mouse
  wheel; if it feels slow, a follow-up raises the stiffness for `Line` steps
  that are whole numbers, which only mouse wheels send.
- [The diff and the lists scroll differently] → The diff keeps egui's
  smoothing until a later change moves it to the spring.
- [egui passes the input a list took on to the diff] → `owed` takes it back
  before any area reads it (decision 2); a UI test moves the pointer onto
  the diff right after a burst.
- [A fast list goes on briefly when the input turns back] → Only when the
  target ends up behind the list; a target still ahead stops the list there
  (decision 1). The manual check scrolls quickly up and down.
- [A desktop whose display is wired to the dedicated adapter] → With an
  integrated adapter enabled as well, git-bull draws on it and the system
  copies each frame to the dedicated one. git-bull's frames are small, and
  `WGPU_POWER_PREF=high` chooses the dedicated adapter.
- [UI tests that scroll need more frames] → The spring moves for up to
  1.2 s, more than egui_kittest's default of 4 frames of 0.25 s, of which
  the spring counts 0.1 s each. Tests that turn the wheel use a harness at
  60 frames per second with enough frames.

## Migration Plan

Nothing to migrate: no setting and no stored state changes. Reverting the
change restores egui's smoothing and eframe's choice of adapter.
