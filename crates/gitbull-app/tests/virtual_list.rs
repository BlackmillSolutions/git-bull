//! The list for millions of rows, driven like a user would.

use eframe::egui::{
    Event, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect, TouchPhase, Vec2, pos2, vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::virtual_list::{ListState, ROW_HEIGHT, VirtualList};

const ROWS: u64 = 2_000_000;
/// The frames of a display at 60 frames per second.
const FRAME: f32 = 1.0 / 60.0;
/// The largest burst of the touchpad the probe recorded, in points.
const BURST: f64 = 681.0;

struct Test {
    list: ListState,
    rows: u64,
    /// Where the list was drawn in the last frame.
    rect: Rect,
    clicked: Vec<u64>,
}

/// A list in a window at 60 frames per second, with room for the spring
/// to come to rest after the wheel.
fn harness(rows: u64) -> Harness<'static, Test> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
        .with_step_dt(FRAME)
        .with_max_steps(120)
        .build_ui_state(
            |ui, test: &mut Test| {
                let output =
                    VirtualList::new("list", test.rows).show(ui, &mut test.list, |ui, row, _| {
                        ui.label(format!("Row {row}"));
                    });
                test.rect = output.response.rect;
                test.clicked.extend(output.clicked);
            },
            Test {
                list: ListState::default(),
                rows,
                rect: Rect::NOTHING,
                clicked: Vec::new(),
            },
        )
}

fn view(harness: &Harness<'_, Test>) -> f64 {
    f64::from(harness.state().rect.height())
}

fn scroll_to_row(harness: &mut Harness<'_, Test>, row: u64, extra: f64) {
    harness.run();
    let view = view(harness);
    harness
        .state_mut()
        .list
        .scroll_to(row as f64 * f64::from(ROW_HEIGHT) + extra, ROWS, view);
    harness.run();
}

fn center_y(harness: &Harness<'_, Test>, label: &str) -> f32 {
    harness.get_by_label(label).rect().center().y
}

/// A point inside the row at `row_y` pixels below the top of the list.
fn in_row(harness: &Harness<'_, Test>, row_y: f32) -> Pos2 {
    let rect = harness.state().rect;
    pos2(rect.left() + 40.0, rect.top() + row_y + ROW_HEIGHT / 2.0)
}

#[test]
fn rows_are_drawn_exactly_in_place_at_row_one_and_a_half_million() {
    let mut harness = harness(ROWS);
    scroll_to_row(&mut harness, 1_500_000, 5.0);
    let top = harness.state().rect.top();
    assert_eq!(
        center_y(&harness, "Row 1500000"),
        top - 5.0 + ROW_HEIGHT / 2.0
    );
    assert_eq!(
        center_y(&harness, "Row 1500001"),
        top + 19.0 + ROW_HEIGHT / 2.0
    );
    assert_eq!(
        center_y(&harness, "Row 1500005"),
        top + 115.0 + ROW_HEIGHT / 2.0
    );
}

#[test]
fn only_rows_in_view_are_drawn() {
    let mut harness = harness(ROWS);
    scroll_to_row(&mut harness, 1_500_000, 0.0);
    assert!(harness.query_by_label("Row 1499999").is_none());
    assert!(harness.query_by_label("Row 1500000").is_some());
    let last_in_view = 1_500_000 + (view(&harness) / f64::from(ROW_HEIGHT)).ceil() as u64 - 1;
    assert!(
        harness
            .query_by_label(&format!("Row {last_in_view}"))
            .is_some()
    );
    assert!(
        harness
            .query_by_label(&format!("Row {}", last_in_view + 1))
            .is_none()
    );
}

fn wheel(harness: &mut Harness<'_, Test>, unit: MouseWheelUnit, delta: f32) {
    harness.run();
    let inside = in_row(harness, 0.0);
    harness.hover_at(inside);
    harness.event(Event::MouseWheel {
        unit,
        delta: Vec2::new(0.0, delta),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    harness.run();
}

#[test]
fn the_mouse_wheel_scrolls_the_list() {
    let mut harness = harness(ROWS);
    wheel(&mut harness, MouseWheelUnit::Line, -3.0);
    let position = harness.state().list.position();
    assert!(position > 0.0, "{position}");
}

#[test]
fn the_touchpad_scrolls_the_list_by_its_pixels() {
    let mut harness = harness(ROWS);
    wheel(&mut harness, MouseWheelUnit::Point, -48.0);
    let position = harness.state().list.position();
    assert!((position - 48.0).abs() < 0.5, "{position}");
}

#[test]
fn scrolling_up_at_the_top_stays_at_the_top() {
    let mut harness = harness(ROWS);
    wheel(&mut harness, MouseWheelUnit::Point, 48.0);
    assert_eq!(harness.state().list.position(), 0.0);
}

/// Clicks the row at `row_y` pixels below the top, which focuses the list.
fn click(harness: &mut Harness<'_, Test>, row_y: f32) {
    harness.run();
    let point = in_row(harness, row_y);
    harness.hover_at(point);
    harness.drag_at(point);
    harness.drop_at(point);
    harness.run();
}

fn selected_after(start: u64, key: Key) -> Option<u64> {
    let mut harness = harness(ROWS);
    harness.run();
    click(&mut harness, 0.0);
    let view = view(&harness);
    let state = &mut harness.state_mut().list;
    state.select(Some(start));
    state.reveal(start, ROWS, view);
    harness.run();
    harness.key_press(key);
    harness.run();
    harness.state().list.selected()
}

#[test]
fn clicking_a_row_selects_it() {
    let mut harness = harness(ROWS);
    click(&mut harness, 2.0 * ROW_HEIGHT);
    assert_eq!(harness.state().list.selected(), Some(2));
    assert_eq!(harness.state().clicked, [2]);
}

#[test]
fn clicking_deep_in_the_list_selects_the_row_under_the_pointer() {
    let mut harness = harness(ROWS);
    scroll_to_row(&mut harness, 1_500_000, 10.0);
    // The top row shows only its last 14 pixels.
    click(&mut harness, 14.0 - ROW_HEIGHT / 2.0 + 1.0);
    assert_eq!(harness.state().list.selected(), Some(1_500_001));
}

#[test]
fn down_selects_the_next_row() {
    assert_eq!(selected_after(1_500_000, Key::ArrowDown), Some(1_500_001));
}

#[test]
fn up_selects_the_previous_row() {
    assert_eq!(selected_after(1_500_000, Key::ArrowUp), Some(1_499_999));
}

#[test]
fn page_down_moves_by_the_rows_in_view() {
    let page = selected_after(1_500_000, Key::PageDown).unwrap() - 1_500_000;
    assert!(page >= 5, "{page}");
}

#[test]
fn page_up_moves_by_the_rows_in_view() {
    let page = 1_500_000 - selected_after(1_500_000, Key::PageUp).unwrap();
    assert!(page >= 5, "{page}");
}

#[test]
fn home_selects_the_first_row() {
    assert_eq!(selected_after(1_500_000, Key::Home), Some(0));
}

#[test]
fn end_selects_the_last_row() {
    assert_eq!(selected_after(3, Key::End), Some(ROWS - 1));
}

#[test]
fn the_selection_moved_with_keys_stays_in_view() {
    let mut harness = harness(ROWS);
    click(&mut harness, 0.0);
    harness.key_press(Key::End);
    harness.run();
    assert!(
        harness
            .query_by_label(&format!("Row {}", ROWS - 1))
            .is_some()
    );
    harness.key_press(Key::Home);
    harness.run();
    assert!(harness.query_by_label("Row 0").is_some());
}

#[test]
fn keys_do_nothing_while_the_list_has_no_focus() {
    let mut harness = harness(ROWS);
    harness.run();
    harness.key_press(Key::ArrowDown);
    harness.key_press(Key::End);
    harness.run();
    assert_eq!(harness.state().list.selected(), None);
    assert_eq!(harness.state().list.position(), 0.0);
}

#[test]
fn several_presses_in_one_frame_move_several_rows() {
    let mut harness = harness(ROWS);
    click(&mut harness, 0.0);
    // A slow frame can bring several repeats of a held key at once.
    for pressed in [true, false, true, false, true, false] {
        harness.input_mut().events.push(Event::Key {
            key: Key::ArrowDown,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run();
    assert_eq!(harness.state().list.selected(), Some(3));
}

#[test]
fn arrow_keys_stay_in_the_list_when_other_widgets_can_take_focus() {
    let mut harness = Harness::builder()
        .with_size(vec2(400.0, 400.0))
        .build_ui_state(
            |ui, test: &mut Test| {
                let _ = ui.button("Above");
                ui.allocate_ui(vec2(380.0, 240.0), |ui| {
                    let output = VirtualList::new("list", test.rows).show(
                        ui,
                        &mut test.list,
                        |ui, row, _| {
                            ui.label(format!("Row {row}"));
                        },
                    );
                    test.rect = output.response.rect;
                });
                let _ = ui.button("Below");
            },
            Test {
                list: ListState::default(),
                rows: ROWS,
                rect: Rect::NOTHING,
                clicked: Vec::new(),
            },
        );
    click(&mut harness, 0.0);
    for _ in 0..3 {
        harness.key_press(Key::ArrowDown);
        harness.run();
    }
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(harness.state().list.selected(), Some(2));
}

#[test]
fn dragging_the_scrollbar_thumb_scrolls_through_the_whole_list() {
    let mut harness = harness(ROWS);
    harness.run();
    let rect = harness.state().rect;
    let thumb = pos2(rect.right() - 5.0, rect.top() + 12.0);
    harness.hover_at(thumb);
    harness.drag_at(thumb);
    harness.run();
    for step in 1..=10 {
        harness.hover_at(pos2(thumb.x, thumb.y + step as f32 * 100.0));
        harness.run();
    }
    harness.drop_at(pos2(thumb.x, rect.bottom() + 100.0));
    harness.run();
    let end = ROWS as f64 * f64::from(ROW_HEIGHT) - view(&harness);
    assert_eq!(harness.state().list.position(), end);
    assert!(
        harness
            .query_by_label(&format!("Row {}", ROWS - 1))
            .is_some()
    );
}

#[test]
fn a_short_list_has_no_scrollbar_and_rows_fill_the_width() {
    let mut harness = harness(3);
    harness.run();
    let rect = harness.state().rect;
    let row = harness.get_by_label("Row 2").rect();
    assert!(row.bottom() <= rect.top() + 3.0 * ROW_HEIGHT);
    wheel(&mut harness, MouseWheelUnit::Point, -48.0);
    assert_eq!(harness.state().list.position(), 0.0);
}

/// Puts the pointer over the list.
fn hover(harness: &mut Harness<'_, Test>) {
    harness.run();
    let inside = in_row(harness, 0.0);
    harness.hover_at(inside);
    harness.run();
}

fn wheel_event(unit: MouseWheelUnit, delta: f32, phase: TouchPhase, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit,
        delta: Vec2::new(0.0, delta),
        phase,
        modifiers,
    }
}

/// Turns the wheel in the next frame; negative values scroll towards later
/// rows.
fn turn(harness: &mut Harness<'_, Test>, unit: MouseWheelUnit, delta: f32) {
    let event = wheel_event(unit, delta, TouchPhase::Move, Modifiers::NONE);
    harness.input_mut().events.push(event);
}

/// Steps `frames` frames and returns the position of the list after each.
fn frames(harness: &mut Harness<'_, Test>, frames: usize) -> Vec<f64> {
    (0..frames)
        .map(|_| {
            harness.step();
            harness.state().list.position()
        })
        .collect()
}

/// The largest change of the distance the list moved from one frame to the
/// next, from rest at `start`.
fn largest_change(start: f64, positions: &[f64]) -> f64 {
    let moved: Vec<f64> = std::iter::once(start)
        .chain(positions.iter().copied())
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    std::iter::once(0.0)
        .chain(moved)
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f64::max)
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.01, "{actual} ≠ {expected}");
}

#[test]
fn a_burst_of_the_touchpad_changes_the_speed_without_jumps() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    // Windows reports the touchpad as fractional lines of a mouse wheel.
    turn(&mut harness, MouseWheelUnit::Line, -(BURST / 40.0) as f32);
    let positions = frames(&mut harness, 90);
    let change = largest_change(0.0, &positions);
    assert!(change < 0.02 * BURST, "{change}");
    assert_close(harness.state().list.position(), BURST);
}

#[test]
fn one_notch_of_the_mouse_wheel_moves_the_list_by_40_points_within_a_second() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    turn(&mut harness, MouseWheelUnit::Line, -1.0);
    let positions = frames(&mut harness, 60);
    assert!(positions.iter().all(|&p| p <= 40.0 + 1e-6), "{positions:?}");
    assert!(!harness.state().list.is_moving());
    assert_close(harness.state().list.position(), 40.0);
}

#[test]
fn a_touchpad_gesture_moves_the_list_in_the_frame_each_movement_arrives() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    let mut send = |phase, delta, expected| {
        let event = wheel_event(MouseWheelUnit::Point, delta, phase, Modifiers::NONE);
        harness.input_mut().events.push(event);
        harness.step();
        assert_eq!(harness.state().list.position(), expected, "{phase:?}");
    };
    send(TouchPhase::Start, 0.0, 0.0);
    send(TouchPhase::Move, -20.0, 20.0);
    send(TouchPhase::Move, -30.0, 50.0);
    send(TouchPhase::Move, 10.0, 40.0);
    send(TouchPhase::End, 0.0, 40.0);
    assert!(!harness.state().list.is_moving());
}

#[test]
fn a_key_ends_the_motion_of_the_wheel() {
    let mut harness = harness(ROWS);
    click(&mut harness, 0.0);
    // The click took the pointer away.
    hover(&mut harness);
    turn(&mut harness, MouseWheelUnit::Line, -3.0);
    frames(&mut harness, 5);
    assert!(harness.state().list.is_moving());
    harness.input_mut().events.push(Event::Key {
        key: Key::PageDown,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    let list = &harness.state().list;
    let selected = list.selected().expect("a selection");
    assert!(selected > 0);
    let y = list.row_y(selected);
    assert!(
        y >= 0.0 && y + f64::from(ROW_HEIGHT) <= view(&harness),
        "{y}"
    );
    let position = list.position();
    let after = frames(&mut harness, 30);
    assert!(after.iter().all(|&p| p == position), "{after:?}");
}

#[test]
fn a_click_during_a_motion_selects_the_row_pressed_on_and_ends_the_motion() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    turn(&mut harness, MouseWheelUnit::Line, -(BURST / 40.0) as f32);
    frames(&mut harness, 5);
    // 60 points below the top of the list.
    let point = in_row(&harness, 2.0 * ROW_HEIGHT);
    harness.hover_at(point);
    harness.step();
    assert!(harness.state().list.is_moving());
    // As the list was last drawn.
    let position = harness.state().list.position();
    let pressed_on = ((position + 60.0) / f64::from(ROW_HEIGHT)).floor() as u64;

    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
        assert_eq!(harness.state().list.position(), position, "{pressed}");
    }

    assert_eq!(harness.state().clicked, [pressed_on]);
    assert!(!harness.state().list.is_moving());
    let after = frames(&mut harness, 30);
    assert!(after.iter().all(|&p| p == position), "{after:?}");
}

#[test]
fn a_tap_during_a_motion_selects_the_row_tapped_and_ends_the_motion() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    turn(&mut harness, MouseWheelUnit::Line, -(BURST / 40.0) as f32);
    frames(&mut harness, 5);
    // 60 points below the top of the list.
    let point = in_row(&harness, 2.0 * ROW_HEIGHT);
    harness.hover_at(point);
    harness.step();
    assert!(harness.state().list.is_moving());
    // As the list was last drawn.
    let position = harness.state().list.position();
    let tapped = ((position + 60.0) / f64::from(ROW_HEIGHT)).floor() as u64;

    // A tap on a touchpad presses and releases in the same frame.
    for pressed in [true, false] {
        harness.input_mut().events.push(Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.step();

    assert_eq!(harness.state().list.position(), position);
    assert_eq!(harness.state().clicked, [tapped]);
    assert!(!harness.state().list.is_moving());
}

#[test]
fn a_row_selected_again_during_a_motion_lets_the_list_rest_at_its_target() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    turn(&mut harness, MouseWheelUnit::Line, -(BURST / 40.0) as f32);
    frames(&mut harness, 5);
    let target = harness.state().list.target();
    // Above the rows in view by now.
    harness.state_mut().list.reselect(1);
    frames(&mut harness, 120);
    assert_close(harness.state().list.position(), target);
    assert_eq!(harness.state().list.selected(), Some(1));
}

#[test]
fn a_row_selected_again_at_rest_is_scrolled_into_view() {
    let mut harness = harness(ROWS);
    harness.run();
    harness.state_mut().list.reselect(1_000);
    harness.run();
    let list = &harness.state().list;
    assert_eq!(list.selected(), Some(1_000));
    let y = list.row_y(1_000);
    assert!(
        y >= 0.0 && y + f64::from(ROW_HEIGHT) <= view(&harness),
        "{y}"
    );
}

#[test]
fn the_wheel_stops_the_list_at_its_last_row() {
    let rows = 50;
    let mut harness = harness(rows);
    hover(&mut harness);
    let view = view(&harness);
    let end = rows as f64 * f64::from(ROW_HEIGHT) - view;
    harness.state_mut().list.scroll_to(end - 100.0, rows, view);
    turn(&mut harness, MouseWheelUnit::Line, -10.0);
    let positions = frames(&mut harness, 90);
    assert!(positions.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(positions.iter().all(|&p| p <= end));
    assert_close(harness.state().list.position(), end);
    assert!(!harness.state().list.is_moving());
    assert!(
        harness
            .query_by_label(&format!("Row {}", rows - 1))
            .is_some()
    );
}

#[test]
fn turned_back_during_a_fast_motion_the_list_rests_where_the_input_asked() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    turn(&mut harness, MouseWheelUnit::Line, -(BURST / 40.0) as f32);
    frames(&mut harness, 18);
    // Back by 150 points, less than is left of the motion.
    turn(&mut harness, MouseWheelUnit::Line, 150.0 / 40.0);
    let target = BURST - 150.0;
    let positions = frames(&mut harness, 120);
    assert!(
        positions.iter().all(|&p| p <= target + 0.01),
        "{positions:?}"
    );
    assert_close(harness.state().list.position(), target);
    assert!(!harness.state().list.is_moving());
}

#[test]
fn control_and_the_wheel_do_not_scroll_the_list() {
    let mut harness = harness(ROWS);
    hover(&mut harness);
    for modifiers in [Modifiers::CTRL, Modifiers::COMMAND] {
        let event = wheel_event(MouseWheelUnit::Line, -3.0, TouchPhase::Move, modifiers);
        harness.input_mut().events.push(event);
        harness.run();
        assert_eq!(harness.state().list.position(), 0.0, "{modifiers:?}");
    }
}
