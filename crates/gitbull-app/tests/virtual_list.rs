//! The list for millions of rows, driven like a user would.

use eframe::egui::{
    Event, Key, Modifiers, MouseWheelUnit, Pos2, Rect, TouchPhase, Vec2, pos2, vec2,
};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gitbull_app::virtual_list::{ListState, ROW_HEIGHT, VirtualList};

const ROWS: u64 = 2_000_000;

struct Test {
    list: ListState,
    rows: u64,
    /// Where the list was drawn in the last frame.
    rect: Rect,
    clicked: Vec<u64>,
}

fn harness(rows: u64) -> Harness<'static, Test> {
    Harness::builder()
        .with_size(vec2(400.0, 300.0))
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
