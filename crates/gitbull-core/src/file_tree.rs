//! The rows of a file list: the files a commit changed or the working copy
//! holds, flat in the order of Git or as a tree of folders, narrowed by a
//! filter (spec `file-lists`).
//!
//! [`FileOrder`] prepares what never changes for one list of files, the
//! order of the tree and the paths in lower case, on the worker that read
//! the files; sorting 50,000 paths takes longer than a frame.
//! [`FileTree`] builds the rows from it when the mode, the filter or a
//! collapsed folder change, never when they are only read.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gitbull_git::path::RepoPath;

/// A folder of a group, known by its path, such as `src/app`. The same
/// folder can hold files of several groups of the File status view.
pub type FolderKey = (usize, RepoPath);

/// How a file list shows its files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Full paths, in the order of Git.
    #[default]
    Flat,
    /// A tree of folders.
    Tree,
}

/// One row of a file list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The title of a group that shows files, when the list has titles.
    Title(usize),
    /// A folder of the tree; `folder` is its index in its group.
    Folder {
        group: usize,
        folder: usize,
        depth: usize,
        expanded: bool,
    },
    /// The file at `index` of its group, as Git listed it.
    File {
        group: usize,
        index: usize,
        depth: usize,
    },
}

/// What is selected, kept across builds of the rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selected {
    File { group: usize, index: usize },
    Folder { group: usize, path: RepoPath },
}

/// The prepared order of one list of files.
#[derive(Debug, Default)]
pub struct FileOrder {
    groups: Vec<GroupOrder>,
    titles: bool,
}

impl FileOrder {
    /// Prepares `groups`, each a list of files as their path and the path
    /// a renamed or copied file came from. With `titles`, each group that
    /// shows files is shown under its title.
    pub fn new<'a, G, F>(groups: G, titles: bool) -> FileOrder
    where
        G: IntoIterator<Item = F>,
        F: IntoIterator<Item = (&'a RepoPath, Option<&'a RepoPath>)>,
    {
        FileOrder {
            groups: groups.into_iter().map(GroupOrder::new).collect(),
            titles,
        }
    }

    /// The number of groups.
    pub fn groups(&self) -> usize {
        self.groups.len()
    }

    /// The files of `group` in the order of Git, as their path and the path
    /// a renamed or copied file came from.
    pub fn files(&self, group: usize) -> impl Iterator<Item = (&RepoPath, Option<&RepoPath>)> {
        self.groups[group]
            .files
            .iter()
            .map(|file| (&file.path, file.old_path.as_ref()))
    }

    /// The path of the file at `index` of `group`.
    pub fn file_path(&self, group: usize, index: usize) -> &RepoPath {
        &self.groups[group].files[index].path
    }

    /// The path the renamed or copied file at `index` of `group` came from.
    pub fn file_old_path(&self, group: usize, index: usize) -> Option<&RepoPath> {
        self.groups[group].files[index].old_path.as_ref()
    }

    /// The name of the file at `index` of `group`, the last part of its
    /// path.
    pub fn file_name(&self, group: usize, index: usize) -> Cow<'_, str> {
        let file = &self.groups[group].files[index];
        String::from_utf8_lossy(&file.path.as_bytes()[file.name_start..])
    }

    /// Where a renamed or copied file came from, as the tree shows it: the
    /// name alone when it stayed in its folder, else the whole old path.
    pub fn came_from(&self, group: usize, index: usize) -> Option<Cow<'_, str>> {
        let file = &self.groups[group].files[index];
        let old = file.old_path.as_ref()?;
        let folder = &file.path.as_bytes()[..file.name_start];
        Some(match old.as_bytes().strip_prefix(folder) {
            Some(name) if !name.contains(&b'/') => String::from_utf8_lossy(name),
            _ => old.to_string_lossy(),
        })
    }

    /// The path of folder `folder` of `group`, such as `src/app`.
    pub fn folder_path(&self, group: usize, folder: usize) -> &RepoPath {
        &self.groups[group].folders[folder].path
    }

    /// The name of folder `folder` of `group` as its row shows it; a folder
    /// whose only content is one folder shares its row, as `src/app`.
    pub fn folder_name(&self, group: usize, folder: usize) -> &str {
        &self.groups[group].folders[folder].name
    }
}

/// One group of a [`FileOrder`].
#[derive(Debug, Default)]
struct GroupOrder {
    files: Vec<FileInfo>,
    folders: Vec<FolderInfo>,
    /// The tree in pre-order: in each folder its folders, then its files,
    /// each sorted by name.
    nodes: Vec<Node>,
    folder_by_path: HashMap<RepoPath, usize>,
}

#[derive(Debug)]
struct FileInfo {
    path: RepoPath,
    old_path: Option<RepoPath>,
    /// Where the name begins in the path.
    name_start: usize,
    lower: String,
    lower_old: Option<String>,
}

impl FileInfo {
    fn matches(&self, filter: &str) -> bool {
        self.lower.contains(filter)
            || self
                .lower_old
                .as_ref()
                .is_some_and(|old| old.contains(filter))
    }
}

#[derive(Debug)]
struct FolderInfo {
    path: RepoPath,
    name: String,
    /// The node after the folder and everything inside it.
    end: usize,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    kind: NodeKind,
    depth: u32,
    /// The folder that holds the node.
    parent: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
enum NodeKind {
    Folder(u32),
    File(u32),
}

/// A folder while the tree is put together.
#[derive(Default)]
struct Dir {
    name: Vec<u8>,
    dirs: Vec<usize>,
    files: Vec<usize>,
    by_name: HashMap<Vec<u8>, usize>,
}

/// Sorts by name regardless of case, then by the bytes.
fn name_key(name: &[u8]) -> (String, Vec<u8>) {
    (String::from_utf8_lossy(name).to_lowercase(), name.to_vec())
}

impl GroupOrder {
    fn new<'a>(
        files: impl IntoIterator<Item = (&'a RepoPath, Option<&'a RepoPath>)>,
    ) -> GroupOrder {
        let files: Vec<FileInfo> = files
            .into_iter()
            .map(|(path, old_path)| FileInfo {
                path: path.clone(),
                old_path: old_path.cloned(),
                name_start: path
                    .as_bytes()
                    .iter()
                    .rposition(|&b| b == b'/')
                    .map_or(0, |slash| slash + 1),
                lower: path.to_string_lossy().to_lowercase(),
                lower_old: old_path.map(|old| old.to_string_lossy().to_lowercase()),
            })
            .collect();

        let mut dirs = vec![Dir::default()];
        for (index, file) in files.iter().enumerate() {
            let bytes = file.path.as_bytes();
            let mut dir = 0;
            if file.name_start > 0 {
                for part in bytes[..file.name_start - 1].split(|&b| b == b'/') {
                    dir = match dirs[dir].by_name.get(part) {
                        Some(&child) => child,
                        None => {
                            let child = dirs.len();
                            dirs.push(Dir {
                                name: part.to_vec(),
                                ..Dir::default()
                            });
                            dirs[dir].by_name.insert(part.to_vec(), child);
                            dirs[dir].dirs.push(child);
                            child
                        }
                    };
                }
            }
            dirs[dir].files.push(index);
        }
        for dir in 0..dirs.len() {
            let mut children = std::mem::take(&mut dirs[dir].dirs);
            children.sort_by_cached_key(|&child| name_key(&dirs[child].name));
            dirs[dir].dirs = children;
            dirs[dir].files.sort_by_cached_key(|&file| {
                name_key(&files[file].path.as_bytes()[files[file].name_start..])
            });
        }

        let mut group = GroupOrder {
            files,
            ..GroupOrder::default()
        };
        group.emit(&dirs, 0, 0, None, &[]);
        group
    }

    /// Adds the folders and files of `dir` to the nodes, at `depth` inside
    /// folder `parent`, whose path is `path`.
    fn emit(&mut self, dirs: &[Dir], dir: usize, depth: u32, parent: Option<u32>, path: &[u8]) {
        for &child in &dirs[dir].dirs {
            // A folder whose only content is one folder shares its row.
            let mut last = child;
            let mut name = String::from_utf8_lossy(&dirs[child].name).into_owned();
            while dirs[last].files.is_empty() && dirs[last].dirs.len() == 1 {
                last = dirs[last].dirs[0];
                name.push('/');
                name.push_str(&String::from_utf8_lossy(&dirs[last].name));
            }
            // The name went through a lossy conversion; the path keeps the
            // bytes of Git.
            let mut exact = path.to_vec();
            let mut walk = child;
            loop {
                if !exact.is_empty() {
                    exact.push(b'/');
                }
                exact.extend_from_slice(&dirs[walk].name);
                if walk == last {
                    break;
                }
                walk = dirs[walk].dirs[0];
            }
            let folder = self.folders.len();
            self.folder_by_path
                .insert(RepoPath::new(exact.clone()), folder);
            self.folders.push(FolderInfo {
                path: RepoPath::new(exact.clone()),
                name,
                end: 0,
            });
            self.nodes.push(Node {
                kind: NodeKind::Folder(folder as u32),
                depth,
                parent,
            });
            self.emit(dirs, last, depth + 1, Some(folder as u32), &exact);
            self.folders[folder].end = self.nodes.len();
        }
        for &file in &dirs[dir].files {
            self.nodes.push(Node {
                kind: NodeKind::File(file as u32),
                depth,
                parent,
            });
        }
    }
}

/// The place of each file of `group` of `order`, by its path.
fn places_of(order: &FileOrder, group: usize) -> HashMap<&RepoPath, usize> {
    order
        .groups
        .get(group)
        .into_iter()
        .flat_map(|g| g.files.iter().enumerate())
        .map(|(at, file)| (&file.path, at))
        .collect()
}

/// The folders collapsed without a filter, and those collapsed while
/// filtering, which each change of the filter forgets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collapsed {
    pub plain: HashSet<FolderKey>,
    pub filtered: HashSet<FolderKey>,
}

/// No row.
const NONE: u32 = u32::MAX;

/// The rows of one file list.
#[derive(Debug)]
pub struct FileTree {
    order: Arc<FileOrder>,
    mode: Mode,
    filter: String,
    lower_filter: String,
    collapsed: Collapsed,
    /// Kept while the filter shows nothing, so that it returns with the
    /// files.
    selected: Option<Selected>,
    /// The other files of a selection of several files, by their index in
    /// the group of the selected file, which is the anchor. Empty while one
    /// file is selected, and always when a folder is.
    also: HashSet<usize>,
    /// The file the keys move from, when it is not the anchor: the file a
    /// Shift click or Shift+Arrow ended at. Only set while `also` is not
    /// empty.
    lead: Option<usize>,
    /// The first file shown is to be selected once the filter shows one.
    wants_first: bool,
    rows: Vec<Row>,
    /// Per row, the row of the folder that holds it.
    parents: Vec<Option<u32>>,
    /// Per group and file, its row.
    file_rows: Vec<Vec<u32>>,
    /// Per group and folder, its row.
    folder_rows: Vec<Vec<u32>>,
    first_file: Option<usize>,
    selected_row: Option<usize>,
    builds: u64,
}

impl FileTree {
    /// The rows of `order`, flat, with every folder expanded.
    pub fn new(order: Arc<FileOrder>) -> FileTree {
        FileTree::shown_as(order, Mode::Flat, "")
    }

    /// The rows of `order` in `mode`, narrowed by `filter`, with every
    /// folder expanded; the rows are built once.
    pub fn shown_as(order: Arc<FileOrder>, mode: Mode, filter: &str) -> FileTree {
        let mut tree = FileTree::unbuilt(order, mode, filter);
        tree.build();
        tree
    }

    fn unbuilt(order: Arc<FileOrder>, mode: Mode, filter: &str) -> FileTree {
        FileTree {
            order,
            mode,
            filter: filter.to_owned(),
            lower_filter: filter.to_lowercase(),
            collapsed: Collapsed::default(),
            selected: None,
            also: HashSet::new(),
            lead: None,
            wants_first: false,
            rows: Vec::new(),
            parents: Vec::new(),
            file_rows: Vec::new(),
            folder_rows: Vec::new(),
            first_file: None,
            selected_row: None,
            builds: 0,
        }
    }

    /// Whether anything is selected: shown, kept while the filter hides
    /// it, or the first file waited for.
    pub fn holds_selection(&self) -> bool {
        self.selected.is_some() || self.wants_first
    }

    /// The rows of `order`, a list read again, with the mode, the filter,
    /// the collapsed folders that still hold files and the selection of
    /// this tree; a selected file is found again by its path in its group.
    ///
    /// A selected file that was shown and is no longer in its group, as
    /// after it was staged, gives way to the file below it in the rows that
    /// were shown, else to the one above it, each of its group; when the
    /// group has no such file, to the first file of another group.
    pub fn renewed(&self, order: Arc<FileOrder>) -> FileTree {
        let holds = |(group, path): &FolderKey| {
            order
                .groups
                .get(*group)
                .is_some_and(|g| g.folder_by_path.contains_key(path))
        };
        let mut collapsed = self.collapsed.clone();
        collapsed.plain.retain(holds);
        collapsed.filtered.retain(holds);

        // The other files of a selection of several files are found again by
        // their paths, all in one step: after "Select all" there are tens of
        // thousands of them. When the anchor left the group, the nearest of
        // the others above it, else below it, takes its place.
        let mut also = HashSet::new();
        let mut lead = None;
        let mut promoted = None;
        if let Some(Selected::File { group, index }) = &self.selected
            && !self.also.is_empty()
        {
            let places = places_of(&order, *group);
            let new_index = |old: usize| places.get(self.order.file_path(*group, old)).copied();
            let survivors: HashSet<usize> = self
                .also
                .iter()
                .copied()
                .filter(|&old| new_index(old).is_some())
                .collect();
            let mut others = survivors.clone();
            if new_index(*index).is_none() {
                let taking_over = self
                    .nearest_above_else_below(*group, self.selected_row, &survivors)
                    .or_else(|| survivors.iter().copied().min());
                if let Some(old) = taking_over {
                    others.remove(&old);
                    promoted = new_index(old).map(|index| Selected::File {
                        group: *group,
                        index,
                    });
                }
            }
            also = others.into_iter().filter_map(new_index).collect();
            lead = self
                .lead
                .and_then(new_index)
                .filter(|lead| also.contains(lead));
        }
        let selected = match &self.selected {
            Some(folder @ Selected::Folder { .. }) => Some(folder.clone()),
            Some(Selected::File { group, index }) => {
                let found = |index: usize| {
                    let path = self.order.file_path(*group, index);
                    order
                        .groups
                        .get(*group)
                        .and_then(|g| g.files.iter().position(|file| file.path == *path))
                        .map(|index| Selected::File {
                            group: *group,
                            index,
                        })
                };
                found(*index).or(promoted).or_else(|| {
                    // The file left its group. Its neighbours are looked up in
                    // one step each: after "Stage all" none of them is left,
                    // and a search of the list per neighbour would cost the
                    // square of its length.
                    let places = places_of(&order, *group);
                    self.neighbours_of_selected().find_map(|(_, neighbour)| {
                        let path = self.order.file_path(*group, neighbour);
                        places.get(path).map(|index| Selected::File {
                            group: *group,
                            index: *index,
                        })
                    })
                })
            }
            None => None,
        };
        // The file left its group, and no file of the group that was shown is
        // left: the first file of another group takes over.
        let left = match (&self.selected, &selected, self.selected_row) {
            (Some(Selected::File { group, .. }), None, Some(_)) => Some(*group),
            _ => None,
        };
        let mut tree = FileTree::unbuilt(order, self.mode, &self.filter);
        tree.collapsed = collapsed;
        tree.selected = selected;
        tree.also = also;
        tree.lead = lead;
        tree.wants_first = self.wants_first;
        tree.build();
        if let Some(left) = left
            && let Some(row) = tree.first_file_outside(left)
        {
            tree.select_row(row);
        }
        tree
    }

    /// The files of the group of the selected file in the rows shown, each
    /// with its row: those below it from the nearest on, then those above it
    /// from the nearest on. Nothing while no file is selected and shown.
    fn neighbours_of_selected(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let at = match (&self.selected, self.selected_row) {
            (Some(Selected::File { group, .. }), Some(row)) => Some((*group, row)),
            _ => None,
        };
        let (of, row) = at.unwrap_or((usize::MAX, 0));
        let file = move |at: usize| match self.rows.get(at) {
            Some(Row::File { group, index, .. }) if *group == of => Some((at, *index)),
            _ => None,
        };
        let below = (row + 1..self.rows.len()).filter_map(file);
        let above = (0..row).rev().filter_map(file);
        below.chain(above).filter(move |_| at.is_some())
    }

    /// The files of `group` that `filter` lets through, by their places in
    /// the group, also inside collapsed folders: what "all" of a group means
    /// while a filter narrows the list. The filter is given, so that it can
    /// be asked before the tree has taken it.
    pub fn files_matching(&self, group: usize, filter: &str) -> impl Iterator<Item = usize> + '_ {
        let filter = filter.to_lowercase();
        let files = self
            .order
            .groups
            .get(group)
            .map_or(&[][..], |group| group.files.as_slice());
        files
            .iter()
            .enumerate()
            .filter(move |(_, file)| filter.is_empty() || file.matches(&filter))
            .map(|(index, _)| index)
    }

    /// The row of the first file shown that is not of `group`.
    fn first_file_outside(&self, group: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| matches!(row, Row::File { group: of, .. } if *of != group))
    }

    /// The row that takes over when the selected file leaves its group: the
    /// file below it, else the one above it, each of its group, else the
    /// first file of another group. `None` while no file is selected and
    /// shown, and for the only file shown.
    pub fn successor_of_selected(&self) -> Option<usize> {
        let (_, anchor) = self.selected_file()?;
        self.successor_of_selection(|index| index == anchor)
    }

    /// The row that takes over when the files of the selection for which
    /// `leaves` answers yes leave their group, as after they were staged: the
    /// first file below the lowest row of them that stays, else the last file
    /// above the highest row that stays, each of their group in the rows as
    /// they are shown, else the first file of another group. A selected file
    /// that `leaves` does not name stays, as a file in conflict does. `None`
    /// while no file is selected and shown.
    pub fn successor_of_selection(&self, leaves: impl Fn(usize) -> bool) -> Option<usize> {
        let (group, anchor) = self.selected_file()?;
        let anchor_row = self.selected_row?;
        let (mut lowest, mut highest, mut any) = (usize::MAX, 0, false);
        let leaving = self
            .also
            .iter()
            .copied()
            .chain([anchor])
            .filter(|&index| leaves(index))
            .filter_map(|index| self.row_of_file(group, index));
        for row in leaving {
            (lowest, highest, any) = (lowest.min(row), highest.max(row), true);
        }
        if !any {
            (lowest, highest) = (anchor_row, anchor_row);
        }
        // Whether the row at `at` is a file of the group that stays; `None`
        // at the end of the group. A folder row is passed over, as is a file
        // that leaves.
        let stays = |at: usize| match self.rows[at] {
            Row::File {
                group: of, index, ..
            } if of == group => Some(!leaves(index)),
            Row::Folder { group: of, .. } if of == group => Some(false),
            _ => None,
        };
        let below =
            (lowest + 1..self.rows.len()).map_while(|at| stays(at).map(|stays| (at, stays)));
        let above = (0..highest)
            .rev()
            .map_while(|at| stays(at).map(|stays| (at, stays)));
        below
            .chain(above)
            .find_map(|(at, stays)| stays.then_some(at))
            .or_else(|| self.first_file_outside(group))
    }

    pub fn order(&self) -> &Arc<FileOrder> {
        &self.order
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// How often the rows were built.
    pub fn builds(&self) -> u64 {
        self.builds
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if mode != self.mode {
            self.mode = mode;
            self.build();
        }
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Narrows the list to the files whose path or old path contains
    /// `filter`, regardless of case. Every change expands the folders that
    /// were collapsed while filtering.
    pub fn set_filter(&mut self, filter: &str) {
        if filter == self.filter {
            return;
        }
        self.filter = filter.to_owned();
        self.lower_filter = filter.to_lowercase();
        self.collapsed.filtered.clear();
        self.narrow_selection();
        self.build();
    }

    /// The filter changed: the files it hides leave a selection of several
    /// files, so that no action reaches a file the user does not see, and
    /// when it hides the anchor, the nearest selected file it shows, above,
    /// else below, takes its place. When nothing is left, the rows decide, as
    /// for one file: the first file shown is selected.
    fn narrow_selection(&mut self) {
        let Some(Selected::File { group, index }) = self.selected.clone() else {
            return;
        };
        if self.also.is_empty() {
            return;
        }
        let files = &self.order.groups[group].files;
        let lower = self.lower_filter.as_str();
        self.also.retain(|&other| files[other].matches(lower));
        if !files[index].matches(lower) {
            let taking_over = self
                .nearest_above_else_below(group, self.selected_row, &self.also)
                .or_else(|| self.also.iter().copied().min());
            if let Some(next) = taking_over {
                self.also.remove(&next);
                self.selected = Some(Selected::File { group, index: next });
            }
        }
        if self.lead.is_some_and(|lead| !self.also.contains(&lead)) {
            self.lead = None;
        }
    }

    pub fn collapsed(&self) -> &Collapsed {
        &self.collapsed
    }

    /// Collapses or expands folder `folder` of `group`. A selection inside
    /// a folder that collapses moves to that folder.
    pub fn toggle(&mut self, group: usize, folder: usize) {
        let path = self.order.folder_path(group, folder).clone();
        let key = (group, path);
        let set = match self.lower_filter.is_empty() {
            true => &mut self.collapsed.plain,
            false => &mut self.collapsed.filtered,
        };
        if !set.remove(&key) {
            set.insert(key.clone());
            let (group, path) = key;
            if self
                .selected
                .as_ref()
                .is_some_and(|selected| self.is_inside(selected, group, &path))
            {
                self.selected = Some(Selected::Folder { group, path });
                self.reduce_to_anchor();
            }
        }
        self.build();
    }

    /// Whether `selected` lies inside the folder `path` of `group`.
    fn is_inside(&self, selected: &Selected, group: usize, path: &RepoPath) -> bool {
        let below = |inner: &RepoPath| {
            inner
                .as_bytes()
                .strip_prefix(path.as_bytes())
                .is_some_and(|rest| rest.first() == Some(&b'/'))
        };
        match selected {
            Selected::File { group: g, index } => {
                *g == group && below(self.order.file_path(*g, *index))
            }
            Selected::Folder {
                group: g,
                path: inner,
            } => *g == group && below(inner),
        }
    }

    pub fn row_of_file(&self, group: usize, index: usize) -> Option<usize> {
        let row = *self.file_rows.get(group)?.get(index)?;
        (row != NONE).then_some(row as usize)
    }

    pub fn row_of_folder(&self, group: usize, path: &RepoPath) -> Option<usize> {
        let folder = *self.order.groups.get(group)?.folder_by_path.get(path)?;
        let row = *self.folder_rows.get(group)?.get(folder)?;
        (row != NONE).then_some(row as usize)
    }

    fn row_of(&self, selected: &Selected) -> Option<usize> {
        match selected {
            Selected::File { group, index } => self.row_of_file(*group, *index),
            Selected::Folder { group, path } => self.row_of_folder(*group, path),
        }
    }

    /// The row of the first file shown.
    pub fn first_file(&self) -> Option<usize> {
        self.first_file
    }

    /// The path of the file or folder at `row`, for copying.
    pub fn path(&self, row: usize) -> Option<&RepoPath> {
        match *self.rows.get(row)? {
            Row::Title(_) => None,
            Row::Folder { group, folder, .. } => Some(self.order.folder_path(group, folder)),
            Row::File { group, index, .. } => Some(self.order.file_path(group, index)),
        }
    }

    /// What is selected, if it is shown.
    pub fn selected(&self) -> Option<&Selected> {
        self.selected_row.and(self.selected.as_ref())
    }

    /// The row of the selection, if it is shown.
    pub fn selected_row(&self) -> Option<usize> {
        self.selected_row
    }

    /// The file selected, if a file is and it is shown.
    pub fn selected_file(&self) -> Option<(usize, usize)> {
        match self.selected()? {
            Selected::File { group, index } => Some((*group, *index)),
            Selected::Folder { .. } => None,
        }
    }

    /// The number of files selected: none while a folder is selected, or
    /// while the filter hides the anchor.
    pub fn selection_len(&self) -> usize {
        self.selected_file().map_or(0, |_| 1 + self.also.len())
    }

    /// Whether the file at `index` of `group` is selected, the anchor or one
    /// of the others.
    pub fn is_file_selected(&self, group: usize, index: usize) -> bool {
        match self.selected() {
            Some(Selected::File {
                group: of,
                index: anchor,
            }) => *of == group && (*anchor == index || self.also.contains(&index)),
            _ => false,
        }
    }

    /// The files selected, each as its group and index, in the order of the
    /// list: also those a collapsed folder hides, at their place in it.
    pub fn selected_files(&self) -> Vec<(usize, usize)> {
        let Some((group, anchor)) = self.selected_file() else {
            return Vec::new();
        };
        let position = self.list_positions(group);
        let mut files: Vec<usize> = self.also.iter().copied().chain([anchor]).collect();
        files.sort_by_key(|&file| position[file]);
        files.into_iter().map(|file| (group, file)).collect()
    }

    /// The place of each file of `group` in the list, whether it is shown
    /// or not: its index in the flat list, its place among the files of the
    /// tree in the order the tree lists them.
    fn list_positions(&self, group: usize) -> Vec<u32> {
        let prepared = &self.order.groups[group];
        let mut position: Vec<u32> = (0..prepared.files.len() as u32).collect();
        if self.mode == Mode::Tree {
            let mut next = 0;
            for node in &prepared.nodes {
                if let NodeKind::File(file) = node.kind {
                    position[file as usize] = next;
                    next += 1;
                }
            }
        }
        position
    }

    /// The row the keys move from: the lead while several files are
    /// selected and it is shown, else the row of the anchor. The list keeps
    /// its cursor here, and not on the anchor, so that the arrow keys go on
    /// from the file a Shift click ended at.
    pub fn cursor_row(&self) -> Option<usize> {
        let anchor = self.selected_row?;
        let lead = match (self.selected(), self.lead) {
            (Some(Selected::File { group, .. }), Some(lead)) => self.row_of_file(*group, lead),
            _ => None,
        };
        Some(lead.unwrap_or(anchor))
    }

    /// A click with Ctrl, or Cmd, on the file at `row`: adds it to the
    /// selection and makes it the anchor, or removes it, and when it is the
    /// anchor, the nearest selected file above it, else below, becomes the
    /// anchor. Does nothing, and answers `false`, for a folder and for the
    /// only file selected. A file of another group starts a selection of its
    /// own.
    pub fn toggle_row(&mut self, row: usize) -> bool {
        let Some(&Row::File { group, index, .. }) = self.rows.get(row) else {
            return false;
        };
        let Some((_, anchor)) = self.selected_file().filter(|&(of, _)| of == group) else {
            self.select_row(row);
            return true;
        };
        if index == anchor {
            let Some(next) = self.nearest_above_else_below(group, self.selected_row, &self.also)
            else {
                return false;
            };
            self.also.remove(&next);
            self.selected = Some(Selected::File { group, index: next });
            self.selected_row = self.row_of_file(group, next);
            self.lead = None;
            return true;
        }
        if self.also.remove(&index) {
            if self.lead == Some(index) {
                self.lead = None;
            }
            return true;
        }
        self.also.insert(anchor);
        self.selected = Some(Selected::File { group, index });
        self.selected_row = Some(row);
        self.lead = None;
        true
    }

    /// A click with Shift on the file at `row`: selects the files shown
    /// from the anchor to it, and makes it the lead. The anchor stays. Does
    /// nothing, and answers `false`, for a folder; a file of another group
    /// starts a selection of its own.
    pub fn select_range_to(&mut self, row: usize) -> bool {
        let Some(&Row::File { group, index, .. }) = self.rows.get(row) else {
            return false;
        };
        let Some((_, anchor)) = self.selected_file().filter(|&(of, _)| of == group) else {
            self.select_row(row);
            return true;
        };
        self.select_range(group, anchor, index);
        true
    }

    /// Shift+Down, or Shift+Up: moves the lead by one file of the anchor's
    /// group and selects the files from the anchor to it, which grows the
    /// selection or shrinks it. Rows of folders are passed over. Answers
    /// `false` at the edge of the group.
    pub fn extend_selection(&mut self, down: bool) -> bool {
        let Some((group, anchor)) = self.selected_file() else {
            return false;
        };
        let Some(from) = self.row_of_file(group, self.lead.unwrap_or(anchor)) else {
            return false;
        };
        let rows: Box<dyn Iterator<Item = usize>> = match down {
            true => Box::new(from + 1..self.rows.len()),
            false => Box::new((0..from).rev()),
        };
        let mut next = None;
        for at in rows {
            match self.rows[at] {
                Row::File {
                    group: of, index, ..
                } if of == group => {
                    next = Some(index);
                    break;
                }
                Row::Folder { group: of, .. } if of == group => {}
                _ => break,
            }
        }
        let Some(next) = next else {
            return false;
        };
        self.select_range(group, anchor, next);
        true
    }

    /// Ctrl+A, or Cmd+A: selects every file of the anchor's group that the
    /// filter shows, those in collapsed folders too, which is what "all"
    /// means for the buttons of a group. The anchor stays. Answers whether
    /// the selection changed; `false` while no file is selected.
    pub fn select_all_shown(&mut self) -> bool {
        let Some((group, anchor)) = self.selected_file() else {
            return false;
        };
        let all: HashSet<usize> = self
            .files_matching(group, &self.filter)
            .filter(|&index| index != anchor)
            .collect();
        let changed = all != self.also;
        self.also = all;
        self.lead = None;
        changed
    }

    /// Escape: leaves the anchor as the only file selected.
    pub fn reduce_to_anchor(&mut self) {
        self.also.clear();
        self.lead = None;
    }

    /// Selects the files shown from `anchor` to `end`, both of `group`.
    fn select_range(&mut self, group: usize, anchor: usize, end: usize) {
        let (Some(from), Some(to)) = (
            self.row_of_file(group, anchor),
            self.row_of_file(group, end),
        ) else {
            return;
        };
        self.also = self.rows[from.min(to)..=from.max(to)]
            .iter()
            .filter_map(|row| match *row {
                Row::File {
                    group: of, index, ..
                } if of == group && index != anchor => Some(index),
                _ => None,
            })
            .collect();
        self.lead = (end != anchor).then_some(end);
    }

    /// Of the files `candidates` of `group` that are shown, the nearest one
    /// above the row `from`, else the nearest one below it.
    fn nearest_above_else_below(
        &self,
        group: usize,
        from: Option<usize>,
        candidates: &HashSet<usize>,
    ) -> Option<usize> {
        let from = from?;
        let shown = candidates
            .iter()
            .filter_map(|&index| Some((self.row_of_file(group, index)?, index)));
        let above = shown
            .clone()
            .filter(|&(row, _)| row < from)
            .max_by_key(|&(row, _)| row);
        let below = shown
            .filter(|&(row, _)| row > from)
            .min_by_key(|&(row, _)| row);
        above.or(below).map(|(_, index)| index)
    }

    /// Selects the file or folder at `row`; a title selects nothing.
    pub fn select_row(&mut self, row: usize) {
        let selected = match self.rows.get(row) {
            Some(Row::File { group, index, .. }) => Selected::File {
                group: *group,
                index: *index,
            },
            Some(Row::Folder { group, folder, .. }) => Selected::Folder {
                group: *group,
                path: self.order.folder_path(*group, *folder).clone(),
            },
            Some(Row::Title(_)) | None => return,
        };
        self.selected = Some(selected);
        self.selected_row = Some(row);
        self.wants_first = false;
        self.reduce_to_anchor();
    }

    /// Selects the file at `index` of `group`; when it is not shown, the
    /// first file shown.
    pub fn select_file(&mut self, group: usize, index: usize) {
        self.selected = Some(Selected::File { group, index });
        self.wants_first = false;
        self.reduce_to_anchor();
        self.find_selection();
    }

    /// Selects the first file shown; while the filter shows none, the
    /// first one it shows again.
    pub fn select_first(&mut self) {
        self.selected = None;
        self.selected_row = None;
        self.reduce_to_anchor();
        match self.first_file {
            Some(row) => self.select_row(row),
            None => self.wants_first = true,
        }
    }

    /// Left at `row`: collapses an expanded folder, or selects the folder
    /// above a file or a collapsed folder. Returns whether anything
    /// changed.
    pub fn left(&mut self, row: usize) -> bool {
        if self.mode == Mode::Flat {
            return false;
        }
        match self.rows.get(row) {
            Some(&Row::Folder {
                group,
                folder,
                expanded: true,
                ..
            }) => {
                self.select_row(row);
                self.toggle(group, folder);
                true
            }
            Some(Row::Folder { .. } | Row::File { .. }) => match self.parents[row] {
                Some(parent) => {
                    self.select_row(parent as usize);
                    true
                }
                None => false,
            },
            Some(Row::Title(_)) | None => false,
        }
    }

    /// Right at `row`: expands a collapsed folder, or selects the first row
    /// inside an expanded one. Returns whether anything changed.
    pub fn right(&mut self, row: usize) -> bool {
        if self.mode == Mode::Flat {
            return false;
        }
        match self.rows.get(row) {
            Some(&Row::Folder {
                group,
                folder,
                expanded: false,
                ..
            }) => {
                self.select_row(row);
                self.toggle(group, folder);
                true
            }
            Some(Row::Folder { .. }) => {
                let inside = self.parents.get(row + 1) == Some(&Some(row as u32));
                if inside {
                    self.select_row(row + 1);
                }
                inside
            }
            _ => false,
        }
    }

    /// Builds the rows, and everything a frame asks about them, from the
    /// mode, the filter and the collapsed folders.
    fn build(&mut self) {
        self.builds += 1;
        let order = Arc::clone(&self.order);
        let filter = self.lower_filter.as_str();
        let filtering = !filter.is_empty();
        let collapsed = match filtering {
            true => &self.collapsed.filtered,
            false => &self.collapsed.plain,
        };
        self.rows.clear();
        self.parents.clear();
        self.file_rows.resize_with(order.groups.len(), Vec::new);
        self.folder_rows.resize_with(order.groups.len(), Vec::new);
        for (group, prepared) in order.groups.iter().enumerate() {
            let file_rows = &mut self.file_rows[group];
            file_rows.clear();
            file_rows.resize(prepared.files.len(), NONE);
            let folder_rows = &mut self.folder_rows[group];
            folder_rows.clear();
            folder_rows.resize(prepared.folders.len(), NONE);
            let matches = |file: usize| !filtering || prepared.files[file].matches(filter);

            match self.mode {
                Mode::Flat => {
                    let shown: Vec<usize> =
                        (0..prepared.files.len()).filter(|&f| matches(f)).collect();
                    if shown.is_empty() {
                        continue;
                    }
                    if order.titles {
                        self.rows.push(Row::Title(group));
                        self.parents.push(None);
                    }
                    for index in shown {
                        file_rows[index] = self.rows.len() as u32;
                        self.rows.push(Row::File {
                            group,
                            index,
                            depth: 0,
                        });
                        self.parents.push(None);
                    }
                }
                Mode::Tree => {
                    // First the matches inside each folder, from the
                    // innermost out; then the rows of what holds matches.
                    let mut counts = vec![0u32; prepared.folders.len()];
                    let mut total = 0u32;
                    for node in prepared.nodes.iter().rev() {
                        let count = match node.kind {
                            NodeKind::File(file) => u32::from(matches(file as usize)),
                            NodeKind::Folder(folder) => counts[folder as usize],
                        };
                        match node.parent {
                            Some(parent) => counts[parent as usize] += count,
                            None => total += count,
                        }
                    }
                    if total == 0 {
                        continue;
                    }
                    if order.titles {
                        self.rows.push(Row::Title(group));
                        self.parents.push(None);
                    }
                    let shut: Vec<bool> = {
                        let mut shut = vec![false; prepared.folders.len()];
                        for (g, path) in collapsed {
                            if *g == group
                                && let Some(&folder) = prepared.folder_by_path.get(path)
                            {
                                shut[folder] = true;
                            }
                        }
                        shut
                    };
                    // The open folders above the node, with where they end.
                    let mut open: Vec<(usize, u32)> = Vec::new();
                    let mut at = 0;
                    while at < prepared.nodes.len() {
                        while open.last().is_some_and(|&(end, _)| end <= at) {
                            open.pop();
                        }
                        let parent = open.last().map(|&(_, row)| row);
                        let node = prepared.nodes[at];
                        let row = self.rows.len() as u32;
                        match node.kind {
                            NodeKind::Folder(folder) => {
                                let folder = folder as usize;
                                let end = prepared.folders[folder].end;
                                if counts[folder] == 0 {
                                    at = end;
                                    continue;
                                }
                                let expanded = !shut[folder];
                                folder_rows[folder] = row;
                                self.rows.push(Row::Folder {
                                    group,
                                    folder,
                                    depth: node.depth as usize,
                                    expanded,
                                });
                                self.parents.push(parent);
                                if expanded {
                                    open.push((end, row));
                                    at += 1;
                                } else {
                                    at = end;
                                }
                            }
                            NodeKind::File(file) => {
                                let file = file as usize;
                                if matches(file) {
                                    file_rows[file] = row;
                                    self.rows.push(Row::File {
                                        group,
                                        index: file,
                                        depth: node.depth as usize,
                                    });
                                    self.parents.push(parent);
                                }
                                at += 1;
                            }
                        }
                    }
                }
            }
        }
        self.first_file = self
            .rows
            .iter()
            .position(|row| matches!(row, Row::File { .. }));
        self.find_selection();
    }

    /// Finds the selection in the rows just built; when they hide it, the
    /// first file shown takes its place, and when there is none, it waits
    /// for the files to show again, as the first file waited for does.
    fn find_selection(&mut self) {
        self.selected_row = self
            .selected
            .as_ref()
            .and_then(|selected| self.row_of(selected));
        if self.holds_selection()
            && self.selected_row.is_none()
            && let Some(row) = self.first_file
        {
            self.select_row(row);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One group of files without old paths.
    fn order(paths: &[&str]) -> Arc<FileOrder> {
        let paths: Vec<RepoPath> = paths.iter().map(|path| RepoPath::from(*path)).collect();
        Arc::new(FileOrder::new(
            [paths.iter().map(|path| (path, None))],
            false,
        ))
    }

    /// Groups of files, under titles.
    fn groups(groups: &[&[&str]]) -> Arc<FileOrder> {
        let groups: Vec<Vec<RepoPath>> = groups
            .iter()
            .map(|paths| paths.iter().map(|path| RepoPath::from(*path)).collect())
            .collect();
        Arc::new(FileOrder::new(
            groups
                .iter()
                .map(|paths| paths.iter().map(|path| (path, None))),
            true,
        ))
    }

    fn tree(paths: &[&str]) -> FileTree {
        let mut tree = FileTree::new(order(paths));
        tree.set_mode(Mode::Tree);
        tree
    }

    /// The rows as text: `# 0` for a title, a folder with `/` and `+` while
    /// collapsed, a file by its name in the tree or its path in the flat
    /// list, each indented by two spaces per level.
    fn shown(tree: &FileTree) -> Vec<String> {
        let order = tree.order();
        tree.rows()
            .iter()
            .map(|row| match *row {
                Row::Title(group) => format!("# {group}"),
                Row::Folder {
                    group,
                    folder,
                    depth,
                    expanded,
                } => format!(
                    "{}{}/{}",
                    "  ".repeat(depth),
                    order.folder_name(group, folder),
                    if expanded { "" } else { "+" }
                ),
                Row::File {
                    group,
                    index,
                    depth,
                } => match tree.mode() {
                    Mode::Flat => order.file_path(group, index).to_string(),
                    Mode::Tree => {
                        format!("{}{}", "  ".repeat(depth), order.file_name(group, index))
                    }
                },
            })
            .collect()
    }

    fn row_named(tree: &FileTree, text: &str) -> usize {
        shown(tree)
            .iter()
            .position(|shown| shown.trim_start() == text)
            .unwrap_or_else(|| panic!("no row {text:?} in {:?}", shown(tree)))
    }

    fn folder(tree: &FileTree, group: usize, path: &str) -> usize {
        let row = tree
            .row_of_folder(group, &RepoPath::from(path))
            .unwrap_or_else(|| panic!("no folder {path}"));
        match tree.rows()[row] {
            Row::Folder { folder, .. } => folder,
            other => panic!("{other:?} is no folder"),
        }
    }

    fn selected_text(tree: &FileTree) -> Option<String> {
        tree.selected_row()
            .map(|row| shown(tree)[row].trim_start().to_owned())
    }

    const APP: [&str; 3] = ["src/app/main.rs", "src/app/view.rs", "README.md"];

    #[test]
    fn the_flat_list_keeps_the_order_of_git() {
        let tree = FileTree::new(order(&["b.rs", "a/x.rs", "A.md"]));
        assert_eq!(shown(&tree), ["b.rs", "a/x.rs", "A.md"]);
    }

    #[test]
    fn the_tree_sorts_folders_before_files_each_by_name() {
        let tree = tree(&["README.md", "src/main.rs", "docs/a.md", "build.rs"]);
        assert_eq!(
            shown(&tree),
            [
                "docs/",
                "  a.md",
                "src/",
                "  main.rs",
                "build.rs",
                "README.md"
            ]
        );
    }

    #[test]
    fn a_folder_whose_only_content_is_a_folder_shares_its_row() {
        assert_eq!(
            shown(&tree(&APP)),
            ["src/app/", "  main.rs", "  view.rs", "README.md"]
        );
        assert_eq!(
            shown(&tree(&["a/b/c/d.rs", "a/b/e.rs"])),
            ["a/b/", "  c/", "    d.rs", "  e.rs"]
        );
    }

    #[test]
    fn groups_show_under_titles_and_empty_groups_are_left_out() {
        let order = groups(&[&["s.rs"], &[], &["u.txt"]]);
        let mut tree = FileTree::new(order);
        assert_eq!(shown(&tree), ["# 0", "s.rs", "# 2", "u.txt"]);
        tree.set_mode(Mode::Tree);
        assert_eq!(shown(&tree), ["# 0", "s.rs", "# 2", "u.txt"]);
    }

    #[test]
    fn the_same_folder_in_two_groups_is_two_folders() {
        let mut tree = FileTree::new(groups(&[&["src/a.rs"], &["src/b.rs"]]));
        tree.set_mode(Mode::Tree);
        let staged = folder(&tree, 0, "src");
        tree.toggle(0, staged);
        assert_eq!(shown(&tree), ["# 0", "src/+", "# 1", "src/", "  b.rs"]);
    }

    #[test]
    fn reading_the_rows_builds_nothing() {
        let tree = tree(&APP);
        let builds = tree.builds();
        let _ = tree.rows();
        let _ = tree.row_of_file(0, 1);
        let _ = tree.row_of_folder(0, &RepoPath::from("src/app"));
        let _ = tree.first_file();
        let _ = tree.path(0);
        let _ = tree.selected_row();
        assert_eq!(tree.builds(), builds);
    }

    #[test]
    fn a_renamed_file_shows_where_it_came_from() {
        let paths = [
            (
                RepoPath::from("src/new.rs"),
                Some(RepoPath::from("src/old.rs")),
            ),
            (
                RepoPath::from("lib/moved.rs"),
                Some(RepoPath::from("src/moved.rs")),
            ),
        ];
        let order = FileOrder::new(
            [paths.iter().map(|(path, old)| (path, old.as_ref()))],
            false,
        );
        assert_eq!(order.file_name(0, 0), "new.rs");
        assert_eq!(order.came_from(0, 0).as_deref(), Some("old.rs"));
        assert_eq!(order.came_from(0, 1).as_deref(), Some("src/moved.rs"));
    }

    #[test]
    fn the_filter_narrows_both_modes_regardless_of_case() {
        let mut tree = FileTree::new(order(&APP));
        tree.set_filter("VIEW");
        assert_eq!(shown(&tree), ["src/app/view.rs"]);
        tree.set_mode(Mode::Tree);
        assert_eq!(shown(&tree), ["src/app/", "  view.rs"]);
    }

    #[test]
    fn the_filter_finds_a_renamed_file_by_its_old_path() {
        let paths = [
            (RepoPath::from("new.rs"), Some(RepoPath::from("old.rs"))),
            (RepoPath::from("b.rs"), None),
        ];
        let order = FileOrder::new(
            [paths.iter().map(|(path, old)| (path, old.as_ref()))],
            false,
        );
        let mut tree = FileTree::new(Arc::new(order));
        tree.set_filter("old");
        assert_eq!(shown(&tree), ["new.rs"]);
    }

    #[test]
    fn a_filter_without_matches_shows_and_selects_nothing() {
        let mut tree = tree(&APP);
        tree.select_first();
        tree.set_filter("zzz");
        assert!(tree.rows().is_empty());
        assert_eq!(tree.first_file(), None);
        assert_eq!(tree.selected_row(), None);
        assert_eq!(tree.selected_file(), None);
    }

    #[test]
    fn groups_without_matches_lose_their_titles() {
        let mut tree = FileTree::new(groups(&[&["src/a.rs"], &["notes.txt"]]));
        tree.set_filter(".rs");
        assert_eq!(shown(&tree), ["# 0", "src/a.rs"]);
    }

    #[test]
    fn a_click_collapses_and_expands_a_folder() {
        let mut tree = tree(&APP);
        let app = folder(&tree, 0, "src/app");
        tree.toggle(0, app);
        assert_eq!(shown(&tree), ["src/app/+", "README.md"]);
        tree.toggle(0, app);
        assert_eq!(
            shown(&tree),
            ["src/app/", "  main.rs", "  view.rs", "README.md"]
        );
    }

    const DOCS: [&str; 4] = [
        "docs/guide.md",
        "src/app/main.rs",
        "src/app/view.rs",
        "README.md",
    ];

    #[test]
    fn folders_collapse_while_filtering_until_the_filter_changes() {
        let mut tree = tree(&DOCS);
        tree.toggle(0, folder(&tree, 0, "docs"));
        tree.set_filter(".r");
        assert_eq!(shown(&tree), ["src/app/", "  main.rs", "  view.rs"]);
        tree.toggle(0, folder(&tree, 0, "src/app"));
        assert_eq!(shown(&tree), ["src/app/+"]);
        tree.set_filter(".rs");
        assert_eq!(shown(&tree), ["src/app/", "  main.rs", "  view.rs"]);
    }

    #[test]
    fn clearing_the_filter_brings_back_the_folders_collapsed_before() {
        let mut tree = tree(&DOCS);
        tree.toggle(0, folder(&tree, 0, "docs"));
        tree.set_filter(".r");
        tree.toggle(0, folder(&tree, 0, "src/app"));
        tree.set_filter("");
        assert_eq!(
            shown(&tree),
            ["docs/+", "src/app/", "  main.rs", "  view.rs", "README.md"]
        );
    }

    #[test]
    fn a_filter_shows_the_matches_inside_folders_collapsed_before() {
        let mut tree = tree(&DOCS);
        tree.toggle(0, folder(&tree, 0, "docs"));
        tree.set_filter("guide");
        assert_eq!(shown(&tree), ["docs/", "  guide.md"]);
    }

    #[test]
    fn collapsed_folders_carry_into_the_tree_of_a_list_read_again() {
        let mut tree = tree(&DOCS);
        tree.toggle(0, folder(&tree, 0, "docs"));
        tree.toggle(0, folder(&tree, 0, "src/app"));
        let renewed = tree.renewed(order(&["src/app/main.rs", "src/app/new.rs", "README.md"]));
        assert_eq!(renewed.mode(), Mode::Tree);
        assert_eq!(shown(&renewed), ["src/app/+", "README.md"]);
        assert_eq!(
            renewed.collapsed().plain,
            HashSet::from([(0, RepoPath::from("src/app"))])
        );
    }

    #[test]
    fn a_selected_folder_carries_into_the_tree_of_a_list_read_again() {
        let mut tree = FileTree::new(groups(&[&["src/a.rs"], &["src/b.rs"]]));
        tree.set_mode(Mode::Tree);
        tree.set_filter("src");
        tree.select_row(tree.row_of_folder(1, &RepoPath::from("src")).unwrap());
        let renewed = tree.renewed(groups(&[&["src/a.rs"], &["src/b.rs", "c.rs"]]));
        assert_eq!(renewed.filter(), "src");
        assert_eq!(
            renewed.selected(),
            Some(&Selected::Folder {
                group: 1,
                path: RepoPath::from("src")
            })
        );
        assert_eq!(renewed.selected_row(), Some(4));
    }

    #[test]
    fn a_selected_file_carries_into_the_tree_of_a_list_read_again_by_its_path() {
        let mut tree = tree(&APP);
        tree.select_file(0, 1);
        let renewed = tree.renewed(order(&["a.txt", "src/app/main.rs", "src/app/view.rs"]));
        assert_eq!(renewed.selected_file(), Some((0, 2)));
    }

    #[test]
    fn a_selected_file_the_filter_hides_carries_into_a_list_read_again() {
        let mut tree = tree(&APP);
        tree.select_file(0, 1);
        tree.set_filter("zzz");
        let mut renewed = tree.renewed(order(&["a.txt", "src/app/main.rs", "src/app/view.rs"]));
        assert!(renewed.holds_selection());
        assert_eq!(renewed.selected_row(), None);
        renewed.set_filter("");
        assert_eq!(renewed.selected_file(), Some((0, 2)));
    }

    #[test]
    fn a_selected_file_gone_from_the_only_list_leaves_nothing_selected() {
        let mut tree = FileTree::new(order(&["a.rs"]));
        tree.select_file(0, 0);
        let renewed = tree.renewed(order(&[]));
        assert!(!renewed.holds_selection());
        assert_eq!(renewed.selected_row(), None);
    }

    /// Two groups, flat, with `selected` of the first group selected.
    fn two_groups(first: &[&str], second: &[&str], selected: &str) -> FileTree {
        let mut tree = FileTree::new(groups(&[first, second]));
        let row = row_named(&tree, selected);
        tree.select_row(row);
        tree
    }

    #[test]
    fn a_selected_file_that_left_its_group_gives_way_to_the_one_below() {
        let tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], "b.rs");
        // `b.rs` was staged.
        let renewed = tree.renewed(groups(&[&["a.rs", "c.rs"], &["b.rs", "s.rs"]]));
        assert_eq!(selected_text(&renewed).as_deref(), Some("c.rs"));
        assert_eq!(renewed.selected_file(), Some((0, 1)));
    }

    #[test]
    fn the_last_file_of_its_group_gives_way_to_the_one_above() {
        let tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], "c.rs");
        let renewed = tree.renewed(groups(&[&["a.rs", "b.rs"], &["c.rs", "s.rs"]]));
        assert_eq!(renewed.selected_file(), Some((0, 1)));
    }

    #[test]
    fn a_file_the_filter_hides_is_passed_over() {
        let mut tree = two_groups(
            &["src/a.rs", "src/b.txt", "src/c.rs"],
            &["s.rs"],
            "src/a.rs",
        );
        tree.set_mode(Mode::Tree);
        tree.set_filter(".rs");
        assert_eq!(selected_text(&tree).as_deref(), Some("a.rs"));
        let renewed = tree.renewed(groups(&[&["src/b.txt", "src/c.rs"], &["s.rs", "src/a.rs"]]));
        assert_eq!(selected_text(&renewed).as_deref(), Some("c.rs"));
    }

    #[test]
    fn a_file_in_a_collapsed_folder_is_passed_over() {
        let mut tree = two_groups(&["a.rs", "lib/b.rs", "z.rs"], &["s.rs"], "a.rs");
        tree.set_mode(Mode::Tree);
        let lib = folder(&tree, 0, "lib");
        tree.toggle(0, lib);
        let renewed = tree.renewed(groups(&[&["lib/b.rs", "z.rs"], &["a.rs", "s.rs"]]));
        assert_eq!(selected_text(&renewed).as_deref(), Some("z.rs"));
    }

    #[test]
    fn a_group_left_without_files_gives_way_to_the_first_file_of_the_other() {
        let tree = two_groups(&["a.rs"], &["s.rs", "t.rs"], "a.rs");
        let renewed = tree.renewed(groups(&[&[], &["a.rs", "s.rs", "t.rs"]]));
        assert_eq!(renewed.selected_file(), Some((1, 0)));

        // Also from the second group to the first.
        let mut tree = FileTree::new(groups(&[&["a.rs", "b.rs"], &["s.rs"]]));
        let row = row_named(&tree, "s.rs");
        tree.select_row(row);
        let renewed = tree.renewed(groups(&[&["a.rs", "b.rs", "s.rs"], &[]]));
        assert_eq!(renewed.selected_file(), Some((0, 0)));
    }

    #[test]
    fn a_selected_file_that_stays_in_its_group_stays_selected() {
        let tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], "b.rs");
        // Another file was staged.
        let renewed = tree.renewed(groups(&[&["b.rs", "c.rs"], &["a.rs", "s.rs"]]));
        assert_eq!(selected_text(&renewed).as_deref(), Some("b.rs"));
    }

    #[test]
    fn the_successor_of_the_selected_file_is_found_in_the_rows_shown() {
        let tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], "b.rs");
        assert_eq!(tree.successor_of_selected(), Some(row_named(&tree, "c.rs")));
        let tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], "c.rs");
        assert_eq!(tree.successor_of_selected(), Some(row_named(&tree, "b.rs")));
        let tree = two_groups(&["a.rs"], &["s.rs"], "a.rs");
        assert_eq!(tree.successor_of_selected(), Some(row_named(&tree, "s.rs")));
        // A folder has no successor, nor has the only file.
        let tree = FileTree::new(order(&["a.rs"]));
        assert_eq!(tree.successor_of_selected(), None);
    }

    #[test]
    fn the_first_file_waits_for_a_filter_that_shows_nothing() {
        let mut tree = FileTree::new(order(&APP));
        tree.set_filter("zzz");
        tree.select_first();
        assert!(tree.holds_selection());
        assert_eq!(tree.selected_row(), None);
        tree.set_filter("");
        assert_eq!(tree.selected_file(), Some((0, 0)));
    }

    #[test]
    fn the_first_file_waited_for_is_the_first_one_a_filter_shows() {
        let mut tree = FileTree::new(order(&APP));
        tree.set_filter("zzz");
        tree.select_first();
        tree.set_filter("view");
        assert_eq!(tree.selected_file(), Some((0, 1)));
    }

    #[test]
    fn the_first_file_waited_for_carries_into_a_list_read_again() {
        let mut tree = FileTree::new(order(&APP));
        tree.set_filter("zzz");
        tree.select_first();
        let mut renewed = tree.renewed(order(&APP));
        assert!(renewed.holds_selection());
        renewed.set_filter("");
        assert_eq!(renewed.selected_file(), Some((0, 0)));
    }

    #[test]
    fn a_tree_shown_with_its_mode_and_filter_builds_its_rows_once() {
        let tree = FileTree::shown_as(order(&APP), Mode::Tree, "view");
        assert_eq!(tree.builds(), 1);
        assert_eq!(shown(&tree), ["src/app/", "  view.rs"]);
    }

    #[test]
    fn a_list_read_again_builds_its_rows_once() {
        let mut tree = tree(&APP);
        tree.set_filter("view");
        let renewed = tree.renewed(order(&APP));
        assert_eq!(renewed.builds(), 1);
        assert_eq!(shown(&renewed), ["src/app/", "  view.rs"]);
    }

    #[test]
    fn rows_of_files_and_folders_follow_each_change() {
        let mut tree = tree(&APP);
        assert_eq!(tree.row_of_file(0, 1), Some(2));
        assert_eq!(tree.row_of_folder(0, &RepoPath::from("src/app")), Some(0));
        assert_eq!(tree.first_file(), Some(1));
        tree.toggle(0, folder(&tree, 0, "src/app"));
        assert_eq!(tree.row_of_file(0, 1), None);
        assert_eq!(tree.row_of_file(0, 2), Some(1));
        assert_eq!(tree.first_file(), Some(1));
        tree.set_mode(Mode::Flat);
        assert_eq!(tree.row_of_file(0, 1), Some(1));
        assert_eq!(tree.row_of_folder(0, &RepoPath::from("src/app")), None);
        assert_eq!(tree.first_file(), Some(0));
    }

    #[test]
    fn the_path_of_a_row_is_the_file_or_the_folder() {
        let tree = tree(&APP);
        assert_eq!(tree.path(0), Some(&RepoPath::from("src/app")));
        assert_eq!(tree.path(2), Some(&RepoPath::from("src/app/view.rs")));
        let titled = FileTree::new(groups(&[&["a.rs"]]));
        assert_eq!(titled.path(0), None);
    }

    #[test]
    fn the_selection_stays_on_its_file_while_filtering() {
        let mut tree = tree(&APP);
        tree.select_file(0, 1);
        tree.set_filter("view");
        assert_eq!(tree.selected_file(), Some((0, 1)));
        assert_eq!(selected_text(&tree).as_deref(), Some("view.rs"));
    }

    #[test]
    fn a_selected_file_the_filter_hides_gives_way_to_the_first_file() {
        let mut tree = tree(&APP);
        tree.select_file(0, 1);
        tree.set_filter("main");
        assert_eq!(tree.selected_file(), Some((0, 0)));
        tree.set_filter("");
        assert_eq!(tree.selected_file(), Some((0, 0)));
        assert_eq!(selected_text(&tree).as_deref(), Some("main.rs"));
    }

    #[test]
    fn the_selection_returns_when_the_filter_matches_again() {
        let mut tree = tree(&APP);
        tree.select_file(0, 1);
        tree.set_filter("viewx");
        assert_eq!(tree.selected_row(), None);
        tree.set_filter("view");
        assert_eq!(tree.selected_file(), Some((0, 1)));
    }

    #[test]
    fn collapsing_the_folder_of_the_selection_selects_the_folder() {
        let mut tree = tree(&APP);
        tree.select_file(0, 0);
        tree.toggle(0, folder(&tree, 0, "src/app"));
        assert_eq!(selected_text(&tree).as_deref(), Some("src/app/+"));
    }

    #[test]
    fn left_moves_from_a_file_to_its_folder_and_then_collapses_it() {
        let mut tree = tree(&APP);
        tree.select_file(0, 0);
        assert!(tree.left(tree.selected_row().unwrap()));
        assert_eq!(selected_text(&tree).as_deref(), Some("src/app/"));
        assert!(tree.left(tree.selected_row().unwrap()));
        assert_eq!(shown(&tree), ["src/app/+", "README.md"]);
        assert_eq!(selected_text(&tree).as_deref(), Some("src/app/+"));
    }

    #[test]
    fn right_expands_a_folder_and_then_moves_into_it() {
        let mut tree = tree(&APP);
        tree.toggle(0, folder(&tree, 0, "src/app"));
        tree.select_row(0);
        assert!(tree.right(0));
        assert_eq!(selected_text(&tree).as_deref(), Some("src/app/"));
        assert_eq!(shown(&tree).len(), 4);
        assert!(tree.right(0));
        assert_eq!(selected_text(&tree).as_deref(), Some("main.rs"));
    }

    #[test]
    fn left_and_right_do_nothing_where_there_is_nowhere_to_go() {
        let mut tree = tree(&APP);
        let readme = row_named(&tree, "README.md");
        tree.select_row(readme);
        assert!(!tree.left(readme));
        assert!(!tree.right(readme));
        tree.toggle(0, folder(&tree, 0, "src/app"));
        tree.select_row(0);
        assert!(!tree.left(0));
        assert_eq!(selected_text(&tree).as_deref(), Some("src/app/+"));
    }

    #[test]
    fn left_climbs_folder_by_folder_below_the_first_level() {
        let mut tree = tree(&["a/b/c/d.rs", "a/b/e.rs"]);
        tree.select_row(row_named(&tree, "d.rs"));
        tree.left(tree.selected_row().unwrap());
        assert_eq!(selected_text(&tree).as_deref(), Some("c/"));
        tree.left(tree.selected_row().unwrap());
        assert_eq!(selected_text(&tree).as_deref(), Some("c/+"));
        tree.left(tree.selected_row().unwrap());
        assert_eq!(selected_text(&tree).as_deref(), Some("a/b/"));
    }

    #[test]
    fn left_and_right_work_while_filtering() {
        let mut tree = tree(&DOCS);
        tree.set_filter(".rs");
        tree.select_row(row_named(&tree, "view.rs"));
        tree.left(tree.selected_row().unwrap());
        tree.left(tree.selected_row().unwrap());
        assert_eq!(shown(&tree), ["src/app/+"]);
        tree.right(0);
        assert_eq!(shown(&tree), ["src/app/", "  main.rs", "  view.rs"]);
    }

    #[test]
    fn left_and_right_do_nothing_in_the_flat_list() {
        let mut tree = FileTree::new(order(&APP));
        tree.select_row(1);
        assert!(!tree.left(1));
        assert!(!tree.right(1));
        assert_eq!(tree.selected_file(), Some((0, 1)));
    }

    // The selection of several files of one group (spec `file-lists`,
    // requirement "Selecting several files in the File status view").

    const FIVE: [&str; 5] = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"];

    /// The paths of the selected files, in the order of the list.
    fn selected_paths(tree: &FileTree) -> Vec<String> {
        tree.selected_files()
            .into_iter()
            .map(|(group, index)| tree.order().file_path(group, index).to_string())
            .collect()
    }

    /// A plain click on the row named `text`.
    fn click(tree: &mut FileTree, text: &str) {
        tree.select_row(row_named(tree, text));
    }

    /// A click with Ctrl, or Cmd, on the row named `text`.
    fn ctrl_click(tree: &mut FileTree, text: &str) -> bool {
        tree.toggle_row(row_named(tree, text))
    }

    /// A click with Shift on the row named `text`.
    fn shift_click(tree: &mut FileTree, text: &str) -> bool {
        tree.select_range_to(row_named(tree, text))
    }

    #[test]
    fn a_plain_click_selects_one_file_which_is_the_anchor_and_the_lead() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        assert_eq!(selected_paths(&tree), ["b.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 1)));
        assert_eq!(tree.cursor_row(), tree.selected_row());
    }

    #[test]
    fn a_ctrl_click_adds_a_file_and_makes_it_the_anchor_and_the_lead() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "a.rs");
        assert!(ctrl_click(&mut tree, "c.rs"));
        assert_eq!(selected_paths(&tree), ["a.rs", "c.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 2)));
        assert_eq!(tree.cursor_row(), Some(row_named(&tree, "c.rs")));
    }

    #[test]
    fn a_plain_click_ends_a_selection_of_several_files() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "a.rs");
        ctrl_click(&mut tree, "c.rs");
        click(&mut tree, "b.rs");
        assert_eq!(selected_paths(&tree), ["b.rs"]);
        assert_eq!(tree.cursor_row(), tree.selected_row());
    }

    #[test]
    fn a_ctrl_click_on_a_selected_file_removes_it() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "a.rs");
        ctrl_click(&mut tree, "c.rs");
        ctrl_click(&mut tree, "e.rs");
        // A file that is not the anchor leaves the anchor where it is.
        assert!(ctrl_click(&mut tree, "a.rs"));
        assert_eq!(selected_paths(&tree), ["c.rs", "e.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 4)));
    }

    #[test]
    fn removing_the_anchor_moves_it_to_the_nearest_selected_file_above() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "a.rs");
        ctrl_click(&mut tree, "c.rs");
        ctrl_click(&mut tree, "e.rs");
        // `e.rs` is the anchor, `c.rs` the nearest selected file above it.
        assert!(ctrl_click(&mut tree, "e.rs"));
        assert_eq!(selected_paths(&tree), ["a.rs", "c.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 2)));
        assert_eq!(tree.cursor_row(), Some(row_named(&tree, "c.rs")));
    }

    #[test]
    fn removing_the_anchor_with_nothing_above_moves_it_below() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "c.rs");
        ctrl_click(&mut tree, "a.rs");
        assert!(ctrl_click(&mut tree, "a.rs"));
        assert_eq!(selected_paths(&tree), ["c.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 2)));
    }

    #[test]
    fn a_ctrl_click_on_the_only_selected_file_does_nothing() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        assert!(!ctrl_click(&mut tree, "b.rs"));
        assert_eq!(selected_paths(&tree), ["b.rs"]);
    }

    #[test]
    fn a_shift_click_selects_the_range_and_keeps_the_anchor() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        assert!(shift_click(&mut tree, "d.rs"));
        assert_eq!(selected_paths(&tree), ["b.rs", "c.rs", "d.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 1)));
        // The lead is the file clicked, not the anchor.
        assert_eq!(tree.cursor_row(), Some(row_named(&tree, "d.rs")));
    }

    #[test]
    fn a_shift_click_above_the_anchor_selects_the_range_upwards() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "d.rs");
        shift_click(&mut tree, "b.rs");
        assert_eq!(selected_paths(&tree), ["b.rs", "c.rs", "d.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 3)));
        assert_eq!(tree.cursor_row(), Some(row_named(&tree, "b.rs")));
    }

    #[test]
    fn a_shift_click_replaces_what_a_ctrl_click_added() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        ctrl_click(&mut tree, "d.rs");
        shift_click(&mut tree, "a.rs");
        // The range runs from the anchor `d.rs`; `b.rs` lies inside it.
        assert_eq!(selected_paths(&tree), ["a.rs", "b.rs", "c.rs", "d.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 3)));
    }

    #[test]
    fn a_range_in_the_tree_holds_files_only() {
        let mut tree = tree(&["docs/c.md", "src/a.rs", "src/b.rs"]);
        click(&mut tree, "c.md");
        assert!(shift_click(&mut tree, "b.rs"));
        assert_eq!(selected_paths(&tree), ["docs/c.md", "src/a.rs", "src/b.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 0)));
    }

    #[test]
    fn a_range_leaves_out_the_files_of_a_collapsed_folder() {
        let mut tree = tree(&["app/a.rs", "src/b.rs", "web/z.rs"]);
        let src = folder(&tree, 0, "src");
        tree.toggle(0, src);
        click(&mut tree, "a.rs");
        assert!(shift_click(&mut tree, "z.rs"));
        assert_eq!(selected_paths(&tree), ["app/a.rs", "web/z.rs"]);
    }

    #[test]
    fn shift_and_ctrl_clicks_on_a_folder_change_nothing() {
        let mut tree = tree(&["app/a.rs", "app/b.rs", "web/z.rs"]);
        click(&mut tree, "a.rs");
        let web = tree.row_of_folder(0, &RepoPath::from("web")).unwrap();
        assert!(!tree.toggle_row(web));
        assert!(!tree.select_range_to(web));
        assert_eq!(selected_paths(&tree), ["app/a.rs"]);
    }

    #[test]
    fn a_selected_folder_holds_no_selected_files() {
        let mut tree = tree(&["app/a.rs", "app/b.rs", "web/z.rs"]);
        click(&mut tree, "a.rs");
        ctrl_click(&mut tree, "b.rs");
        let web = tree.row_of_folder(0, &RepoPath::from("web")).unwrap();
        tree.select_row(web);
        assert!(selected_paths(&tree).is_empty());
        assert_eq!(tree.selected_file(), None);
        assert_eq!(tree.cursor_row(), tree.selected_row());
        // A click with Ctrl on a file starts a selection of its own.
        assert!(ctrl_click(&mut tree, "z.rs"));
        assert_eq!(selected_paths(&tree), ["web/z.rs"]);
    }

    #[test]
    fn a_click_with_a_modifier_in_the_other_group_starts_a_new_selection() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs", "t.rs"], "a.rs");
        ctrl_click(&mut tree, "b.rs");
        assert!(ctrl_click(&mut tree, "t.rs"));
        assert_eq!(selected_paths(&tree), ["t.rs"]);
        assert_eq!(tree.selected_file(), Some((1, 1)));
        // The same for a range.
        click(&mut tree, "a.rs");
        assert!(shift_click(&mut tree, "t.rs"));
        assert_eq!(selected_paths(&tree), ["t.rs"]);
    }

    #[test]
    fn the_selected_files_are_in_the_order_of_the_list() {
        let mut tree = FileTree::new(order(&["b.rs", "a/x.rs", "A.md"]));
        click(&mut tree, "A.md");
        ctrl_click(&mut tree, "b.rs");
        ctrl_click(&mut tree, "a/x.rs");
        assert_eq!(selected_paths(&tree), ["b.rs", "a/x.rs", "A.md"]);
    }

    #[test]
    fn shift_down_and_up_extend_and_shrink_from_the_anchor() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        assert!(tree.extend_selection(true));
        assert!(tree.extend_selection(true));
        assert_eq!(selected_paths(&tree), ["b.rs", "c.rs", "d.rs"]);
        assert_eq!(tree.cursor_row(), Some(row_named(&tree, "d.rs")));
        assert!(tree.extend_selection(false));
        assert_eq!(selected_paths(&tree), ["b.rs", "c.rs"]);
        // Past the anchor the selection grows upwards.
        tree.extend_selection(false);
        tree.extend_selection(false);
        assert_eq!(selected_paths(&tree), ["a.rs", "b.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 1)));
        assert_eq!(tree.cursor_row(), Some(row_named(&tree, "a.rs")));
    }

    #[test]
    fn shift_down_goes_on_from_the_file_a_shift_click_ended_at() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        shift_click(&mut tree, "d.rs");
        assert!(tree.extend_selection(true));
        assert_eq!(selected_paths(&tree), ["b.rs", "c.rs", "d.rs", "e.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 1)));
    }

    #[test]
    fn extending_stops_at_the_edge_of_the_group() {
        let mut tree = two_groups(&["a.rs", "b.rs"], &["s.rs"], "b.rs");
        assert!(!tree.extend_selection(true));
        assert_eq!(selected_paths(&tree), ["b.rs"]);
        let mut tree = two_groups(&["a.rs"], &["s.rs", "t.rs"], "s.rs");
        assert!(!tree.extend_selection(false));
        assert_eq!(selected_paths(&tree), ["s.rs"]);
    }

    #[test]
    fn extending_passes_over_the_row_of_a_folder() {
        let mut tree = tree(&["app/a.rs", "web/z.rs"]);
        click(&mut tree, "a.rs");
        assert!(tree.extend_selection(true));
        assert_eq!(selected_paths(&tree), ["app/a.rs", "web/z.rs"]);
    }

    #[test]
    fn select_all_takes_the_files_the_filter_shows() {
        let mut tree = tree(&["src/a.rs", "src/b.rs", "notes.txt"]);
        tree.set_filter(".rs");
        click(&mut tree, "a.rs");
        assert!(tree.select_all_shown());
        assert_eq!(selected_paths(&tree), ["src/a.rs", "src/b.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 0)));
    }

    #[test]
    fn select_all_takes_the_files_of_collapsed_folders_too() {
        let mut tree = tree(&["lib/a.rs", "z.rs"]);
        let lib = folder(&tree, 0, "lib");
        tree.toggle(0, lib);
        click(&mut tree, "z.rs");
        assert!(tree.select_all_shown());
        assert_eq!(selected_paths(&tree), ["lib/a.rs", "z.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 1)));
    }

    #[test]
    fn select_all_stays_in_the_group_of_the_anchor() {
        let mut tree = two_groups(&["a.rs", "b.rs"], &["s.rs", "t.rs"], "s.rs");
        tree.select_row(row_named(&tree, "t.rs"));
        assert!(tree.select_all_shown());
        assert_eq!(selected_paths(&tree), ["s.rs", "t.rs"]);
    }

    #[test]
    fn select_all_without_a_selection_selects_nothing() {
        let mut tree = FileTree::new(order(&FIVE));
        assert!(!tree.select_all_shown());
        assert!(selected_paths(&tree).is_empty());
    }

    #[test]
    fn reducing_to_the_anchor_leaves_one_file() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        shift_click(&mut tree, "d.rs");
        tree.reduce_to_anchor();
        assert_eq!(selected_paths(&tree), ["b.rs"]);
        assert_eq!(tree.cursor_row(), tree.selected_row());
        // With one file selected it changes nothing.
        tree.reduce_to_anchor();
        assert_eq!(selected_paths(&tree), ["b.rs"]);
    }

    #[test]
    fn the_cursor_is_the_row_of_the_anchor_without_a_selection_of_several_files() {
        let mut tree = FileTree::new(order(&FIVE));
        assert_eq!(tree.cursor_row(), None);
        click(&mut tree, "c.rs");
        assert_eq!(tree.cursor_row(), tree.selected_row());
    }

    #[test]
    fn the_selection_counts_its_files_and_knows_them() {
        let mut tree = FileTree::new(order(&FIVE));
        assert_eq!(tree.selection_len(), 0);
        click(&mut tree, "a.rs");
        assert_eq!(tree.selection_len(), 1);
        shift_click(&mut tree, "c.rs");
        assert_eq!(tree.selection_len(), 3);
        assert!(tree.is_file_selected(0, 1));
        assert!(!tree.is_file_selected(0, 3));
    }

    #[test]
    fn a_selection_of_several_files_carries_into_a_list_read_again_by_their_paths() {
        let mut tree = FileTree::new(order(&FIVE));
        click(&mut tree, "b.rs");
        shift_click(&mut tree, "d.rs");
        let renewed = tree.renewed(order(&["x.rs", "a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]));
        assert_eq!(selected_paths(&renewed), ["b.rs", "c.rs", "d.rs"]);
        // The anchor and the lead carry over too.
        assert_eq!(renewed.selected_file(), Some((0, 2)));
        assert_eq!(renewed.cursor_row(), Some(row_named(&renewed, "d.rs")));
    }

    #[test]
    fn a_selected_file_that_left_its_group_drops_out_of_the_selection() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], "a.rs");
        shift_click(&mut tree, "c.rs");
        // `b.rs` was staged.
        let renewed = tree.renewed(groups(&[&["a.rs", "c.rs"], &["b.rs", "s.rs"]]));
        assert_eq!(selected_paths(&renewed), ["a.rs", "c.rs"]);
        assert_eq!(renewed.selected_file(), Some((0, 0)));
    }

    #[test]
    fn the_anchor_leaving_gives_way_to_the_nearest_selected_file_above() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs", "d.rs"], &["s.rs"], "a.rs");
        ctrl_click(&mut tree, "b.rs");
        ctrl_click(&mut tree, "d.rs");
        // The anchor `d.rs` was staged.
        let renewed = tree.renewed(groups(&[&["a.rs", "b.rs", "c.rs"], &["d.rs", "s.rs"]]));
        assert_eq!(selected_paths(&renewed), ["a.rs", "b.rs"]);
        assert_eq!(renewed.selected_file(), Some((0, 1)));
    }

    #[test]
    fn the_anchor_leaving_gives_way_to_a_selected_file_below_when_none_is_above() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], "c.rs");
        // `b.rs` is the anchor, and `c.rs`, below it, the only other file.
        ctrl_click(&mut tree, "b.rs");
        assert_eq!(tree.selected_file(), Some((0, 1)));
        // The anchor `b.rs` was staged.
        let renewed = tree.renewed(groups(&[&["a.rs", "c.rs"], &["b.rs", "s.rs"]]));
        assert_eq!(selected_paths(&renewed), ["c.rs"]);
        assert_eq!(renewed.selected_file(), Some((0, 1)));
    }

    #[test]
    fn a_selection_that_left_its_group_gives_way_to_the_file_below() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs", "d.rs"], &["s.rs"], "b.rs");
        shift_click(&mut tree, "c.rs");
        // `b.rs` and `c.rs` were staged.
        let renewed = tree.renewed(groups(&[&["a.rs", "d.rs"], &["b.rs", "c.rs", "s.rs"]]));
        assert_eq!(selected_paths(&renewed), ["d.rs"]);
    }

    #[test]
    fn the_filter_removes_the_files_it_hides_from_the_selection() {
        let mut tree = FileTree::new(order(&["src/a.rs", "notes.txt", "src/b.rs"]));
        click(&mut tree, "notes.txt");
        ctrl_click(&mut tree, "src/a.rs");
        tree.set_filter(".rs");
        assert_eq!(selected_paths(&tree), ["src/a.rs"]);
        // Emptying the filter does not bring them back.
        tree.set_filter("");
        assert_eq!(selected_paths(&tree), ["src/a.rs"]);
    }

    #[test]
    fn the_filter_hiding_the_anchor_gives_way_to_a_selected_file_it_shows() {
        let mut tree = FileTree::new(order(&["src/a.rs", "notes.txt", "src/b.rs"]));
        click(&mut tree, "src/a.rs");
        ctrl_click(&mut tree, "notes.txt");
        // The anchor `notes.txt` is hidden by the filter.
        tree.set_filter(".rs");
        assert_eq!(selected_paths(&tree), ["src/a.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 0)));
    }

    #[test]
    fn the_filter_hiding_the_whole_selection_selects_the_first_file_it_shows() {
        let mut tree = FileTree::new(order(&["src/a.rs", "notes.txt", "todo.txt"]));
        click(&mut tree, "notes.txt");
        ctrl_click(&mut tree, "todo.txt");
        tree.set_filter(".rs");
        assert_eq!(selected_paths(&tree), ["src/a.rs"]);
        tree.set_filter("");
        assert_eq!(selected_paths(&tree), ["src/a.rs"]);
    }

    #[test]
    fn a_filter_that_keeps_every_selected_file_keeps_the_selection() {
        let mut tree = FileTree::new(order(&["src/a.rs", "src/b.rs", "notes.txt"]));
        click(&mut tree, "src/a.rs");
        ctrl_click(&mut tree, "src/b.rs");
        tree.set_filter("src");
        assert_eq!(selected_paths(&tree), ["src/a.rs", "src/b.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 1)));
    }

    #[test]
    fn collapsing_a_folder_keeps_the_selected_files_inside_it() {
        let mut tree = tree(&["app/a.rs", "app/b.rs", "web/z.rs"]);
        click(&mut tree, "a.rs");
        ctrl_click(&mut tree, "z.rs");
        let app = folder(&tree, 0, "app");
        tree.toggle(0, app);
        // `app/a.rs` is hidden now and stays selected, at its place in the list.
        assert_eq!(selected_paths(&tree), ["app/a.rs", "web/z.rs"]);
        assert_eq!(tree.selected_file(), Some((0, 2)));
    }

    #[test]
    fn collapsing_the_folder_of_the_anchor_selects_the_folder_and_ends_the_selection() {
        let mut tree = tree(&["app/a.rs", "app/b.rs", "web/z.rs"]);
        click(&mut tree, "z.rs");
        ctrl_click(&mut tree, "a.rs");
        let app = folder(&tree, 0, "app");
        tree.toggle(0, app);
        assert!(selected_paths(&tree).is_empty());
        assert_eq!(tree.selected_row(), Some(row_named(&tree, "app/+")));
        // Expanding it again does not bring the selection back.
        tree.toggle(0, app);
        assert!(selected_paths(&tree).is_empty());
    }

    /// The successor of a tree whose selected files all leave their group.
    fn successor_of_the_selection(tree: &FileTree) -> Option<usize> {
        let (group, _) = tree.selected_file()?;
        tree.successor_of_selection(|index| tree.is_file_selected(group, index))
    }

    #[test]
    fn the_successor_of_a_selection_is_the_file_below_its_lowest_row() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"], &["s.rs"], "b.rs");
        shift_click(&mut tree, "c.rs");
        assert_eq!(
            successor_of_the_selection(&tree),
            Some(row_named(&tree, "d.rs"))
        );
    }

    #[test]
    fn a_file_between_two_selected_files_that_stays_is_the_successor() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"], &["s.rs"], "b.rs");
        ctrl_click(&mut tree, "d.rs");
        assert_eq!(
            successor_of_the_selection(&tree),
            Some(row_named(&tree, "c.rs"))
        );
    }

    #[test]
    fn the_successor_of_the_last_files_of_a_group_is_the_file_above_the_highest_row() {
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"], &["s.rs"], "d.rs");
        shift_click(&mut tree, "e.rs");
        assert_eq!(
            successor_of_the_selection(&tree),
            Some(row_named(&tree, "c.rs"))
        );
    }

    #[test]
    fn the_successor_of_every_file_of_a_group_is_the_first_file_of_the_other() {
        let mut tree = two_groups(&["a.rs", "b.rs"], &["s.rs", "t.rs"], "a.rs");
        shift_click(&mut tree, "b.rs");
        assert_eq!(
            successor_of_the_selection(&tree),
            Some(row_named(&tree, "s.rs"))
        );
    }

    #[test]
    fn the_successor_is_found_in_the_rows_a_filter_leaves() {
        let mut tree = FileTree::new(groups(&[
            &["src/a.rs", "src/b.rs", "src/z.txt", "src/c.rs"],
            &["s.rs"],
        ]));
        tree.set_mode(Mode::Tree);
        tree.set_filter(".rs");
        click(&mut tree, "a.rs");
        shift_click(&mut tree, "b.rs");
        // `src/z.txt` is hidden between the selection and `src/c.rs`.
        assert_eq!(
            successor_of_the_selection(&tree),
            Some(row_named(&tree, "c.rs"))
        );
    }

    #[test]
    fn the_successor_passes_over_collapsed_folders() {
        let mut tree = two_groups(&["a.rs", "lib/b.rs", "z.rs"], &["s.rs"], "a.rs");
        tree.set_mode(Mode::Tree);
        let lib = folder(&tree, 0, "lib");
        tree.toggle(0, lib);
        click(&mut tree, "a.rs");
        assert_eq!(
            successor_of_the_selection(&tree),
            Some(row_named(&tree, "z.rs"))
        );
    }

    #[test]
    fn a_selected_file_that_stays_in_its_group_counts_as_staying() {
        // `c.rs` is in conflict, so that staging the selection leaves it where
        // it is. Selected, it is the first file below the lowest selected row
        // that stays: the successor is the file in conflict, on purpose.
        let mut tree = two_groups(&["a.rs", "b.rs", "c.rs", "d.rs"], &["s.rs"], "b.rs");
        shift_click(&mut tree, "c.rs");
        let (group, conflict) = (0, 2);
        let successor = tree.successor_of_selection(|index| {
            tree.is_file_selected(group, index) && index != conflict
        });
        assert_eq!(successor, Some(row_named(&tree, "c.rs")));
    }

    #[test]
    fn the_successor_of_one_selected_file_is_that_of_the_selected_file() {
        for selected in ["a.rs", "b.rs", "c.rs"] {
            let tree = two_groups(&["a.rs", "b.rs", "c.rs"], &["s.rs"], selected);
            assert_eq!(
                successor_of_the_selection(&tree),
                tree.successor_of_selected(),
                "{selected}"
            );
        }
        let tree = FileTree::new(order(&["a.rs"]));
        assert_eq!(tree.successor_of_selected(), None);
    }

    #[test]
    fn there_is_no_successor_without_a_selection() {
        let tree = FileTree::new(order(&FIVE));
        assert_eq!(tree.successor_of_selection(|_| true), None);
    }

    #[test]
    fn selecting_all_and_reading_the_list_again_stay_linear_in_the_files() {
        // Not the frame budget, which `benchmarks.rs` measures in a release
        // build: a bound so wide that only work that grows with the square of
        // the files, minutes for 50,000, can break it, also in a debug build
        // on a busy machine.
        let names: Vec<String> = (0..50_000)
            .map(|n| format!("d{}/f{n}.rs", n % 50))
            .collect();
        let listed: Vec<&str> = names.iter().map(String::as_str).collect();
        let started = std::time::Instant::now();
        let mut tree = FileTree::new(order(&listed));
        tree.set_mode(Mode::Tree);
        tree.select_file(0, 0);
        assert!(tree.select_all_shown());
        assert_eq!(tree.selection_len(), 50_000);
        // The anchor leaves the group, so that another file takes its place.
        let renewed = tree.renewed(order(&listed[1..]));
        assert_eq!(renewed.selection_len(), 49_999);
        assert_eq!(renewed.selected_files().len(), 49_999);
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_secs(20), "took {took:?}");
    }

    /// A deterministic sequence of numbers, so that the test below needs no
    /// dependency and fails the same way each time.
    struct Sequence(u64);

    impl Sequence {
        fn below(&mut self, limit: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % limit as u64) as usize
        }
    }

    #[test]
    fn a_list_read_again_keeps_exactly_the_selected_files_that_are_left() {
        let mut numbers = Sequence(0x9E37_79B9_7F4A_7C15);
        for round in 0..200 {
            let total = 2 + numbers.below(30);
            let names: Vec<String> = (0..total)
                .map(|n| format!("dir{}/f{n}.rs", n % 3))
                .collect();
            let listed: Vec<&str> = names.iter().map(String::as_str).collect();
            let mut tree = FileTree::new(order(&listed));
            if round % 2 == 0 {
                tree.set_mode(Mode::Tree);
            }
            let first = numbers.below(total);
            tree.select_file(0, first);
            for _ in 0..numbers.below(total) {
                if let Some(row) = tree.row_of_file(0, numbers.below(total)) {
                    tree.toggle_row(row);
                }
            }
            let before = selected_paths(&tree);

            // Some files leave the group, and others join it.
            let mut after: Vec<String> = names
                .iter()
                .filter(|_| numbers.below(3) != 0)
                .cloned()
                .collect();
            after.push("new/added.rs".to_owned());
            let listed: Vec<&str> = after.iter().map(String::as_str).collect();
            let renewed = tree.renewed(order(&listed));

            // The files that are left keep their order, as the list sorts
            // them by their names in both modes.
            let kept: Vec<String> = before
                .iter()
                .filter(|path| after.contains(path))
                .cloned()
                .collect();
            let now = selected_paths(&renewed);
            if kept.is_empty() {
                // A selection that is gone gives way to one file, or to none.
                assert!(now.len() <= 1, "round {round}: {before:?} -> {now:?}");
            } else {
                assert_eq!(now, kept, "round {round}: {before:?}");
            }
        }
    }
}
