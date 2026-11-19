//! The pull request diff view (REQ-63): a file tree beside one virtual list of every file's
//! rows, with sticky file headers, hunk separators that expand, word-level
//! changes, the unified and split layouts, placeholders for files without a
//! patch, an image view and viewed marks.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, KeyBinding, KeyContext, ParentElement as _, Render,
    Styled as _, Subscription, Task, UniformListScrollHandle, Window, actions,
};
use subspace_framework::client_github::Client;
use subspace_framework::client_github::model::{GhDiffFile, GhFileStatus, GhItem};
use subspace_framework::dashboard::{MODAL_CONTEXT, Toast, Tokens, prefs};

use super::highlight::Runs;
use super::model::{DiffRow, FileView, Layout};

actions!(
    github_diff,
    [
        /// Next file (an image file opens as images).
        NextFile,
        /// Previous file.
        PreviousFile,
        /// First file.
        FirstFile,
        /// Last file.
        LastFile,
        /// Scroll down three lines.
        ScrollDown,
        /// Scroll up three lines.
        ScrollUp,
        /// Mark the current file as viewed and go to the next, or unmark it.
        ToggleViewed,
        /// Split or unified layout.
        ToggleLayout,
        /// Open the files on GitHub.
        OpenFiles,
        /// Copy the URL.
        CopyUrl,
        /// Close the image, then go back to the list.
        DiffEscape,
        /// Search the file tree.
        SearchFiles,
        /// Leave the tree search.
        CloseSearch,
        /// Go to the first file the tree search shows.
        PickSearch,
    ]
);

pub const DIFF_CONTEXT: &str = "GithubDiff";
const SEARCH_CONTEXT: &str = "GithubTreeSearch";
pub const STYLE_KEY: &str = "github.diffStyle";
/// Every row of the code view has this height, so the view is a virtual list.
const ROW_H: f32 = 22.0;
const NO_RUNS: &Runs = &Vec::new();
const TREE_ROW_H: f32 = 24.0;
/// Scrolling with the keyboard moves three rows.
const SCROLL_LINES: f32 = 3.0;
/// The code view cannot scroll above its first row.
const SCROLL_TOP_PX: f32 = 0.;
/// The rule between the halves of a split row.
const SPLIT_DIVIDER_W_PX: f32 = 1.;
/// The code view's own text size.
const CODE_FONT_PX: f32 = 12.5;
const DEFAULT_ROLE_FONT_PX: f32 = 13.;
/// Title bar padding.
const BAR_PAD_PX: f32 = 8.;
/// File tree: the header height, its padding, the search box padding, the
/// indent per depth and the width of the status letter.
const TREE_HEADER_H_PX: f32 = 28.;
const TREE_PAD_PX: f32 = 10.;
const SEARCH_PAD_X_PX: f32 = 6.;
const SEARCH_PAD_Y_PX: f32 = 4.;
const TREE_INDENT_PX: f32 = 14.;
const TREE_LETTER_W_PX: f32 = 10.;
/// The tree takes at most half the window and at least this width.
const TREE_MAX_SHARE: f32 = 0.5;
const TREE_MIN_W_PX: f32 = 160.0;
/// A viewed file's tree row dims.
const VIEWED_OPACITY: f32 = 0.45;
/// File header: the width of the fold arrow.
const FOLD_W_PX: f32 = 12.;
/// A note row's indent.
const NOTE_PAD_PX: f32 = 36.;
/// A hunk gap's background, and its background under the pointer.
const GAP_BG_OPACITY: f32 = 0.08;
const GAP_HOVER_OPACITY: f32 = 0.16;
/// Image view: the padding, the gap between the sides, the widest failure
/// message and the largest image shown.
const IMAGE_PAD_PX: f32 = 20.;
const IMAGE_GAP_PX: f32 = 32.;
const IMAGE_NOTE_W_PX: f32 = 320.;
const IMAGE_MAX_PX: f32 = 560.;
const BYTES_PER_KB: f32 = 1024.0;
/// Code lines: the line-number column, its padding and dimming, the sign
/// column in each layout, and the colours of added and removed lines.
const NUMBER_W_PX: f32 = 44.;
const NUMBER_PAD_PX: f32 = 8.;
const NUMBER_OPACITY: f32 = 0.7;
const SIGN_W_PX: f32 = 16.;
const SPLIT_SIGN_W_PX: f32 = 14.;
const LINE_BG_OPACITY: f32 = 0.12;
const WORD_BG_OPACITY: f32 = 0.35;
/// The empty half of a split row.
const EMPTY_HALF_OPACITY: f32 = 0.6;

/// The diff is one uniform list, so code and body rows share the larger scale.
fn scaled_code_row_height(body_font: f32, mono_font: f32) -> f32 {
    ROW_H * (body_font / DEFAULT_ROLE_FONT_PX).max(mono_font / DEFAULT_ROLE_FONT_PX)
}

fn code_row_height(tokens: &Tokens) -> f32 {
    scaled_code_row_height(f32::from(tokens.row_font), f32::from(tokens.mono_font))
}

mod chrome;
mod code;
mod image;
mod load;
mod nav;

use image::ImageView;

/// The loaded text of a file, for expanding hunk gaps.
enum Text {
    Loading,
    Ready(Arc<Vec<String>>),
}

pub fn bind_keys(cx: &mut App) {
    let context = format!("{DIFF_CONTEXT} && !Input && !{MODAL_CONTEXT}");
    let context = Some(context.as_str());
    let find_context = format!("{DIFF_CONTEXT} && !Input && !{MODAL_CONTEXT} && !SubspaceRawKeys");
    cx.bind_keys([
        KeyBinding::new("n", NextFile, context),
        KeyBinding::new("p", PreviousFile, context),
        KeyBinding::new("home", FirstFile, context),
        KeyBinding::new("end", LastFile, context),
        KeyBinding::new("j", ScrollDown, context),
        KeyBinding::new("down", ScrollDown, context),
        KeyBinding::new("k", ScrollUp, context),
        KeyBinding::new("up", ScrollUp, context),
        KeyBinding::new("v", ToggleViewed, context),
        KeyBinding::new("s", ToggleLayout, context),
        KeyBinding::new("o", OpenFiles, context),
        KeyBinding::new("y", CopyUrl, context),
        KeyBinding::new("escape", DiffEscape, context),
        KeyBinding::new(
            "escape",
            subspace_framework::dashboard::find_in_page::Close,
            Some(&find_context),
        ),
        KeyBinding::new("/", SearchFiles, context),
        KeyBinding::new("escape", CloseSearch, Some("GithubTreeSearch > Input")),
        KeyBinding::new("enter", PickSearch, Some("GithubTreeSearch > Input")),
    ]);
}

/// What a diff view opens on.
pub struct OpenDiff {
    pub item: GhItem,
    pub login: String,
    pub client: Option<Arc<Client>>,
    pub focus: FocusHandle,
}

/// What the diff view asks of the GitHub dashboard.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffEvent {
    Close,
    Toast(Toast),
}

pub struct DiffView {
    focus: FocusHandle,
    client: Option<Arc<Client>>,
    login: String,
    pub item: GhItem,
    base_sha: String,
    head_sha: String,
    total: u32,
    files: Vec<FileView>,
    loaded: bool,
    error: Option<String>,
    layout: Layout,
    rows: Vec<DiffRow>,
    starts: Vec<usize>,
    collapsed: HashSet<usize>,
    expanded: HashSet<(usize, usize)>,
    /// The expanding side of each file (head, or merge base for removed).
    texts: HashMap<usize, Text>,
    /// Syntax runs per patch line, per file, made when the file first shows.
    highlights: HashMap<usize, Arc<Vec<Runs>>>,
    highlight_theme: Option<Arc<gpui_kit::component::highlighter::HighlightTheme>>,
    /// Syntax runs per line of the loaded text, for expanded lines.
    context_highlights: HashMap<usize, Arc<Vec<Runs>>>,
    viewed: HashSet<String>,
    current: usize,
    last_top: usize,
    /// The top row a navigation scrolls to, until the list gets there.
    pending: Option<usize>,
    scroll: UniformListScrollHandle,
    tree_scroll: UniformListScrollHandle,
    search: Entity<InputState>,
    searching: bool,
    image: Option<ImageView>,
    _search_subscription: Subscription,
    _load: Option<Task<()>>,
}

impl EventEmitter<DiffEvent> for DiffView {}

impl DiffView {
    pub fn new(open: OpenDiff, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let OpenDiff {
            item,
            login,
            client,
            focus,
        } = open;
        let layout = match prefs::get(cx, STYLE_KEY).as_deref() {
            Some("split") => Layout::Split,
            _ => Layout::Unified,
        };
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search files"));
        let subscription = cx.subscribe(&search, |_: &mut Self, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut view = Self {
            focus,
            client,
            login,
            item,
            base_sha: String::new(),
            head_sha: String::new(),
            total: 0,
            files: Vec::new(),
            loaded: false,
            error: None,
            layout,
            rows: Vec::new(),
            starts: Vec::new(),
            collapsed: HashSet::new(),
            expanded: HashSet::new(),
            texts: HashMap::new(),
            highlights: HashMap::new(),
            highlight_theme: None,
            context_highlights: HashMap::new(),
            viewed: HashSet::new(),
            current: 0,
            last_top: 0,
            pending: None,
            scroll: UniformListScrollHandle::new(),
            tree_scroll: UniformListScrollHandle::new(),
            search,
            searching: false,
            image: None,
            _search_subscription: subscription,
            _load: None,
        };
        view.load(cx);
        view
    }

    pub fn key(&self) -> &str {
        &self.item.key
    }

    pub fn files_url(&self) -> String {
        format!("{}/files", self.item.url)
    }

    /// The file tree, once the diff has files to show.
    fn tree_pane(
        &self,
        tokens: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        (self.loaded && !self.files.is_empty()).then(|| self.tree(tokens, window, cx))
    }

    fn blob_url(&self, file: &GhDiffFile) -> String {
        let sha = if file.status == GhFileStatus::Removed {
            &self.base_sha
        } else {
            &self.head_sha
        };
        format!(
            "https://github.com/{}/blob/{sha}/{}",
            self.item.repo, file.path
        )
    }
}

impl Focusable for DiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let mut context = KeyContext::new_with_defaults(cx.platform_info().os);
        context.add(DIFF_CONTEXT);
        let tree = self.tree_pane(&tokens, window, cx);
        let code = self.code(&tokens, cx);
        v_flex()
            .id("github-diff")
            .key_context(context)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::next_file))
            .on_action(cx.listener(Self::previous_file))
            .on_action(cx.listener(Self::first_file))
            .on_action(cx.listener(Self::last_file))
            .on_action(cx.listener(Self::scroll_down))
            .on_action(cx.listener(Self::scroll_up))
            .on_action(cx.listener(Self::toggle_viewed))
            .on_action(cx.listener(Self::toggle_layout))
            .on_action(cx.listener(Self::open_files))
            .on_action(cx.listener(Self::copy_url))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::open_search))
            .on_action(cx.listener(Self::close_search))
            .on_action(cx.listener(Self::pick_search))
            .size_full()
            .min_w_0()
            .bg(tokens.bg)
            .text_color(tokens.fg)
            .child(self.title_bar(&tokens, cx))
            .child(h_flex().flex_1().min_h_0().children(tree).child(code))
    }
}

#[cfg(test)]
mod size_tests {
    use super::scaled_code_row_height;

    #[test]
    fn diff_rows_follow_the_larger_text_role_without_changing_the_defaults() {
        assert_eq!(scaled_code_row_height(13., 13.), 22.);
        assert!((scaled_code_row_height(18., 13.) - 22. * 18. / 13.).abs() < 0.001);
        assert!((scaled_code_row_height(13., 20.) - 22. * 20. / 13.).abs() < 0.001);
    }
}
