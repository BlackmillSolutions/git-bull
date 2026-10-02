//! The git-bull desktop application: egui UI, theme and translations.

pub mod app;
pub mod blame_view;
pub mod columns;
pub mod commit_list;
pub mod commit_panel;
pub mod components;
pub mod diff_view;
pub mod file_history_view;
pub mod file_list;
pub mod file_status_view;
pub mod fonts;
pub mod graph_view;
pub mod i18n;
pub mod icons;
pub mod native;
pub mod paths;
pub mod search_view;
pub mod sidebar_view;
pub mod style;
pub mod theme;
pub mod ui;
pub mod virtual_list;
#[cfg(test)]
mod vision;
