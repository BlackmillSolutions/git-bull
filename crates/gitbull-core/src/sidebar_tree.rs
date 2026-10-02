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
        /// The stash commit, which names the stash while newer ones are
        /// made before it.
        commit: String,
        selector: String,
        message: String,
    },
    Submodule {
        path: String,
        /// Only initialised submodules can be opened.
        initialised: bool,
    },
}

/// What a row shows, by what it is rather than where it is, so that a
/// selection finds its row again when the rows change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarKey {
    Section(Section),
    View(View),
    Folder {
        section: Section,
        path: String,
    },
    /// A reference by its full name, which no other section has.
    Reference(String),
    /// A stash by its commit.
    Stash(String),
    /// A submodule by its path.
    Submodule(String),
}

impl SidebarRow {
    pub fn key(&self) -> SidebarKey {
        match self {
            SidebarRow::Section { section, .. } => SidebarKey::Section(*section),
            SidebarRow::View(view) => SidebarKey::View(*view),
            SidebarRow::Folder { section, path, .. } => SidebarKey::Folder {
                section: *section,
                path: path.clone(),
            },
            SidebarRow::Reference { name, .. } => SidebarKey::Reference(name.clone()),
            SidebarRow::Stash { commit, .. } => SidebarKey::Stash(commit.clone()),
            SidebarRow::Submodule { path, .. } => SidebarKey::Submodule(path.clone()),
        }
    }

    /// Whether the row shows what `key` names, without copying its name as
    /// `key` does.
    fn shows(&self, key: &SidebarKey) -> bool {
        match (self, key) {
            (SidebarRow::Section { section, .. }, SidebarKey::Section(wanted)) => section == wanted,
            (SidebarRow::View(view), SidebarKey::View(wanted)) => view == wanted,
            (
                SidebarRow::Folder { section, path, .. },
                SidebarKey::Folder {
                    section: wanted_section,
                    path: wanted_path,
                },
            ) => section == wanted_section && path == wanted_path,
            (SidebarRow::Reference { name, .. }, SidebarKey::Reference(wanted)) => name == wanted,
            (SidebarRow::Stash { commit, .. }, SidebarKey::Stash(wanted)) => commit == wanted,
            (SidebarRow::Submodule { path, .. }, SidebarKey::Submodule(wanted)) => path == wanted,
            _ => false,
        }
    }
}

/// The row among `rows` that shows what `key` names; none while it is
/// hidden by the filter or a collapsed section or folder, or gone.
pub fn row_of(rows: &[SidebarRow], key: &SidebarKey) -> Option<usize> {
    rows.iter().position(|row| row.shows(key))
}

/// The rows to show for `sidebar`, or only sections and views while it is
/// loading. The Workspace section offers `views`.
pub fn rows(
    sidebar: Option<&Sidebar>,
    head: &Head,
    state: &SidebarState,
    views: &[View],
) -> Vec<SidebarRow> {
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
        let kind =
            match section {
                Section::Workspace => {
                    rows.extend(views.iter().copied().map(SidebarRow::View));
                    continue;
                }
                Section::Branches => RefKind::Branch,
                Section::Tags => RefKind::Tag,
                Section::Remotes => RefKind::RemoteBranch,
                Section::Stashes => {
                    let stashes = sidebar.map(|s| s.stashes.as_slice()).unwrap_or_default();
                    rows.extend(stashes.iter().enumerate().map(|(index, stash)| {
                        SidebarRow::Stash {
                            index,
                            commit: stash.commit.clone(),
                            selector: stash.selector.clone(),
                            message: stash.message.clone(),
                        }
                    }));
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
                parents: Vec::new(),
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
        let rows = rows(
            Some(&sidebar()),
            &main(),
            &SidebarState::default(),
            &View::ALL,
        );
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
        let text = outline(&rows(Some(&sidebar()), &main(), &state, &View::ALL));
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
        let text = outline(&rows(Some(&sidebar()), &main(), &state, &View::ALL));
        assert!(text.contains(&"    release/ (collapsed)".to_owned()));
        assert!(!text.contains(&"      0.1".to_owned()));
        assert!(text.contains(&"    main".to_owned()));
    }

    #[test]
    fn with_a_detached_head_no_branch_is_current() {
        let head = Head::Detached("1111111111111111111111111111111111111111".into());
        let rows = rows(
            Some(&sidebar()),
            &head,
            &SidebarState::default(),
            &View::ALL,
        );
        assert!(
            !rows
                .iter()
                .any(|row| matches!(row, SidebarRow::Reference { current: true, .. }))
        );
    }

    #[test]
    fn a_remote_branch_named_like_the_current_branch_is_not_current() {
        let rows = rows(
            Some(&sidebar()),
            &main(),
            &SidebarState::default(),
            &View::ALL,
        );
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
        let text = outline(&rows(Some(&sidebar()), &main(), &state, &View::ALL));
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
        let text = outline(&rows(Some(&sidebar()), &main(), &state, &View::ALL));
        assert!(text.contains(&"      0.1".to_owned()), "{text:#?}");
    }

    #[test]
    fn the_filter_matches_the_whole_name_with_its_folders() {
        let state = SidebarState {
            filter: "origin/rel".into(),
            ..SidebarState::default()
        };
        let text = outline(&rows(Some(&sidebar()), &main(), &state, &View::ALL));
        assert!(text.contains(&"      0.1".to_owned()), "{text:#?}");
        assert!(!text.contains(&"    main".to_owned()));
    }

    #[test]
    fn while_loading_only_sections_and_views_are_shown() {
        let text = outline(&rows(None, &main(), &SidebarState::default(), &View::ALL));
        assert_eq!(text.len(), 3 + 6);
    }

    #[test]
    fn ten_thousand_tags_are_quick_to_lay_out() {
        let mut many = sidebar();
        many.references
            .extend((0..10_000).map(|n| reference(&format!("refs/tags/v{n}"), RefKind::Tag)));
        let started = std::time::Instant::now();
        let rows = rows(Some(&many), &main(), &SidebarState::default(), &View::ALL);
        assert!(rows.len() > 10_000);
        assert!(started.elapsed() < std::time::Duration::from_millis(500));

        // A selection near the end is found again once per layout.
        let last = SidebarKey::Reference("refs/tags/v9999".into());
        let started = std::time::Instant::now();
        let row = row_of(&rows, &last);
        assert!(started.elapsed() < std::time::Duration::from_millis(50));
        assert_eq!(row.map(|row| rows[row].key()), Some(last));
    }

    fn laid_out(sidebar: &Sidebar, state: &SidebarState) -> Vec<SidebarRow> {
        rows(Some(sidebar), &main(), state, &View::ALL)
    }

    fn tag() -> SidebarKey {
        SidebarKey::Reference("refs/tags/v1.0".into())
    }

    #[test]
    fn every_row_gives_the_key_of_what_it_shows() {
        let rows = laid_out(&sidebar(), &SidebarState::default());
        let keys: Vec<SidebarKey> = rows.iter().map(SidebarRow::key).collect();
        for key in [
            SidebarKey::Section(Section::Workspace),
            SidebarKey::View(View::FileStatus),
            SidebarKey::Folder {
                section: Section::Remotes,
                path: "origin/release".into(),
            },
            SidebarKey::Reference("refs/heads/main".into()),
            SidebarKey::Reference("refs/remotes/origin/main".into()),
            tag(),
            SidebarKey::Stash("2222222222222222222222222222222222222222".into()),
            SidebarKey::Submodule("libs/missing".into()),
        ] {
            assert!(keys.contains(&key), "{key:?} in {keys:#?}");
        }
        // Each key names one row.
        for (row, key) in keys.iter().enumerate() {
            assert_eq!(row_of(&rows, key), Some(row), "{key:?}");
        }
    }

    #[test]
    fn a_selected_tag_is_found_where_the_filter_moves_it() {
        let all = laid_out(&sidebar(), &SidebarState::default());
        let filtered = laid_out(
            &sidebar(),
            &SidebarState {
                filter: "v1".into(),
                ..SidebarState::default()
            },
        );
        let (before, after) = (row_of(&all, &tag()), row_of(&filtered, &tag()));
        assert!(after < before, "{before:?} {after:?}");
        assert_eq!(after.map(|row| filtered[row].key()), Some(tag()));
    }

    #[test]
    fn a_selected_tag_is_found_below_a_new_branch() {
        let before = row_of(&laid_out(&sidebar(), &SidebarState::default()), &tag());
        let mut more = sidebar();
        more.references
            .push(reference("refs/heads/another", RefKind::Branch));
        let rows = laid_out(&more, &SidebarState::default());
        let after = row_of(&rows, &tag());
        assert_eq!(after, before.map(|row| row + 1));
        assert_eq!(after.map(|row| rows[row].key()), Some(tag()));
    }

    #[test]
    fn a_selected_stash_is_found_by_its_commit_below_a_newer_one() {
        let selected = SidebarKey::Stash("2222222222222222222222222222222222222222".into());
        let mut more = sidebar();
        more.stashes.insert(
            0,
            Stash {
                commit: "5555555555555555555555555555555555555555".into(),
                parents: Vec::new(),
                selector: "stash@{0}".into(),
                message: "WIP on main: newer".into(),
            },
        );
        more.stashes[1].selector = "stash@{1}".into();
        let rows = laid_out(&more, &SidebarState::default());
        let row = row_of(&rows, &selected).expect("the stash");
        assert!(
            matches!(&rows[row], SidebarRow::Stash { message, index: 1, .. } if message == "WIP on main: try")
        );
    }

    #[test]
    fn a_hidden_or_gone_entry_has_no_row() {
        let filtered = SidebarState {
            filter: "graph".into(),
            ..SidebarState::default()
        };
        assert_eq!(row_of(&laid_out(&sidebar(), &filtered), &tag()), None);

        let collapsed_section = SidebarState {
            collapsed_sections: HashSet::from([Section::Tags]),
            ..SidebarState::default()
        };
        assert_eq!(
            row_of(&laid_out(&sidebar(), &collapsed_section), &tag()),
            None
        );

        let collapsed_folder = SidebarState {
            collapsed_folders: HashSet::from([(Section::Remotes, "origin/release".to_owned())]),
            ..SidebarState::default()
        };
        let release = SidebarKey::Reference("refs/remotes/origin/release/0.1".into());
        assert_eq!(
            row_of(&laid_out(&sidebar(), &collapsed_folder), &release),
            None
        );

        let mut fewer = sidebar();
        fewer.references.retain(|r| r.name != "refs/tags/v1.0");
        assert_eq!(
            row_of(&laid_out(&fewer, &SidebarState::default()), &tag()),
            None
        );
    }

    #[test]
    fn the_workspace_section_offers_the_views_given() {
        let text = outline(&rows(None, &main(), &SidebarState::default(), &View::BARE));
        assert_eq!(text[..3], ["+ Workspace", "  History", "  Search"]);
    }
}
