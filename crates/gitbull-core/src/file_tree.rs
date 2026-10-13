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
                found(*index).or_else(|| {
                    // The file left its group. Its neighbours are looked up in
                    // one step each: after "Stage all" none of them is left,
                    // and a search of the list per neighbour would cost the
                    // square of its length.
                    let places: HashMap<&RepoPath, usize> = order
                        .groups
                        .get(*group)
                        .into_iter()
                        .flat_map(|g| g.files.iter().enumerate())
                        .map(|(at, file)| (&file.path, at))
                        .collect();
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
        let (Some(Selected::File { group, .. }), Some(_)) = (&self.selected, self.selected_row)
        else {
            return None;
        };
        self.neighbours_of_selected()
            .next()
            .map(|(row, _)| row)
            .or_else(|| self.first_file_outside(*group))
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
        self.build();
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
    }

    /// Selects the file at `index` of `group`; when it is not shown, the
    /// first file shown.
    pub fn select_file(&mut self, group: usize, index: usize) {
        self.selected = Some(Selected::File { group, index });
        self.wants_first = false;
        self.find_selection();
    }

    /// Selects the first file shown; while the filter shows none, the
    /// first one it shows again.
    pub fn select_first(&mut self) {
        self.selected = None;
        self.selected_row = None;
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
}
