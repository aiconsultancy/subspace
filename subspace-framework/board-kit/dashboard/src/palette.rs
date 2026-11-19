//! The commands and item search a dashboard offers the ⌘K palette.

use std::rc::Rc;

use gpui_kit::{Action, App, SharedString, Window};

use crate::{Row, SearchDoc, SearchIndex};

pub type Callback = Rc<dyn Fn(&mut Window, &mut App)>;
pub type PickCallback = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// One choice in a command's sub-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteOption {
    pub value: SharedString,
    pub label: SharedString,
    pub hint: Option<SharedString>,
    pub checked: bool,
}

impl PaletteOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            hint: None,
            checked: false,
        }
    }

    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// A second step, such as the sprint or team list, shown in place of the
/// palette's root list. Picking an option closes the palette and calls
/// `on_pick` with the option's value.
pub struct SubList {
    pub placeholder: SharedString,
    pub options: Vec<PaletteOption>,
    pub on_pick: PickCallback,
}

impl SubList {
    pub fn new(
        placeholder: impl Into<SharedString>,
        options: Vec<PaletteOption>,
        on_pick: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            placeholder: placeholder.into(),
            options,
            on_pick: Rc::new(on_pick),
        }
    }
}

/// What a palette command does.
pub enum CommandRun {
    /// Dispatch the action to the focused dashboard's view, like a key.
    Action(Box<dyn Action>),
    Callback(Callback),
    Sub(Rc<SubList>),
}

impl Clone for CommandRun {
    fn clone(&self) -> Self {
        match self {
            Self::Action(action) => Self::Action(action.boxed_clone()),
            Self::Callback(callback) => Self::Callback(callback.clone()),
            Self::Sub(sub) => Self::Sub(sub.clone()),
        }
    }
}

/// A palette command a dashboard offers.
#[derive(Clone)]
pub struct PaletteCommand {
    pub label: SharedString,
    /// Extra words the palette matches, separated by spaces.
    pub keywords: SharedString,
    /// The key that does the same, shown at the right.
    pub shortcut: Option<SharedString>,
    pub run: CommandRun,
    /// What the palette's recent commands store for this command, when its
    /// label changes with state, such as *Back to #discovery*. `None`
    /// stores the label.
    pub recent_key: Option<SharedString>,
}

impl PaletteCommand {
    /// A command that dispatches `action` to the dashboard's view, so the
    /// dashboard handles it with `on_action` like a key.
    pub fn new(label: impl Into<SharedString>, action: impl Action) -> Self {
        Self::with_run(label, CommandRun::Action(Box::new(action)))
    }

    /// A command that calls `run`. Capture `cx.entity().downgrade()` to
    /// reach the dashboard.
    pub fn callback(
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self::with_run(label, CommandRun::Callback(Rc::new(run)))
    }

    /// A command that shows `sub` in place of the palette's list.
    pub fn sub(label: impl Into<SharedString>, sub: SubList) -> Self {
        Self::with_run(label, CommandRun::Sub(Rc::new(sub)))
    }

    fn with_run(label: impl Into<SharedString>, run: CommandRun) -> Self {
        Self {
            label: label.into(),
            keywords: SharedString::default(),
            shortcut: None,
            run,
            recent_key: None,
        }
    }

    pub fn keywords(mut self, keywords: impl Into<SharedString>) -> Self {
        self.keywords = keywords.into();
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

impl std::fmt::Debug for PaletteCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let run = match &self.run {
            CommandRun::Action(action) => action.name(),
            CommandRun::Callback(_) => "callback",
            CommandRun::Sub(_) => "sub-list",
        };
        f.debug_struct("PaletteCommand")
            .field("label", &self.label)
            .field("run", &run)
            .finish()
    }
}

// ---- Item search ------------------------------------------------------------------

/// A dashboard's loaded items, indexed for the palette (REQ-05). Build it
/// again when the items change; searching makes no request.
pub struct ItemSearch {
    index: SearchIndex,
    rows: Vec<Row>,
}

impl ItemSearch {
    /// Each item's search document and the row the palette shows for it.
    pub fn new(items: impl IntoIterator<Item = (SearchDoc, Row)>) -> Self {
        let (docs, rows) = items.into_iter().unzip();
        Self {
            index: SearchIndex::new(docs),
            rows,
        }
    }

    /// The best `limit` items for `query`, best first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<(&SearchDoc, &Row)> {
        self.index
            .search(query, limit)
            .into_iter()
            .map(|hit| (&self.index.docs()[hit.index], &self.rows[hit.index]))
            .collect()
    }

    /// The item with document id `id`.
    pub fn find(&self, id: &str) -> Option<(&SearchDoc, &Row)> {
        let at = self.index.docs().iter().position(|doc| doc.id == id)?;
        Some((&self.index.docs()[at], &self.rows[at]))
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// What `/` searches in a dashboard.
#[derive(Clone)]
pub struct Search {
    /// Such as "Search this board"; also the help text for `/`.
    pub placeholder: SharedString,
    pub items: Rc<ItemSearch>,
}
