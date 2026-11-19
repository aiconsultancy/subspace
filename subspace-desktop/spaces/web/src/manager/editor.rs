use gpui_kit::component::input::InputState;
use gpui_kit::component::select::{SearchableVec, SelectState};
use gpui_kit::{App, AppContext as _, Context, Entity, Window};
use subspace_framework::core::settings::SettingsFile;

use super::model::SourceKind;
use super::{Manager, new_input};

pub(super) type Choice = Entity<SelectState<SearchableVec<String>>>;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct EditorSnapshot {
    id: String,
    title: String,
    location: String,
    reference: String,
    board_path: String,
    kind: SourceKind,
    release: Option<String>,
    prereleases: bool,
}

pub(super) struct Editor {
    pub(super) id: Entity<InputState>,
    pub(super) title: Entity<InputState>,
    pub(super) location: Entity<InputState>,
    pub(super) reference: Entity<InputState>,
    pub(super) board_path: Entity<InputState>,
    pub(super) kind: Choice,
    pub(super) releases: Choice,
    pub(super) release_ids: Vec<(String, u64, String)>,
    pub(super) prereleases: bool,
    pub(super) replacing: bool,
    pub(super) rename_only: bool,
}

impl Editor {
    pub(super) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            id: new_input("", window, cx),
            title: new_input("", window, cx),
            location: new_input("", window, cx),
            reference: new_input("", window, cx),
            board_path: new_input("", window, cx),
            kind: choice(
                SourceKind::ALL.map(|kind| kind.label().to_owned()).to_vec(),
                window,
                cx,
            ),
            releases: choice(Vec::new(), window, cx),
            release_ids: Vec::new(),
            prereleases: false,
            replacing: false,
            rename_only: false,
        }
    }

    pub(super) fn source_kind(&self, cx: &App) -> SourceKind {
        SourceKind::from_label(
            self.kind
                .read(cx)
                .selected_value()
                .map_or("", String::as_str),
        )
    }

    pub(super) fn identity(&self, cx: &App) -> (String, String) {
        (
            self.id.read(cx).value().trim().to_owned(),
            self.title.read(cx).value().trim().to_owned(),
        )
    }

    fn snapshot(&self, cx: &App) -> EditorSnapshot {
        let (id, title) = self.identity(cx);
        EditorSnapshot {
            id,
            title,
            location: self.location.read(cx).value().to_string(),
            reference: self.reference.read(cx).value().to_string(),
            board_path: self.board_path.read(cx).value().to_string(),
            kind: self.source_kind(cx),
            release: self.releases.read(cx).selected_value().cloned(),
            prereleases: self.prereleases,
        }
    }
}

pub(super) fn choice(items: Vec<String>, window: &mut Window, cx: &mut App) -> Choice {
    cx.new(|cx| {
        let selected = items.first().cloned();
        let mut state = SelectState::new(SearchableVec::new(items), None, window, cx);
        if let Some(selected) = selected {
            state.set_selected_value(&selected, window, cx);
        }
        state
    })
}

impl Manager {
    pub(super) fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel(cx);
        self.subscriptions.truncate(1);
        self.editor = Some(Editor::new(window, cx));
        self.watch_editor(cx);
        cx.notify();
    }

    pub(super) fn weather(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repository = cx
            .global::<subspace_framework::dashboard::CurrentProduct>()
            .0
            .product
            .release_repository
            .clone();
        self.add(window, cx);
        let Some(editor) = &self.editor else { return };
        set(&editor.id, "weather", window, cx);
        set(&editor.title, "Weather", window, cx);
        set(&editor.location, &repository, window, cx);
        set(
            &editor.board_path,
            "subspace-desktop/web-spaces/examples/weather",
            window,
            cx,
        );
        editor.kind.update(cx, |state, cx| {
            state.set_selected_value(&SourceKind::Repository.label().to_owned(), window, cx);
        });
    }

    pub(super) fn edit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.add(window, cx);
        let title = self
            .installed
            .iter()
            .find(|board| board.id == id)
            .map(|board| board.title.clone())
            .unwrap_or_else(|| id.to_owned());
        let Some(editor) = &mut self.editor else {
            return;
        };
        editor.replacing = true;
        editor.rename_only = true;
        set(&editor.id, id, window, cx);
        set(&editor.title, &title, window, cx);
    }

    pub(super) fn replace(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.editor {
            editor.rename_only = false;
        }
        self.pending = None;
        cx.notify();
    }

    fn watch_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        for input in [
            &editor.id,
            &editor.title,
            &editor.location,
            &editor.reference,
            &editor.board_path,
        ] {
            self.subscriptions.push(cx.subscribe(
                input,
                |this, _, event: &gpui_kit::component::input::InputEvent, cx| {
                    if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                        this.editor_changed(cx);
                    }
                },
            ));
        }
        self.subscriptions.push(cx.subscribe(
            &editor.kind,
            |this,
             _,
             event: &gpui_kit::component::select::SelectEvent<SearchableVec<String>>,
             cx| {
                if matches!(event, gpui_kit::component::select::SelectEvent::Confirm(_)) {
                    this.editor_changed(cx);
                }
            },
        ));
    }

    pub(super) fn capture_form(&mut self, cx: &App) {
        self.form_snapshot = self.editor.as_ref().map(|editor| editor.snapshot(cx));
    }

    pub(super) fn editor_changed(&mut self, cx: &mut Context<Self>) {
        let current = self.editor.as_ref().map(|editor| editor.snapshot(cx));
        if current == self.form_snapshot {
            return;
        }
        if current.as_ref().map(|value| &value.location)
            != self.form_snapshot.as_ref().map(|value| &value.location)
            && let Some(editor) = &mut self.editor
        {
            editor.release_ids.clear();
        }
        self.form_snapshot = current;
        self.epoch.invalidate();
        self.pending = None;
        self.offer = None;
        cx.notify();
    }

    pub(super) fn rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let (id, title) = editor.identity(cx);
        let result = super::config_path(cx).and_then(|path| rename(&path, &id, &title));
        match result {
            Ok(()) => self.after_save(window, cx),
            Err(error) => self.fail(error, cx),
        }
    }
}

fn rename(path: &std::path::Path, id: &str, title: &str) -> Result<(), String> {
    super::model::validate_title(title)?;
    let mut file = SettingsFile::open(path).map_err(|error| error.to_string())?;
    let entry = file
        .web_board(id)
        .ok_or("The board was removed. Reopen Settings.")?;
    file.update_web_board(id, Some(super::model::named_entry(entry, title)))
        .map_err(|error| error.to_string())
}

pub(super) fn set(input: &Entity<InputState>, text: &str, window: &mut Window, cx: &mut App) {
    input.update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
}
