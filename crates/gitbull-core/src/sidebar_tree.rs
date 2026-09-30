//! The rows of the sidebar: sections, views, folders and entries, flattened
//! so that one list shows them, however many references there are.

use std::collections::{BTreeMap, HashSet};

use gitbull_git::head::Head;
use gitbull_git::refs::{RefKind, Reference};
use gitbull_git::stashes::SubmoduleState;

use crate::session::Sidebar;
use crate::workspace::View;

/// The sections, in the order they are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Section {
    Workspace,
    Branches,
    Tags,
    Remotes,
    Stashes,
    Submodules,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::Workspace,
        Section::Branches,
        Section::Tags,
        Section::Remotes,
        Section::Stashes,
        Section::Submodules,
    ];
}

/// What the user changed in the sidebar of one tab.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarState {
    pub collapsed_sections: HashSet<Section>,
    /// Folders by section and path, such as `feature` or `origin/release`.
    pub collapsed_folders: HashSet<(Section, String)>,
    /// Narrows branches, tags and remote branches to names containing it.
    pub filter: String,
}

/// One row of the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarRow {
    Section {
        section: Section,
        collapsed: bool,
    },
    View(View),
    Folder {
        section: Section,
        /// From the section down, such as `origin/release`.
        path: String,
        name: String,
        depth: usize,
        collapsed: bool,
    },
    Reference {
        section: Section,
        /// The full name, such as `refs/heads/feature/graph`.
        name: String,
        /// The last part of the name, such as `graph`.
        label: String,
        depth: usize,
        /// The branch that is checked out.
        current: bool,
    },
    Stash {
        index: usize,
        selector: String,
        message: String,
    },
    Submodule {
        path: String,
        /// Only initialised submodules can be opened.
        initialised: bool,
    },
}

/// The rows to show for `sidebar`, or only sections and views while it is
/// loading.
pub fn rows(sidebar: Option<&Sidebar>, head: &Head, state: &SidebarState) -> Vec<SidebarRow> {
    let filter = state.filter.to_lowercase();
    let current = match head {
        Head::Branch(name) => Some(name.as_str()),
        Head::Detached(_) => None,
    };
    let mut rows = Vec::new();
    for section in Section::ALL {
        let collapsed = state.collapsed_sections.contains(&section);
        rows.push(SidebarRow::Section { section, collapsed });
        if collapsed {
            continue;
        }
        let kind = match section {
            Section::Workspace => {
                rows.extend([View::History, View::FileStatus, View::Search].map(SidebarRow::View));
                continue;
            }
            Section::Branches => RefKind::Branch,
            Section::Tags => RefKind::Tag,
            Section::Remotes => RefKind::RemoteBranch,
            Section::Stashes => {
                let stashes = sidebar.map(|s| s.stashes.as_slice()).unwrap_or_default();
                rows.extend(
                    stashes
                        .iter()
                        .enumerate()
                        .map(|(index, stash)| SidebarRow::Stash {
                            index,
                            selector: stash.selector.clone(),
                            message: stash.message.clone(),
                        }),
                );
                continue;
            }
            Section::Submodules => {
                let submodules = sidebar.map(|s| s.submodules.as_slice()).unwrap_or_default();
                rows.extend(submodules.iter().map(|submodule| SidebarRow::Submodule {
                    path: submodule.path.clone(),
                    initialised: submodule.state != SubmoduleState::NotInitialised,
                }));
                continue;
            }
        };
        let Some(sidebar) = sidebar else {
            continue;
        };
        let mut tree = Folder::default();
        for reference in &sidebar.references {
            if reference.kind == kind && reference.short.to_lowercase().contains(&filter) {
                tree.insert(reference);
            }
        }
        let place = Place {
            section,
            state,
            // While filtering, every folder with a match is open.
            filtering: !filter.is_empty(),
            current: current.filter(|_| kind == RefKind::Branch),
        };
        tree.flatten(&place, "", 0, &mut rows);
    }
    rows
}

/// The references of one section below one folder.
#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<&'a str, Folder<'a>>,
    entries: Vec<(&'a str, &'a Reference)>,
}

/// What every folder of a section needs to know to lay out its rows.
struct Place<'a> {
    section: Section,
    state: &'a SidebarState,
    filtering: bool,
    /// The short name of the checked-out branch, in the Branches section.
    current: Option<&'a str>,
}

impl<'a> Folder<'a> {
    fn insert(&mut self, reference: &'a Reference) {
        let mut folder = self;
        let mut parts = reference.short.split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                folder.entries.push((part, reference));
            } else {
                folder = folder.folders.entry(part).or_default();
            }
        }
    }

    /// Folders first, each followed by what it contains, then the entries.
    fn flatten(&self, place: &Place<'_>, path: &str, depth: usize, rows: &mut Vec<SidebarRow>) {
        for (name, folder) in &self.folders {
            let path = if path.is_empty() {
                (*name).to_owned()
            } else {
                format!("{path}/{name}")
            };
            let collapsed = !place.filtering
                && place
                    .state
                    .collapsed_folders
                    .contains(&(place.section, path.clone()));
            rows.push(SidebarRow::Folder {
                section: place.section,
                path: path.clone(),
                name: (*name).to_owned(),
                depth,
                collapsed,
            });
            if !collapsed {
                folder.flatten(place, &path, depth + 1, rows);
            }
        }
        for (label, reference) in &self.entries {
            rows.push(SidebarRow::Reference {
                section: place.section,
                name: reference.name.clone(),
                label: (*label).to_owned(),
                depth,
                current: place.current == Some(reference.short.as_str()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitbull_git::stashes::{Stash, Submodule};

    fn reference(name: &str, kind: RefKind) -> Reference {
        let short = name
            .strip_prefix("refs/heads/")
            .or_else(|| name.strip_prefix("refs/remotes/"))
            .or_else(|| name.strip_prefix("refs/tags/"))
            .unwrap_or(name);
        Reference {
            name: name.to_owned(),
            short: short.to_owned(),
            kind,
            commit: Some("1111111111111111111111111111111111111111".into()),
            upstream: None,
        }
    }

    fn sidebar() -> Sidebar {
        Sidebar {
            references: vec![
                reference("refs/heads/feature/diff-view", RefKind::Branch),
                reference("refs/heads/feature/graph-layout", RefKind::Branch),
                reference("refs/heads/main", RefKind::Branch),
                reference("refs/remotes/origin/main", RefKind::RemoteBranch),
                reference("refs/remotes/origin/release/0.1", RefKind::RemoteBranch),
                reference("refs/tags/v1.0", RefKind::Tag),
            ],
            stashes: vec![Stash {
                commit: "2222222222222222222222222222222222222222".into(),
                selector: "stash@{0}".into(),
                message: "WIP on main: try".into(),
            }],
            submodules: vec![
                Submodule {
                    path: "libs/inner".into(),
                    commit: "3333333333333333333333333333333333333333".into(),
                    state: SubmoduleState::Current,
                },
                Submodule {
                    path: "libs/missing".into(),
                    commit: "4444444444444444444444444444444444444444".into(),
                    state: SubmoduleState::NotInitialised,
                },
            ],
        }
    }

    fn main() -> Head {
        Head::Branch("main".into())
    }

    /// The rows as indented text: `+` a section, `/` a folder, `*` the
    /// current branch.
    fn outline(rows: &[SidebarRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                SidebarRow::Section { section, collapsed } => {
                    format!(
                        "+ {section:?}{}",
                        if *collapsed { " (collapsed)" } else { "" }
                    )
                }
                SidebarRow::View(view) => format!("  {view:?}"),
                SidebarRow::Folder {
                    name,
                    depth,
                    collapsed,
                    ..
                } => format!(
                    "{}{name}/{}",
                    "  ".repeat(depth + 1),
                    if *collapsed { " (collapsed)" } else { "" }
                ),
                SidebarRow::Reference {
                    label,
                    depth,
                    current,
                    ..
                } => format!(
                    "{}{label}{}",
                    "  ".repeat(depth + 1),
                    if *current { " *" } else { "" }
                ),
                SidebarRow::Stash { message, .. } => format!("  {message}"),
                SidebarRow::Submodule { path, initialised } => {
                    format!(
                        "  {path}{}",
                        if *initialised {
                            ""
                        } else {
                            " (not initialised)"
                        }
                    )
                }
            })
            .collect()
    }

    #[test]
    fn sections_come_in_order_with_their_entries_grouped_by_slash() {
        let rows = rows(Some(&sidebar()), &main(), &SidebarState::default());
        assert_eq!(
            outline(&rows),
            [
                "+ Workspace",
                "  History",
                "  FileStatus",
                "  Search",
                "+ Branches",
                "  feature/",
                "    diff-view",
                "    graph-layout",
                "  main *",
                "+ Tags",
                "  v1.0",
                "+ Remotes",
                "  origin/",
                "    release/",
                "      0.1",
                "    main",
                "+ Stashes",
                "  WIP on main: try",
                "+ Submodules",
                "  libs/inner",
                "  libs/missing (not initialised)",
            ]
        );
    }

    #[test]
    fn a_collapsed_section_hides_its_rows_and_leaves_the_others() {
        let state = SidebarState {
            collapsed_sections: HashSet::from([Section::Tags]),
            ..SidebarState::default()
        };
        let text = outline(&rows(Some(&sidebar()), &main(), &state));
        assert!(text.contains(&"+ Tags (collapsed)".to_owned()));
        assert!(!text.contains(&"  v1.0".to_owned()));
        assert!(text.contains(&"  main *".to_owned()));
        assert!(text.contains(&"      0.1".to_owned()));
    }

    #[test]
    fn a_collapsed_folder_hides_what_it_contains() {
        let state = SidebarState {
            collapsed_folders: HashSet::from([(Section::Remotes, "origin/release".to_owned())]),
            ..SidebarState::default()
        };
        let text = outline(&rows(Some(&sidebar()), &main(), &state));
        assert!(text.contains(&"    release/ (collapsed)".to_owned()));
        assert!(!text.contains(&"      0.1".to_owned()));
        assert!(text.contains(&"    main".to_owned()));
    }

    #[test]
    fn with_a_detached_head_no_branch_is_current() {
        let head = Head::Detached("1111111111111111111111111111111111111111".into());
        let rows = rows(Some(&sidebar()), &head, &SidebarState::default());
        assert!(
            !rows
                .iter()
                .any(|row| matches!(row, SidebarRow::Reference { current: true, .. }))
        );
    }

    #[test]
    fn a_remote_branch_named_like_the_current_branch_is_not_current() {
        let rows = rows(Some(&sidebar()), &main(), &SidebarState::default());
        let current: Vec<&str> = rows
            .iter()
            .filter_map(|row| match row {
                SidebarRow::Reference {
                    name,
                    current: true,
                    ..
                } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(current, ["refs/heads/main"]);
    }

    #[test]
    fn the_filter_keeps_matching_references_with_their_folders_ignoring_case() {
        let state = SidebarState {
            filter: "GRAPH".into(),
            ..SidebarState::default()
        };
        let text = outline(&rows(Some(&sidebar()), &main(), &state));
        assert_eq!(
            text,
            [
                "+ Workspace",
                "  History",
                "  FileStatus",
                "  Search",
                "+ Branches",
                "  feature/",
                "    graph-layout",
                "+ Tags",
                "+ Remotes",
                "+ Stashes",
                "  WIP on main: try",
                "+ Submodules",
                "  libs/inner",
                "  libs/missing (not initialised)",
            ]
        );
    }

    #[test]
    fn the_filter_shows_matches_inside_collapsed_folders() {
        let state = SidebarState {
            filter: "0.1".into(),
            collapsed_folders: HashSet::from([(Section::Remotes, "origin".to_owned())]),
            ..SidebarState::default()
        };
        let text = outline(&rows(Some(&sidebar()), &main(), &state));
        assert!(text.contains(&"      0.1".to_owned()), "{text:#?}");
    }

    #[test]
    fn the_filter_matches_the_whole_name_with_its_folders() {
        let state = SidebarState {
            filter: "origin/rel".into(),
            ..SidebarState::default()
        };
        let text = outline(&rows(Some(&sidebar()), &main(), &state));
        assert!(text.contains(&"      0.1".to_owned()), "{text:#?}");
        assert!(!text.contains(&"    main".to_owned()));
    }

    #[test]
    fn while_loading_only_sections_and_views_are_shown() {
        let text = outline(&rows(None, &main(), &SidebarState::default()));
        assert_eq!(text.len(), 3 + 6);
    }

    #[test]
    fn ten_thousand_tags_are_quick_to_lay_out() {
        let mut many = sidebar();
        many.references
            .extend((0..10_000).map(|n| reference(&format!("refs/tags/v{n}"), RefKind::Tag)));
        let started = std::time::Instant::now();
        let rows = rows(Some(&many), &main(), &SidebarState::default());
        assert!(rows.len() > 10_000);
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
    }
}
