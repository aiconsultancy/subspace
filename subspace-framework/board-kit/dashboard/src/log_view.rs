//! The Framework's log viewer: the records in `subspace_core::log`'s buffer,
//! filtered by level, target and text, following the tail while asked to.
//! The app opens it in place of the board, as it opens the Search view; its
//! header's Close and Escape return to the board.

mod filter;
#[cfg(test)]
mod runtime_tests;
mod view;

use std::{path::PathBuf, time::Duration};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle,
    Subscription, Task, Window,
};
use subspace_core::log::{self, Level, Record};

use crate::menu::{Menu, MenuAnchors, MenuContents, MenuEvent, MenuOption};
use crate::prefs;

// Keep the established action namespace so menus, shortcuts and callers share
// the same command even though its declaration lives below the app shell.
gpui_kit::actions!(subspace_app, [OpenLogView]);

pub use filter::{Filter, visible};

/// The stored capture level, read by the app at startup.
pub const LEVEL_KEY: &str = "app.log-level";
/// How often follow mode asks the buffer for new records.
const FOLLOW_EVERY: Duration = Duration::from_millis(500);
/// What the target filter offers for every target.
const ALL_TARGETS: &str = "all";

pub enum LogViewEvent {
    Close,
}

/// Which popover menu is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuKind {
    Target,
    Capture,
}

impl MenuKind {
    fn id(self) -> &'static str {
        match self {
            Self::Target => "log-target",
            Self::Capture => "log-capture",
        }
    }
}

pub struct LogView {
    records: Vec<Record>,
    log_path: Option<PathBuf>,
    filter: Filter,
    input: Entity<InputState>,
    follow: bool,
    scroll: ScrollHandle,
    menu: Option<(Entity<Menu>, Subscription, MenuKind)>,
    menu_anchors: MenuAnchors,
    poll: Option<Task<()>>,
    focus: FocusHandle,
    _subscription: Subscription,
    toolbar: Entity<crate::overflow_toolbar::OverflowToolbar>,
}

impl EventEmitter<LogViewEvent> for LogView {}

impl Focusable for LogView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl LogView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        view::bind_keys(cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter the log…"));
        let subscription = cx.subscribe_in(&input, window, |this: &mut Self, _, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.filter.text = this.input.read(cx).value().to_string();
                cx.notify();
            }
        });
        let mut view = Self {
            records: log::recent(),
            log_path: log::path(),
            filter: Filter::default(),
            input,
            follow: true,
            scroll: ScrollHandle::new(),
            menu: None,
            menu_anchors: MenuAnchors::default(),
            poll: None,
            focus: cx.focus_handle(),
            _subscription: subscription,
            toolbar: cx.new(|_| crate::overflow_toolbar::OverflowToolbar::new(Vec::new())),
        };
        view.menu_anchors.set_toolbar(&view.toolbar);
        view.start_following(cx);
        view
    }

    /// The records the filters keep, oldest first.
    fn shown(&self) -> Vec<&Record> {
        visible(&self.records, &self.filter)
    }

    /// Every target in the buffer, for the target menu.
    fn targets(&self) -> Vec<String> {
        let mut targets: Vec<String> = self
            .records
            .iter()
            .map(|record| record.target.clone())
            .collect();
        targets.sort();
        targets.dedup();
        targets
    }

    fn start_following(&mut self, cx: &mut Context<Self>) {
        if !self.follow {
            self.poll = None;
            return;
        }
        self.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FOLLOW_EVERY).await;
                let stopped = this.update(cx, |this, cx| this.pull(cx)).unwrap_or(true);
                if stopped {
                    break;
                }
            }
        }));
    }

    /// New records since the last pull; true when following stopped.
    fn pull(&mut self, cx: &mut Context<Self>) -> bool {
        let last = self.records.last().map(|record| record.seq).unwrap_or(0);
        let fresh = log::recent_after(last);
        if !fresh.is_empty() {
            self.records.extend(fresh);
            let overflow = self.records.len().saturating_sub(log::BUFFER_RECORDS);
            self.records.drain(..overflow);
            self.scroll_to_end();
            cx.notify();
        }
        !self.follow
    }

    fn scroll_to_end(&self) {
        let count = self.shown().len();
        if count > 0 {
            self.scroll.scroll_to_item(count - 1);
        }
    }

    fn set_follow(&mut self, follow: bool, cx: &mut Context<Self>) {
        self.follow = follow;
        self.start_following(cx);
        if follow {
            self.pull(cx);
            self.scroll_to_end();
        }
        cx.notify();
    }

    fn set_level(&mut self, level: Level, cx: &mut Context<Self>) {
        self.filter.level = level;
        cx.notify();
    }

    /// The capture level: what the sink keeps from now on, remembered
    /// between runs.
    fn set_capture(level: Level, cx: &mut Context<Self>) {
        log::set_level(level);
        prefs::set(cx, LEVEL_KEY, level.as_str());
        cx.notify();
    }

    fn menu_options(&self, kind: MenuKind) -> Vec<MenuOption> {
        match kind {
            MenuKind::Target => {
                let all = MenuOption::new(ALL_TARGETS, "All targets")
                    .checked(self.filter.target.is_none());
                let rest = self.targets().into_iter().map(|target| {
                    let checked = self.filter.target.as_deref() == Some(target.as_str());
                    MenuOption::new(target.clone(), target).checked(checked)
                });
                std::iter::once(all).chain(rest).collect()
            }
            MenuKind::Capture => Level::ALL
                .into_iter()
                .map(|level| {
                    MenuOption::new(level.as_str(), capture_label(level))
                        .checked(level == log::level())
                })
                .collect(),
        }
    }

    fn open_menu(&mut self, kind: MenuKind, window: &mut Window, cx: &mut Context<Self>) {
        let (label, placeholder) = view::menu_frame(kind);
        let options = self.menu_options(kind);
        let anchor = self.menu_anchors.anchor(kind.id(), cx);
        let Some(menu) = self.menu_anchors.open(
            anchor,
            MenuContents::new(label, placeholder, options),
            window,
            cx,
        ) else {
            return;
        };
        let subscription = cx.subscribe_in(&menu, window, |this, _, event, _, cx| {
            let kind = this.menu.take().map(|(_, _, kind)| kind);
            if let (MenuEvent::Pick(value), Some(kind)) = (event, kind) {
                this.picked(kind, value.as_ref(), cx);
            }
            cx.notify();
        });
        self.menu = Some((menu, subscription, kind));
        cx.notify();
    }

    fn picked(&mut self, kind: MenuKind, value: &str, cx: &mut Context<Self>) {
        match kind {
            MenuKind::Target => self.set_target(value),
            MenuKind::Capture => Self::capture(value, cx),
        }
    }

    /// Filters to one target, or to all of them for the `ALL_TARGETS` entry.
    fn set_target(&mut self, value: &str) {
        self.filter.target = (value != ALL_TARGETS).then(|| value.to_owned());
    }

    /// Sets the capture level a menu entry names; an unknown name is ignored.
    fn capture(value: &str, cx: &mut Context<Self>) {
        Level::parse(value)
            .into_iter()
            .for_each(|level| Self::set_capture(level, cx));
    }

    /// Leave the view: dismiss an open menu, then ask the shell to close it.
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.escape(window, cx);
    }

    /// Escape closes an open menu first, and the view only when none is open.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_anchors.dismiss(window, cx);
        if self.menu.take().is_none() {
            cx.emit(LogViewEvent::Close);
        }
        cx.notify();
    }

    /// The shown lines as text, for the clipboard.
    fn text(&self) -> String {
        self.shown()
            .iter()
            .map(|record| record.line())
            .collect::<Vec<String>>()
            .join("\n")
    }
}

/// `Capture: info`, the capture menu's labels.
fn capture_label(level: Level) -> String {
    format!("Capture {}", level.as_str())
}

#[cfg(test)]
mod tests {
    use subspace_core::log::Level;

    use super::capture_label;

    #[test]
    fn the_capture_menu_names_its_levels() {
        assert_eq!(capture_label(Level::Debug), "Capture debug");
    }
}
