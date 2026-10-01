# Proposal

## Why

Scrolling the commit list with a touchpad on Windows stutters: when the user
scrolls quickly up and down, the list moves in jerks. Measurements on the
user's laptop found two causes.

- Windows reports the touchpad as fractional lines of a mouse wheel, and
  they arrive in uneven bursts: pauses of up to 260 ms, then up to 681
  points at once. egui smooths each burst on its own, 90 % of it within
  0.1 s, so the speed of the list jumps with every burst. At 60 frames per
  second the first frame of a burst moves the list by a third of it.
- On a laptop with an integrated and a dedicated graphics adapter, git-bull
  draws with the dedicated one, as eframe prefers it. On the user's laptop
  that is an NVIDIA adapter through Vulkan, which reached about 30 frames per
  second on the laptop's own display, against 60 with the integrated Intel
  adapter.

A probe let the user compare three ways to move a list on the same
touchpad: egui's smoothing, the raw events, and a critically damped spring
towards the position the events ask for. The spring with a stiffness of
8 per second felt smoothest. Measured, the change of the distance moved from
one frame to the next stayed below 3.3 points in 90 % of the frames, against
11.1 with egui's smoothing, and below 41 points at most, against 241.

## What Changes

- The long lists of git-bull (commit list, sidebar, the file lists of the
  commit panel and of File status, file history and search results) follow
  the mouse wheel and the touchpad with a critically damped spring. Their
  speed no longer jumps with each burst of input, and they come to rest at
  the position the input asked for, without overshooting it.
- A touchpad that the system reports as a gesture with a start and an end,
  or in small steps of points, as macOS and Wayland do, still moves the
  lists at once, as egui does today.
- Keys, the scrollbar and jumps to a selected row move the lists at once,
  as today, and end a motion of the spring still under way.
- git-bull prefers the integrated, power-saving graphics adapter on
  computers that have two. The environment variable `WGPU_POWER_PREF` still
  chooses another.

Out of scope: the diff and blame, which scroll with egui's `ScrollArea` and
keep its smoothing; kinetic scrolling on Windows after the fingers leave the
touchpad; the number of lines per notch of the wheel; and where the commit
list stands after a reload of the history.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `application-shell`: New requirements "Smooth scrolling of lists" and
  "Graphics adapter".

## Impact

- `crates/gitbull-app/src/virtual_list.rs`: the spring in `ListState`, the
  wheel input read from egui's events instead of its smoothed scroll delta.
- `crates/gitbull-app/src/main.rs`: the power preference of wgpu in the
  `NativeOptions`.
- Tests in `virtual_list.rs` and `tests/virtual_list.rs`.
- `README.md`: which graphics adapter git-bull uses, and how to choose the
  other one.
- No new dependency: eframe re-exports wgpu.
- The change builds on `ui-design-system` (pull request #15), whose
  `virtual_list.rs` and focus-visible state it extends.
