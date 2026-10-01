# Tasks

Group 1 comes first; group 2 does not depend on it. The manual check of
group 3 needs both. Each task names the test that shows it works.

## 1. Spring

- [ ] 1.1 Add the spring to `ListState` in `virtual_list.rs`: the fields `pending` and `velocity`, a function that adds wheel input and drops what reaches past either end, and the step of decision 1 with `STIFFNESS` 8, which rests below 0.5 points and 5 points per second and stops at either end; let `scroll_to`, `scroll_by`, `apply` and `reveal` end the motion, `reveal` also when the row is already in view (design, decisions 1 and 3); verify unit tests that one notch of 40 points rests within 1 s at 60 frames per second and never passes its target, that the position after 0.5 s is the same at 30 and at 60 frames per second, that a burst of 681 points changes the distance per frame by less than 2 % of it, that a second burst during a motion keeps the velocity, that `scroll_to`, `scroll_by`, `apply` and `reveal` end the motion, that neither end makes the list move back, that rows added or removed during a motion keep the list within them, and that the existing unit tests of `virtual_list.rs` pass
- [ ] 1.2 Sort the wheel events of a frame as decision 2 describes, once per frame, into points to move at once and points for the spring, and keep whether a touchpad gesture is under way in egui's temporary data; verify unit tests for each unit (`Point`, `Line` with `line_scroll_speed`, `Page` with the height of the list), for the zoom modifier and the horizontal modifier, for a gesture from `Start` to `End` across frames, and for `Point` steps below 8 points and of 8 points or more
- [ ] 1.3 Let `VirtualList::show` take this input when the pointer is over the list, set egui's `smooth_scroll_delta.y` to 0, run the spring with `stable_dt.min(0.1)` and ask for another frame while it moves; verify UI tests in `tests/virtual_list.rs` with a harness that steps at 60 frames per second for the scenarios "Bursts of a touchpad", "One notch of the mouse wheel", "Touchpad gesture", "A key ends the motion" and "End of the list" of `application-shell`, a UI test that Ctrl and the wheel do not scroll the list, and that the existing tests of `tests/virtual_list.rs` and `tests/benchmarks.rs` that turn the wheel pass

## 2. Graphics adapter

- [ ] 2.1 Set the power preference of wgpu in `main.rs` to `LowPower`, unless `WGPU_POWER_PREF` chooses another (design, decision 4), and say in the README, below the graphics interfaces git-bull draws with, that it uses the integrated adapter and how `WGPU_POWER_PREF=high` chooses the dedicated one; verify a unit test of the function that chooses the preference, with and without a value from the environment, for the scenarios "Laptop with two graphics adapters" and "Dedicated adapter on request" of `application-shell`

## 3. Manual check

- [ ] 3.1 The user scrolls the commit list on Windows with the touchpad, quickly up and down, and with a mouse wheel, and Task Manager shows git-bull on the integrated graphics adapter, and on the dedicated one when started with `WGPU_POWER_PREF=high`; verify that the result and the date are recorded in this task

## 4. Final check

- [ ] 4.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`, `cargo deny check` and `openspec validate smooth-scrolling --strict`; verify all succeed and CI is green on Linux, Windows and macOS
