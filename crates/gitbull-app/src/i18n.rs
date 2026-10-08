//! User-visible texts, from the Fluent files in `i18n/`.
//!
//! Every text the UI shows has a variant in [`Msg`]. The UI can only ask for
//! those, and a test checks that each one exists in the English file.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

include!(concat!(env!("OUT_DIR"), "/languages.rs"));

/// The language every other one falls back to.
pub const ENGLISH: &str = "en-US";

macro_rules! messages {
    ($($variant:ident => $id:literal,)*) => {
        /// A text of the UI.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Msg {
            $($variant,)*
        }

        impl Msg {
            /// Every text of the UI.
            pub const ALL: &[Msg] = &[$(Msg::$variant,)*];

            /// The message id in the Fluent files.
            pub fn id(self) -> &'static str {
                match self {
                    $(Msg::$variant => $id,)*
                }
            }
        }
    };
}

messages! {
    AppTitle => "app-title",
    StartTitle => "start-title",
    StartMissing => "start-missing",
    StartConfiguredMissing => "start-configured-missing",
    StartTooOld => "start-too-old",
    StartUnusable => "start-unusable",
    StartInstallWindows => "start-install-windows",
    StartInstallMacos => "start-install-macos",
    StartInstallLinux => "start-install-linux",
    StartCheckAgain => "start-check-again",
    StartSetPath => "start-set-path",
    StartDetails => "start-details",
    TabNew => "tab-new",
    TabClose => "tab-close",
    TabOpening => "tab-opening",
    WindowMinimize => "window-minimize",
    WindowMaximize => "window-maximize",
    WindowRestore => "window-restore",
    WindowClose => "window-close",
    HomeTab => "home-tab",
    HomeFilter => "home-filter",
    HomeOpenFolder => "home-open-folder",
    HomePinned => "home-pinned",
    HomeRecent => "home-recent",
    HomeEmpty => "home-empty",
    HomeNoMatch => "home-no-match",
    HomeReading => "home-reading",
    HomeClean => "home-clean",
    HomeChanged => "home-changed",
    HomeConflicts => "home-conflicts",
    HomeNotFound => "home-not-found",
    HomeBare => "home-bare",
    HomeDone => "home-done",
    HomeStateConflict => "home-state-conflict",
    HomeStateWorking => "home-state-working",
    HomeStateNew => "home-state-new",
    HomeStateReady => "home-state-ready",
    HomeStatePaused => "home-state-paused",
    HomeOverlap => "home-overlap",
    HomeNewBranches => "home-new-branches",
    HomeFolderGone => "home-folder-gone",
    HomeAheadBehind => "home-ahead-behind",
    HomeLines => "home-lines",
    CockpitDetails => "cockpit-details",
    CockpitShow => "cockpit-show",
    CockpitBaseDetected => "cockpit-base-detected",
    CockpitBaseSet => "cockpit-base-set",
    CockpitBaseUpstream => "cockpit-base-upstream",
    CockpitNoBase => "cockpit-no-base",
    CockpitBase => "cockpit-base",
    CockpitDetect => "cockpit-detect",
    CockpitCounts => "cockpit-counts",
    CockpitLines => "cockpit-lines",
    CockpitNoPredictionGit => "cockpit-no-prediction-git",
    CockpitNoPredictionDriver => "cockpit-no-prediction-driver",
    CockpitNewCommits => "cockpit-new-commits",
    CockpitMoreCommits => "cockpit-more-commits",
    CockpitFiles => "cockpit-files",
    CockpitMoreFiles => "cockpit-more-files",
    CockpitUncommitted => "cockpit-uncommitted",
    CockpitOverlaps => "cockpit-overlaps",
    CockpitOverlapWith => "cockpit-overlap-with",
    CockpitWorktrees => "cockpit-worktrees",
    CockpitBranches => "cockpit-branches",
    CockpitReading => "cockpit-reading",
    CockpitNothing => "cockpit-nothing",
    CockpitCopyAi => "cockpit-copy-ai",
    CockpitCopyChoices => "cockpit-copy-choices",
    CockpitCopySummary => "cockpit-copy-summary",
    CockpitCopyDiff => "cockpit-copy-diff",
    CockpitCopied => "cockpit-copied",
    CockpitCopyAiDiff => "cockpit-copy-ai-diff",
    CockpitCopyFailed => "cockpit-copy-failed",
    CockpitReadFailed => "cockpit-read-failed",
    CockpitBranchFailed => "cockpit-branch-failed",
    CockpitOpenRemote => "cockpit-open-remote",
    HomeMarkSeen => "home-mark-seen",
    HomeMarkAllSeen => "home-mark-all-seen",
    HomeActiveNow => "home-active-now",
    HomeActiveMinutes => "home-active-minutes",
    HomeActiveHours => "home-active-hours",
    HomeActiveDays => "home-active-days",
    HomeActiveWeeks => "home-active-weeks",
    HomeOpen => "home-open",
    HomeShowInFileManager => "home-show-in-file-manager",
    HomePin => "home-pin",
    HomeUnpin => "home-unpin",
    HomeRemove => "home-remove",
    HomeFileManagerFailed => "home-file-manager-failed",
    NoticeNotARepository => "notice-not-a-repository",
    NoticeDismiss => "notice-dismiss",
    ToolbarOpen => "toolbar-open",
    ToolbarRefresh => "toolbar-refresh",
    ToolbarBranch => "toolbar-branch",
    ToolbarTheme => "toolbar-theme",
    ToolbarSettings => "toolbar-settings",
    ThemeSystem => "theme-system",
    ThemeLight => "theme-light",
    ThemeDark => "theme-dark",
    SidebarWorkspace => "sidebar-workspace",
    SidebarBranches => "sidebar-branches",
    SidebarTags => "sidebar-tags",
    SidebarRemotes => "sidebar-remotes",
    SidebarStashes => "sidebar-stashes",
    SidebarSubmodules => "sidebar-submodules",
    Sidebar => "sidebar",
    ViewHistory => "view-history",
    ViewFileStatus => "view-file-status",
    ViewSearch => "view-search",
    ColumnGraph => "column-graph",
    ColumnDescription => "column-description",
    ColumnDate => "column-date",
    ColumnAuthor => "column-author",
    ColumnCommit => "column-commit",
    ColumnPath => "column-path",
    NoticeHiddenByFilter => "notice-hidden-by-filter",
    NoticeShowAllBranches => "notice-show-all-branches",
    NoticeNotACommit => "notice-not-a-commit",
    NoticeHashUnknown => "notice-hash-unknown",
    NoticeHashAmbiguous => "notice-hash-ambiguous",
    NoticeNotInHistory => "notice-not-in-history",
    SearchHint => "search-hint",
    SearchMode => "search-mode",
    SearchModeHash => "search-mode-hash",
    SearchModeMessage => "search-mode-message",
    SearchModeAuthor => "search-mode-author",
    SearchModePath => "search-mode-path",
    SearchNext => "search-next",
    SearchPrevious => "search-previous",
    SearchRunning => "search-running",
    SearchCount => "search-count",
    SearchNone => "search-none",
    SearchEmpty => "search-empty",
    SearchHashHint => "search-hash-hint",
    SearchFailed => "search-failed",
    SearchMatch => "search-match",
    SidebarFilter => "sidebar-filter",
    SidebarCurrentBranch => "sidebar-current-branch",
    SidebarNotInitialised => "sidebar-not-initialised",
    SidebarShowOnlyBranch => "sidebar-show-only-branch",
    FilterAllBranches => "filter-all-branches",
    FilterCurrentBranch => "filter-current-branch",
    RowLoading => "row-loading",
    HistoryEmpty => "history-empty",
    HistoryUncommitted => "history-uncommitted",
    OpenFileStatus => "open-file-status",
    GraphHint => "graph-hint",
    GraphGenerate => "graph-generate",
    GraphConfirmTitle => "graph-confirm-title",
    GraphConfirmBody => "graph-confirm-body",
    GraphConfirmGenerate => "graph-confirm-generate",
    ActionCheckout => "action-checkout",
    ActionCreateBranch => "action-create-branch",
    CreateFailedTitle => "create-failed-title",
    CreateRefusedTaken => "create-refused-taken",
    CreateRefusedInvalid => "create-refused-invalid",
    CloseQuestionTitle => "close-question-title",
    CloseQuestionTab => "close-question-tab",
    CloseQuestionWindow => "close-question-window",
    CloseQuestionWindowMany => "close-question-window-many",
    CloseKeepOpen => "close-keep-open",
    CloseAnyway => "close-anyway",
    SettingsGitBusy => "settings-git-busy",
    SettingsBehaviour => "settings-behaviour",
    SettingsDetachNotice => "settings-detach-notice",
    SidebarCheckOut => "sidebar-check-out",
    CommitCheckOut => "commit-check-out",
    CommitCreateBranch => "commit-create-branch",
    SidebarCreateBranch => "sidebar-create-branch",
    CreateBranchTitle => "create-branch-title",
    CreateStartCaption => "create-start-caption",
    CreateStartReference => "create-start-reference",
    CreateStartHead => "create-start-head",
    CreateBranchName => "create-branch-name",
    CreateCheckOut => "create-check-out",
    CreateConfirm => "create-confirm",
    NameControl => "name-control",
    NameCharacter => "name-character",
    NameDoubleDot => "name-double-dot",
    NameAtBrace => "name-at-brace",
    NameLeadingDash => "name-leading-dash",
    NameLeadingSlash => "name-leading-slash",
    NameTrailingSlash => "name-trailing-slash",
    NameDoubleSlash => "name-double-slash",
    NameEndsWithDot => "name-ends-with-dot",
    NameEndsWithLock => "name-ends-with-lock",
    NamePartStartsWithDot => "name-part-starts-with-dot",
    NameReserved => "name-reserved",
    NameTakenBranch => "name-taken-branch",
    NameTakenTag => "name-taken-tag",
    NameNeedsFolderBranch => "name-needs-folder-branch",
    NameNeedsFolderTag => "name-needs-folder-tag",
    NameIsFolderOfBranch => "name-is-folder-of-branch",
    NameIsFolderOfTag => "name-is-folder-of-tag",
    DetachTitle => "detach-title",
    DetachBody => "detach-body",
    DetachDontShow => "detach-dont-show",
    DetachCheckOut => "detach-check-out",
    DialogCancel => "dialog-cancel",
    DialogClose => "dialog-close",
    DialogCopyMessage => "dialog-copy-message",
    CheckoutBlockedTitle => "checkout-blocked-title",
    CheckoutBlockedChanges => "checkout-blocked-changes",
    CheckoutBlockedUntracked => "checkout-blocked-untracked",
    CheckoutFailedTitle => "checkout-failed-title",
    CheckoutFailedBody => "checkout-failed-body",
    CheckoutHookTitle => "checkout-hook-title",
    CheckoutHookBody => "checkout-hook-body",
    SidebarElsewhere => "sidebar-elsewhere",
    CheckoutWorktreeBody => "checkout-worktree-body",
    CheckoutOpenWorktree => "checkout-open-worktree",
    CheckoutTwinFollows => "checkout-twin-follows",
    CheckoutTwinFollowsNone => "checkout-twin-follows-none",
    GraphCancel => "graph-cancel",
    GraphGenerating => "graph-generating",
    GraphFailed => "graph-failed",
    CopyFullHash => "copy-full-hash",
    CopyShortHash => "copy-short-hash",
    CopyMessage => "copy-message",
    Copied => "copied",
    PanelCommit => "panel-commit",
    PanelDiff => "panel-diff",
    DetailCommit => "detail-commit",
    DetailParents => "detail-parents",
    DetailAuthor => "detail-author",
    DetailCommitter => "detail-committer",
    DetailReferences => "detail-references",
    DetailChanges => "detail-changes",
    DetailFiles => "detail-files",
    LinesBinary => "lines-binary",
    FilesNone => "files-none",
    FilesFilter => "files-filter",
    FilesShowTree => "files-show-tree",
    FilesNoMatch => "files-no-match",
    FilesFailed => "files-failed",
    FileHistory => "file-history",
    FileBlame => "file-blame",
    CopyPath => "copy-path",
    Back => "back",
    FileHistoryTitle => "file-history-title",
    FileHistoryNone => "file-history-none",
    FileHistoryFailed => "file-history-failed",
    BlameTitle => "blame-title",
    BlameBinary => "blame-binary",
    BlameFailed => "blame-failed",
    ChangeAdded => "change-added",
    ChangeModified => "change-modified",
    ChangeDeleted => "change-deleted",
    ChangeRenamed => "change-renamed",
    ChangeCopied => "change-copied",
    ChangeTypeChanged => "change-type-changed",
    ChangeConflicted => "change-conflicted",
    ChangeUntracked => "change-untracked",
    PanelFiles => "panel-files",
    FileStatusStaged => "file-status-staged",
    FileStatusUnstaged => "file-status-unstaged",
    FileStatusUntracked => "file-status-untracked",
    FileStatusLoading => "file-status-loading",
    FileStatusClean => "file-status-clean",
    DiffUnchanged => "diff-unchanged",
    DiffMode => "diff-mode",
    DiffBinary => "diff-binary",
    DiffSubmodule => "diff-submodule",
    DiffAbsent => "diff-absent",
    DiffSize => "diff-size",
    DiffTruncated => "diff-truncated",
    DiffLoadAll => "diff-load-all",
    DiffCut => "diff-cut",
    DiffNoNewline => "diff-no-newline",
    DiffFailed => "diff-failed",
    DiffMissingContent => "diff-missing-content",
    DiffLineAdded => "diff-line-added",
    DiffLineRemoved => "diff-line-removed",
    DiffLineContext => "diff-line-context",
    DiffHiddenLines => "diff-hidden-lines",
    DiffShowAfterPrevious => "diff-show-after-previous",
    DiffShowBeforeNext => "diff-show-before-next",
    DiffShowAll => "diff-show-all",
    DiffTooLarge => "diff-too-large",
    DiffShowInvisibles => "diff-show-invisibles",
    DiffPreviousHunk => "diff-previous-hunk",
    DiffNextHunk => "diff-next-hunk",
    CopyLines => "copy-lines",
    CopyHunk => "copy-hunk",
    ErrorOpenFailed => "error-open-failed",
    ErrorInternal => "error-internal",
    ErrorOwnership => "error-ownership",
    ErrorDetails => "error-details",
    ErrorCommand => "error-command",
    ErrorRetry => "error-retry",
    ErrorClose => "error-close",
    SettingsTitle => "settings-title",
    SettingsClose => "settings-close",
    SettingsAppearance => "settings-appearance",
    SettingsColourVision => "settings-colour-vision",
    SettingsInterfaceSize => "settings-interface-size",
    SettingsTitleBar => "settings-title-bar",
    SettingsSystemTitleBar => "settings-system-title-bar",
    SettingsAtNextStart => "settings-at-next-start",
    SettingsSectionGit => "settings-section-git",
    ColourVisionStandard => "colour-vision-standard",
    ColourVisionRedGreen => "colour-vision-red-green",
    ColourVisionBlueYellow => "colour-vision-blue-yellow",
    InterfaceSizePercent => "interface-size",
    SettingsTheme => "settings-theme",
    SettingsLanguage => "settings-language",
    SettingsGit => "settings-git",
    SettingsGitAutomatic => "settings-git-automatic",
    SettingsGitBrowse => "settings-git-browse",
    SettingsGitApply => "settings-git-apply",
    SettingsGitApplied => "settings-git-applied",
    StatusGitVersion => "status-git-version",
    StatusLoading => "status-loading",
    StatusLoadedSoFar => "status-loaded-so-far",
    StatusCommits => "status-commits",
    StatusHome => "status-home",
    StatusHomeReading => "status-home-reading",
    StatusDetached => "status-detached",
    StatusSettingsReset => "status-settings-reset",
}

/// The texts for the chosen language, with English for anything it lacks.
pub struct Translations {
    chosen: Option<FluentBundle<FluentResource>>,
    english: FluentBundle<FluentResource>,
}

impl Translations {
    /// Texts for `language` from the embedded files.
    pub fn load(language: &str) -> Translations {
        Translations::from_resources(RESOURCES, language)
    }

    /// Texts for `language` from `resources`, pairs of language tag and
    /// Fluent source.
    pub fn from_resources(resources: &[(&str, &str)], language: &str) -> Translations {
        let find = |wanted: &str| {
            resources
                .iter()
                .find(|(tag, _)| *tag == wanted)
                .and_then(|(tag, source)| bundle(tag, source))
        };
        let english = find(ENGLISH)
            .or_else(|| bundle(ENGLISH, ""))
            .expect("an empty English bundle can always be built");
        let chosen = if language == ENGLISH {
            None
        } else {
            find(language)
        };
        Translations { chosen, english }
    }

    /// The text of `msg`.
    pub fn text(&self, msg: Msg) -> String {
        self.text_with(msg, None)
    }

    /// The text of `msg` with the values of its placeholders.
    pub fn text_with(&self, msg: Msg, args: Option<&FluentArgs>) -> String {
        for bundle in self.chosen.iter().chain(std::iter::once(&self.english)) {
            if let Some(pattern) = bundle.get_message(msg.id()).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                return bundle
                    .format_pattern(pattern, args, &mut errors)
                    .into_owned();
            }
        }
        // Unreachable while the completeness test passes.
        msg.id().to_owned()
    }
}

/// The languages git-bull ships, as tags.
pub fn languages() -> impl Iterator<Item = &'static str> {
    RESOURCES.iter().map(|(tag, _)| *tag)
}

fn bundle(tag: &str, source: &str) -> Option<FluentBundle<FluentResource>> {
    let language: LanguageIdentifier = tag.parse().ok()?;
    // A file with errors still yields the messages that parsed.
    let resource =
        FluentResource::try_new(source.to_owned()).unwrap_or_else(|(partial, _)| partial);
    let mut bundle = FluentBundle::new_concurrent(vec![language]);
    // Isolation marks keep right-to-left placeholders apart, which this
    // milestone does not support; egui would draw them as boxes.
    bundle.set_use_isolating(false);
    bundle.add_resource_overriding(resource);
    Some(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGLISH_SOURCE: &str = "toolbar-open = Open\ntoolbar-refresh = Refresh\n";

    #[test]
    fn every_text_of_the_ui_exists_in_english() {
        let (_, source) = RESOURCES
            .iter()
            .find(|(tag, _)| *tag == ENGLISH)
            .expect("English is embedded");
        let resource = FluentResource::try_new(source.to_string()).expect("valid Fluent");
        let ids: Vec<&str> = resource
            .entries()
            .filter_map(|entry| match entry {
                fluent_syntax::ast::Entry::Message(message) => Some(message.id.name),
                _ => None,
            })
            .collect();
        let missing: Vec<&str> = Msg::ALL
            .iter()
            .map(|msg| msg.id())
            .filter(|id| !ids.contains(id))
            .collect();
        assert!(missing.is_empty(), "missing in {ENGLISH}.ftl: {missing:?}");
    }

    #[test]
    fn english_is_used_by_default() {
        assert_eq!(Translations::load(ENGLISH).text(Msg::ToolbarOpen), "Open");
    }

    #[test]
    fn unknown_language_falls_back_to_english() {
        assert_eq!(Translations::load("xx-YY").text(Msg::ToolbarOpen), "Open");
    }

    #[test]
    fn an_added_language_is_used_when_chosen() {
        let resources = [
            (ENGLISH, ENGLISH_SOURCE),
            ("de-DE", "toolbar-open = Öffnen\n"),
        ];
        let texts = Translations::from_resources(&resources, "de-DE");
        assert_eq!(texts.text(Msg::ToolbarOpen), "Öffnen");
    }

    #[test]
    fn text_missing_in_the_chosen_language_comes_from_english() {
        let resources = [
            (ENGLISH, ENGLISH_SOURCE),
            ("de-DE", "toolbar-open = Öffnen\n"),
        ];
        let texts = Translations::from_resources(&resources, "de-DE");
        assert_eq!(texts.text(Msg::ToolbarRefresh), "Refresh");
    }

    #[test]
    fn placeholders_are_filled_without_isolation_marks() {
        let resources = [(ENGLISH, "app-title = git-bull – { $repository }\n")];
        let texts = Translations::from_resources(&resources, ENGLISH);
        let mut args = FluentArgs::new();
        args.set("repository", "linux");
        assert_eq!(
            texts.text_with(Msg::AppTitle, Some(&args)),
            "git-bull – linux"
        );
    }
}
