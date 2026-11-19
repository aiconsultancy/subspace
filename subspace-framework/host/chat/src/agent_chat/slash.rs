//! The `/` command list: typing `/` as the first character opens a fuzzy
//! list of Subspace commands and Copilot skills above the chat input.
//!
//! The filtering, ordering and completion are plain functions, so the panel
//! can test them without a window. [`SlashList`] shows the result in the
//! shared [`CompletionList`], which keeps the highlight.
//!
//! The keys come from the chat input's own context, which only matches
//! while the list is open, so the arrow keys keep moving the text cursor the
//! rest of the time.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Window, div,
};
use subspace_dashboard::chat_input::completion::{CompletionEvent, CompletionItem, CompletionList};

use super::ChatEngine;

/// Where a command comes from: Subspace handles its own, the runtime runs a
/// skill through `RunCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashKind {
    Subspace,
    Skill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    /// Without the leading slash, such as `new`.
    pub name: SharedString,
    pub description: SharedString,
    pub aliases: Vec<SharedString>,
    pub kind: SlashKind,
}

impl SlashCommand {
    pub fn new(
        name: impl Into<SharedString>,
        description: impl Into<SharedString>,
        kind: SlashKind,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            aliases: Vec::new(),
            kind,
        }
    }

    pub fn aliases(mut self, aliases: impl IntoIterator<Item = &'static str>) -> Self {
        self.aliases = aliases.into_iter().map(SharedString::from).collect();
        self
    }
}

/// The commands Subspace answers itself.
pub fn subspace_commands(engine: ChatEngine) -> Vec<SlashCommand> {
    let mut commands = vec![
        SlashCommand::new("new", "Start a new chat", SlashKind::Subspace)
            .aliases(["clear", "reset"]),
        SlashCommand::new("add-context", "Add the current item", SlashKind::Subspace),
        SlashCommand::new("sessions", "Switch saved session", SlashKind::Subspace),
        SlashCommand::new("model", "Choose the model", SlashKind::Subspace),
    ];
    if engine == ChatEngine::Copilot {
        commands.insert(
            1,
            SlashCommand::new("yolo on", "Approve every request", SlashKind::Subspace),
        );
        commands.insert(
            2,
            SlashCommand::new("yolo off", "Ask before each request", SlashKind::Subspace),
        );
    }
    commands
}

/// Subspace's commands first, then the runtime's skills.
pub fn catalog(engine: ChatEngine, skills: &[SlashCommand]) -> Vec<SlashCommand> {
    let mut commands = subspace_commands(engine);
    commands.extend(skills.iter().cloned());
    commands
}

/// The query behind a leading `/`, if the list should be open at all. Only a
/// first-character slash opens it, and a new line closes it again.
pub fn query(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('/')?;
    (!rest.contains('\n')).then_some(rest)
}

/// The positions of the matching commands: whole-prefix matches first, then
/// the rest, each keeping the catalog order.
pub fn filter(commands: &[SlashCommand], query: &str) -> Vec<usize> {
    let query = query.trim_start().to_lowercase();
    let mut prefixes = Vec::new();
    let mut others = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        match rank(command, &query) {
            Some(Rank::Prefix) => prefixes.push(index),
            Some(Rank::Fuzzy) => others.push(index),
            None => {}
        }
    }
    prefixes.extend(others);
    prefixes
}

enum Rank {
    Prefix,
    Fuzzy,
}

fn rank(command: &SlashCommand, query: &str) -> Option<Rank> {
    if query.is_empty() {
        return Some(Rank::Prefix);
    }
    let names = std::iter::once(&command.name).chain(command.aliases.iter());
    let mut fuzzy = false;
    for name in names {
        let name = name.to_lowercase();
        if name.starts_with(query) {
            return Some(Rank::Prefix);
        }
        fuzzy = fuzzy || subsequence(&name, query);
    }
    fuzzy.then_some(Rank::Fuzzy)
}

/// Whether every character of `query` appears in `name`, in order.
fn subsequence(name: &str, query: &str) -> bool {
    let mut chars = name.chars();
    query.chars().all(|wanted| chars.any(|c| c == wanted))
}

/// The text that replaces the input when a command is completed. A command
/// that takes arguments keeps the trailing space to type them after.
pub fn completion(command: &SlashCommand) -> String {
    format!("/{} ", command.name)
}

/// Subspace's own commands take no arguments, so choosing one runs it. A skill
/// is only inserted, so arguments can follow.
pub fn runs_on_choice(command: &SlashCommand) -> bool {
    command.kind == SlashKind::Subspace
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashEvent {
    /// Put this text in the input, and run it now when `run` is set.
    Complete {
        text: String,
        run: bool,
    },
    Close,
}

pub struct SlashList {
    engine: ChatEngine,
    commands: Vec<SlashCommand>,
    shown: Vec<usize>,
    open: bool,
    /// The shared list that shows the matches and keeps the highlight.
    list: Entity<CompletionList>,
    _subscription: Subscription,
}

impl EventEmitter<SlashEvent> for SlashList {}

impl SlashList {
    pub fn new(engine: ChatEngine, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let list = cx.new(|cx| CompletionList::new("chat-slash", window, cx));
        let subscription = cx.subscribe(&list, |slash, _, event, cx| match event {
            CompletionEvent::Accept(_) => slash.complete(cx),
        });
        Self {
            engine,
            commands: catalog(engine, &[]),
            shown: Vec::new(),
            open: false,
            list,
            _subscription: subscription,
        }
    }

    /// Replace the skills the runtime published. Subspace's own commands stay.
    pub fn set_skills(&mut self, skills: &[SlashCommand], cx: &mut Context<Self>) {
        self.commands = catalog(self.engine, skills);
        cx.notify();
    }

    /// Follow the chat input's text: open the list and filter it, or close.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        match query(text) {
            Some(query) => {
                self.shown = filter(&self.commands, query);
                self.open = true;
            }
            None => {
                self.shown.clear();
                self.open = false;
            }
        }
        self.show(cx);
    }

    /// Hands the matches to the shared list, or empties it while closed.
    fn show(&mut self, cx: &mut Context<Self>) {
        let items = match self.open {
            true => self
                .shown
                .iter()
                .filter_map(|&index| self.commands.get(index))
                .map(|command| CompletionItem::new(format!("/{}", command.name)))
                .collect(),
            false => Vec::new(),
        };
        self.list.update(cx, |list, cx| list.show(items, cx));
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open && !self.shown.is_empty()
    }

    pub fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.step(step, cx));
    }

    /// The highlighted command, if the list is open.
    pub fn highlighted(&self, cx: &App) -> Option<&SlashCommand> {
        let active = self.list.read(cx).active()?;
        self.is_open()
            .then(|| self.shown.get(active))
            .flatten()
            .and_then(|&index| self.commands.get(index))
    }

    pub fn complete(&mut self, cx: &mut Context<Self>) {
        let Some((text, run)) = self
            .highlighted(cx)
            .map(|command| (completion(command), runs_on_choice(command)))
        else {
            return;
        };
        self.open = false;
        self.show(cx);
        cx.emit(SlashEvent::Complete { text, run });
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.show(cx);
        cx.emit(SlashEvent::Close);
    }
}

impl Render for SlashList {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Hidden while closed, so the composer's gap stays as it was.
        div()
            .w_full()
            .when(!self.is_open(), |slash| slash.hidden())
            .child(self.list.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui_kit::TestAppContext;

    use super::ChatEngine;
    use super::{
        SlashCommand, SlashEvent, SlashKind, SlashList, catalog, completion, filter, query,
        runs_on_choice, subspace_commands,
    };

    #[gpui_kit::test]
    fn the_shared_list_moves_the_highlight_and_completes_that_command(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (slash, cx) =
            cx.add_window_view(|window, cx| SlashList::new(ChatEngine::Copilot, window, cx));
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&slash, move |_, event: &SlashEvent, _| {
                seen.borrow_mut().push(event.clone());
            })
            .detach();
        });
        slash.update(cx, |slash, cx| {
            slash.set_text("/yolo", cx);
            slash.step(1, cx);
            slash.complete(cx);
        });
        cx.run_until_parked();
        assert_eq!(
            *events.borrow(),
            [SlashEvent::Complete {
                text: "/yolo off ".into(),
                run: true
            }]
        );
        assert!(slash.read_with(cx, |slash, _| !slash.is_open()));
    }

    #[test]
    fn choosing_a_subspace_command_runs_it_and_a_skill_waits_for_arguments() {
        let yolo = SlashCommand::new("yolo on", "Approve every request", SlashKind::Subspace);
        let skill = SlashCommand::new("review", "Review a change", SlashKind::Skill);
        assert!(runs_on_choice(&yolo));
        assert!(!runs_on_choice(&skill));
    }

    fn names(commands: &[SlashCommand], shown: &[usize]) -> Vec<String> {
        shown
            .iter()
            .map(|&index| commands[index].name.to_string())
            .collect()
    }

    #[test]
    fn only_a_leading_slash_on_one_line_opens_the_list() {
        assert_eq!(query("/mo"), Some("mo"));
        assert_eq!(query("/"), Some(""));
        assert_eq!(query("ask /mo"), None);
        assert_eq!(query("/new\nmore"), None, "a new line closes the list");
    }

    #[test]
    fn the_filter_prefers_prefixes_and_still_matches_in_order() {
        let commands = subspace_commands(ChatEngine::Copilot);
        assert_eq!(filter(&commands, "").len(), commands.len());
        assert_eq!(names(&commands, &filter(&commands, "mod")), ["model"]);
        assert_eq!(
            names(&commands, &filter(&commands, "yolo")),
            ["yolo on", "yolo off"]
        );
        assert_eq!(
            names(&commands, &filter(&commands, "adc")),
            ["add-context"],
            "a subsequence matches"
        );
        assert!(filter(&commands, "zz").is_empty());
    }

    #[test]
    fn an_alias_matches_its_command() {
        let commands = subspace_commands(ChatEngine::Copilot);
        assert_eq!(names(&commands, &filter(&commands, "clear")), ["new"]);
        assert_eq!(names(&commands, &filter(&commands, "reset")), ["new"]);
    }

    #[test]
    fn skills_follow_subspaces_own_commands_and_match_too() {
        let skills = [SlashCommand::new(
            "review-pr",
            "Review a pull request",
            SlashKind::Skill,
        )];
        let commands = catalog(ChatEngine::Copilot, &skills);
        assert_eq!(
            commands.first().map(|command| command.name.as_ref()),
            Some("new")
        );
        assert_eq!(
            names(&commands, &filter(&commands, "review")),
            ["review-pr"]
        );
    }

    #[test]
    fn completion_leaves_room_for_arguments() {
        let command = SlashCommand::new("yolo on", "Approve every request", SlashKind::Subspace);
        assert_eq!(completion(&command), "/yolo on ");
    }

    #[test]
    fn pi_hides_copilot_permission_commands() {
        let commands = subspace_commands(ChatEngine::Pi);
        assert!(
            commands
                .iter()
                .all(|command| !command.name.starts_with("yolo"))
        );
        assert!(commands.iter().any(|command| command.name == "model"));
        assert!(commands.iter().any(|command| command.name == "add-context"));
    }
}
