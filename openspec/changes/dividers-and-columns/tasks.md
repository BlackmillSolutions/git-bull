# Tasks

Group 1 adds the fields of the settings that the later groups record.
Group 2 builds the columns with draggable edges in the commit list, and
group 3 gives them to the file history. Group 4, the divider in the commit
panel, needs only group 1. Group 5 checks the whole change with the user.
Each task names the test that shows it works.

## 1. Settings

- [x] 1.1 Add `Layout::commit_details_height` and `Layout::path_column` as `Option<f32>` in `crates/gitbull-core/src/settings.rs` (design, decision 7); verify unit tests that a settings file without them reads them as absent and keeps every other value of `[layout]`, and that both survive saving and reading, for the scenario "Layout survives a restart" of `app-settings`

## 2. Columns of the commit list

- [x] 2.1 Add `crates/gitbull-app/src/columns.rs` with `Widths<N>`, `cells` and the clamp of a dragged width by its range and by the 120 points the Description keeps, and move `text_cell` there (design, decisions 2 to 4); verify unit tests that `cells` lays out the Graph from the left, the trailing columns from the right and the Description between them at least zero wide, as `commit_list::columns` does today, that a dragged width stops at its range and where the Description would get narrower than 120 points, that a Description already narrower lets a column only shrink, and that the width follows the pointer less the distance at which the edge was taken
- [x] 2.2 Add `virtual_list::scrolls(rows, height)` and use it in `VirtualList::show` (design, decision 6); verify unit tests that it is true when the rows are higher than the view and false when they fit, and that the tests of `tests/virtual_list.rs` pass
- [x] 2.3 Add `columns::header`, which draws the titles, a line of one point at each edge with the strokes of a panel's divider, an edge of 8 points with `CursorIcon::ResizeHorizontal`, handles the drags and leaves room for the scrollbar; let the commit list draw its header and rows through `columns` with the widths from `graph_column`, `date_column`, `author_column` and `hash_column`, recorded in the frames in which a width changed (design, decisions 2 to 7); verify UI tests in `tests/commit_list.rs` for the scenarios "Column right of the Description is resized" for the edges left of Date, Author and Commit with the saved width of each, "Description keeps a minimum width" and "Pointer over an edge" of `application-shell`, that the Commit header starts where the hashes of the rows start in a list that scrolls, that widths saved in the settings are used at start, and that `dragging_the_edge_of_the_graph_column_changes_and_keeps_its_width` and the other tests of `tests/commit_list.rs` pass
- [x] 2.4 Update the snapshots of the window and of the graph, which now show the lines at the edges of the headers; verify that `tests/window_snapshots.rs` and `tests/graph_snapshots.rs` pass with the new images. The snapshots of the graph cut out the rows without the header and stay as they were; the five of the main window show the lines.

## 3. Columns of the file history

- [ ] 3.1 Add `column-path = Path` to `crates/gitbull-app/i18n/en-US.ftl` and `Msg::ColumnPath` (design, decision 2); verify the unit test `every_text_of_the_ui_exists_in_english` of `i18n.rs`
- [ ] 3.2 Give the list of the file history a header through `columns::header` with Description, Path, Date, Author and Commit, and draw its rows from `cells`, with `path_column` and the shared `date_column`, `author_column` and `hash_column`, Author 160 points wide by default (design, decisions 2 and 7); verify UI tests in `tests/file_history.rs` for the scenarios "Headers are shown", "Path column is resized", "Widths shared with the commit list" and "Path width survives a restart" of `file-history`, with the saved width of Path after the drag, and that `each_entry_shows_its_description_and_the_path_the_file_had`, `every_widget_tab_reaches_in_the_file_history_has_a_role_and_a_name` and the other tests of `tests/file_history.rs` pass

## 4. Divider in the commit panel

- [ ] 4.1 Put the message and the fields of `commit_panel::show` in `Panel::top("commit_details")`, resizable and without a frame of its own, from 40 points to the height below the title less `MIN_FILE_ROOM`, by default `(height - MIN_FILE_ROOM).max(height * 0.4)` or `commit_details_height`, recorded every frame, with the list of changed files below it (design, decision 1); verify UI tests in `tests/commit_panel.rs` for the scenarios "Divider in the commit panel is dragged", "Divider in the commit panel stays" and "Divider in the commit panel keeps room for the files" of `application-shell`, that the pointer over the divider shows `CursorIcon::ResizeVertical` (scenario "Pointer over an edge"), that a saved `commit_details_height` is used at start and the dragged height is saved, that the row "Uncommitted changes" shows no divider, and that the other tests of `tests/commit_panel.rs` and `tests/stash_details.rs` pass
- [ ] 4.2 Update the snapshots of the window, which show the commit details at their default height; verify that `tests/window_snapshots.rs` passes with the new images

## 5. Check with the user

- [ ] 5.1 The user approves the snapshots of tasks 2.4 and 4.2 and checks on Windows with a release build: dragging the divider in the commit panel, selecting commits with short and long messages, dragging every edge of the headers of the commit list and of the file history, a narrow window, and that every divider and width is as left after a restart; verify that the result and the date are recorded in this task
- [ ] 5.2 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace`, `cargo deny check` and `openspec validate dividers-and-columns --strict`; verify all succeed and CI is green on Linux, Windows and macOS
