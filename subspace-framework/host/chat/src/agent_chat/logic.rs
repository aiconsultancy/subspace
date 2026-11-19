use super::slash::{SlashCommand as SlashEntry, SlashKind};
use super::toolbar::{ChatModeOption, ModelOption, NamedOption};
use super::{
    ChatEvent, ChatMode, Choice, ContextRef, Entry, EntryKind, Event, InputIntent,
    MAX_WIDTH_FRACTION, MIN_WIDTH, ModelChoice, Pending, Preference, SlashCommand, TranscriptEntry,
};
use gpui_kit::SharedString;

/// Subspace's own commands, which never reach the runtime as text.
const NEW_CHAT: [&str; 3] = ["/new", "/clear", "/reset"];

pub(super) fn input_intent(text: &str, skills: &[SlashCommand]) -> InputIntent {
    let text = text.trim();
    if !text.starts_with('/') {
        return InputIntent::Message(text.to_owned());
    }
    if let Some(name) = text.strip_prefix("/rename") {
        return InputIntent::Rename(name.trim().to_owned());
    }
    match text {
        "/add-context" => InputIntent::AddContext,
        "/close" => InputIntent::CloseChat,
        "/yolo on" => InputIntent::EnableYolo,
        "/yolo off" => InputIntent::DisableYolo,
        "/sessions" => InputIntent::OpenSessions,
        "/model" => InputIntent::OpenModels,
        text if NEW_CHAT.contains(&text) => InputIntent::NewChat,
        text => skill_intent(text, skills),
    }
}

/// A slash command is only run when the runtime listed it as a skill.
fn skill_intent(text: &str, skills: &[SlashCommand]) -> InputIntent {
    let (name, args) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let name = name.trim_start_matches('/');
    let Some(command) = skills
        .iter()
        .find(|skill| skill.name == name || skill.aliases.iter().any(|alias| alias == name))
    else {
        return InputIntent::UnknownCommand;
    };
    InputIntent::RunSkill {
        name: command.name.clone(),
        args: args.trim().to_owned(),
    }
}

pub(super) fn include_current(intent: &InputIntent, automatic: bool) -> bool {
    match intent {
        InputIntent::AddContext => true,
        InputIntent::Message(_) => automatic,
        _ => false,
    }
}

/// The models the toolbar lists.
pub(super) fn model_options(models: &[ModelChoice]) -> Vec<ModelOption> {
    models
        .iter()
        .map(|model| ModelOption {
            id: model.id.clone().into(),
            label: model.label.clone().into(),
            efforts: model.efforts.iter().map(SharedString::from).collect(),
        })
        .collect()
}

pub(super) fn agent_options(agents: &[Choice]) -> Vec<NamedOption> {
    agents
        .iter()
        .map(|agent| NamedOption {
            id: agent.id.clone().into(),
            label: agent.label.clone().into(),
        })
        .collect()
}

/// The runtime's skill commands, for the `/` list.
pub(super) fn skill_commands(commands: &[SlashCommand]) -> Vec<SlashEntry> {
    commands
        .iter()
        .map(|command| SlashEntry {
            name: command.name.clone().into(),
            description: command.description.clone().into(),
            aliases: command.aliases.iter().map(SharedString::from).collect(),
            kind: SlashKind::Skill,
        })
        .collect()
}

// qual:allow(dry, repeated_matches) reason: "The two directions of the same mode mapping; collapsing them would hide one direction."
pub(super) fn mode_option(mode: ChatMode) -> ChatModeOption {
    match mode {
        ChatMode::Interactive => ChatModeOption::Interactive,
        ChatMode::Plan => ChatModeOption::Plan,
        ChatMode::Autopilot => ChatModeOption::Autopilot,
    }
}

pub(super) fn sdk_mode(mode: ChatModeOption) -> ChatMode {
    match mode {
        ChatModeOption::Interactive => ChatMode::Interactive,
        ChatModeOption::Plan => ChatMode::Plan,
        ChatModeOption::Autopilot => ChatMode::Autopilot,
    }
}

/// The reasoning effort to send with `model`: kept only when that model
/// supports it, so the runtime never rejects the request.
pub(super) fn effort_for(
    models: &[ModelChoice],
    model: &str,
    effort: Option<&str>,
) -> Option<String> {
    models
        .iter()
        .find(|choice| choice.id == model)
        .and_then(|choice| choice.supported_effort(effort))
}

/// The last skill list in a batch of events, if it published one.
pub(super) fn latest_commands(events: &[Event]) -> Option<Vec<SlashCommand>> {
    events.iter().rev().find_map(|event| match event {
        Event::Commands(commands) => Some(commands.clone()),
        _ => None,
    })
}

/// Whether a batch of events reports a newly connected chat.
pub(super) fn became_ready(events: &[Event]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            Event::Chat {
                event: ChatEvent::Ready { .. },
                ..
            }
        )
    })
}

pub(super) fn context_snapshot(
    contexts: &[ContextRef],
    current: Option<ContextRef>,
    automatic: bool,
) -> Vec<ContextRef> {
    let mut contexts = contexts.to_vec();
    let _ = current
        .filter(|_| automatic)
        .map(|current| upsert_context(&mut contexts, current));
    contexts
}

/// Interim rows the runtime has not confirmed yet carry this id prefix.
const STREAM: &str = "stream-";

/// The label of a thinking row: its duration once the turn ends, and the
/// streaming state while reasoning still arrives.
pub(super) fn thinking_label(seconds: Option<u64>, streaming: bool) -> String {
    if streaming {
        return "Thinking…".to_owned();
    }
    match seconds {
        Some(seconds) => format!("Thought for {seconds}s"),
        None => "Thought".to_owned(),
    }
}

/// A reply shows its footer once the turn that produced it finished, so the
/// buttons do not flicker under streaming text.
pub(super) fn reply_is_complete(index: usize, entries: usize, busy: bool) -> bool {
    index + 1 < entries || !busy
}

/// Whether a row is still being streamed rather than confirmed by the runtime.
pub(super) fn is_streaming(id: &str) -> bool {
    id.starts_with(STREAM)
}

/// Whether a row shows the steering label: Copilot reported that it took the
/// message into a turn that was already running.
pub(super) fn is_steering(entry: &Entry) -> bool {
    matches!(entry, Entry::User { steering: true, .. })
}

/// The open row a delta extends, created from the kind the runtime reported.
fn stream_entry(kind: EntryKind, text: String) -> Entry {
    match kind {
        EntryKind::User => Entry::User {
            text,
            steering: false,
        },
        EntryKind::Info => Entry::Info(text),
        EntryKind::Thinking => Entry::Thinking {
            text,
            seconds: None,
        },
        _ => Entry::Assistant(text),
    }
}

/// A confirmed row replaces the interim stream of its kind, then updates the
/// row it addresses: a tool call by its `call_id`, anything else by its id.
fn upsert_entry(entries: &mut Vec<TranscriptEntry>, entry: TranscriptEntry) {
    entries.retain(|existing| !(is_streaming(&existing.id) && existing.kind() == entry.kind()));
    let existing = match entry.entry.tool() {
        Some(call) => entries.iter_mut().find(|existing| {
            existing
                .entry
                .tool()
                .is_some_and(|open| open.call_id == call.call_id)
        }),
        None => entries.iter_mut().find(|existing| existing.id == entry.id),
    };
    match existing {
        Some(existing) => *existing = entry,
        None => entries.push(entry),
    }
}

fn append_delta(entries: &mut Vec<TranscriptEntry>, kind: EntryKind, text: String) {
    if let Some(last) = entries
        .last_mut()
        .filter(|entry| entry.kind() == kind && is_streaming(&entry.id))
    {
        last.entry.push_text(&text);
        return;
    }
    entries.push(TranscriptEntry {
        id: format!("{STREAM}{}", entries.len()),
        entry: stream_entry(kind, text),
    });
}

pub(super) fn apply_transcript(entries: &mut Vec<TranscriptEntry>, event: Option<ChatEvent>) {
    match event {
        // Full SDK rows replace interim deltas, including dropped deltas.
        Some(ChatEvent::Entry(entry)) => upsert_entry(entries, entry),
        Some(ChatEvent::Delta { kind, text }) => append_delta(entries, kind, text),
        _ => {}
    }
}

pub(super) fn resolve_pending(pending: &mut Vec<Pending>, id: &str) {
    pending.retain(|pending| !matches!(pending,
        Pending::Approval { id: existing, .. } | Pending::Question { id: existing, .. } if existing == id));
}

pub(super) fn remove_reference(
    contexts: &mut Vec<ContextRef>,
    current: Option<&ContextRef>,
    automatic: &mut bool,
    key: &str,
) -> bool {
    contexts.retain(|context| context.key != key);
    let disable = *automatic && current.is_some_and(|current| current.key == key);
    *automatic &= !disable;
    disable
}

pub(super) fn clamped_width(width: f32, viewport: f32) -> f32 {
    width.clamp(MIN_WIDTH, (viewport * MAX_WIDTH_FRACTION).max(MIN_WIDTH))
}

pub(super) fn bool_preference(enabled: bool) -> &'static str {
    if enabled { "1" } else { "0" }
}

pub(super) fn change_preference(
    preference: Preference,
    tools: &mut bool,
    thinking: &mut bool,
    automatic: &mut bool,
) -> (&'static str, bool) {
    let (key, value) = match preference {
        Preference::Tools => ("chat.tools", tools),
        Preference::Thinking => ("chat.thinking", thinking),
        Preference::AutoContext => ("chat.auto-context", automatic),
    };
    *value = !*value;
    (key, *value)
}

pub(super) fn activity_label(busy: bool, pending: bool) -> &'static str {
    match (busy, pending) {
        (_, true) => "Chat !",
        (true, false) => "Chat …",
        _ => "Chat",
    }
}

pub(super) fn upsert_context(contexts: &mut Vec<ContextRef>, context: ContextRef) {
    if let Some(existing) = contexts
        .iter_mut()
        .find(|existing| existing.key == context.key)
    {
        *existing = context;
    } else {
        contexts.push(context);
    }
}

#[cfg(test)]
mod tests {
    use super::super::slash::SlashKind;
    use super::super::toolbar::ChatModeOption;
    use super::super::{InputIntent, Pending, Preference};
    use super::{
        activity_label, agent_options, apply_transcript, became_ready, bool_preference,
        change_preference, clamped_width, context_snapshot, effort_for, include_current,
        input_intent, is_steering, is_streaming, latest_commands, mode_option, model_options,
        remove_reference, reply_is_complete, resolve_pending, sdk_mode, skill_commands,
        thinking_label, upsert_context,
    };
    use subspace_agent::{
        ChatEvent, ChatMode, Choice, ContextRef, Entry, EntryKind, Event, ModelChoice,
        SlashCommand, ToolCall, ToolStatus, TranscriptEntry,
    };

    fn skills() -> Vec<SlashCommand> {
        vec![SlashCommand {
            name: "review-pr".into(),
            description: "Review a pull request".into(),
            aliases: vec!["pr".into()],
        }]
    }

    #[test]
    fn only_a_steered_user_message_carries_the_label() {
        assert!(is_steering(&Entry::User {
            text: "use the other branch".into(),
            steering: true
        }));
        assert!(!is_steering(&Entry::User {
            text: "start here".into(),
            steering: false
        }));
        assert!(
            !is_steering(&Entry::Assistant("on it".into())),
            "only the user's own messages are steered"
        );
    }

    #[test]
    fn slash_commands_and_messages_have_distinct_effects() {
        let skills = skills();
        let intent = |text: &str| input_intent(text, &skills);
        assert_eq!(intent(" /yolo on "), InputIntent::EnableYolo);
        assert_eq!(intent("/yolo off"), InputIntent::DisableYolo);
        assert_eq!(intent("/add-context"), InputIntent::AddContext);
        assert_eq!(intent("/sessions"), InputIntent::OpenSessions);
        assert_eq!(intent("/model"), InputIntent::OpenModels);
        assert_eq!(intent("/close"), InputIntent::CloseChat);
        assert_eq!(
            intent("/rename  Release notes "),
            InputIntent::Rename("Release notes".into())
        );
        assert_eq!(
            intent("/rename"),
            InputIntent::Rename(String::new()),
            "an empty name is refused by the runtime, not guessed at here"
        );
        for text in ["/new", "/clear", "/reset"] {
            assert_eq!(intent(text), InputIntent::NewChat, "{text}");
        }
        assert_eq!(intent("/unsupported"), InputIntent::UnknownCommand);
        assert_eq!(intent(" hello "), InputIntent::Message("hello".into()));
    }

    #[test]
    fn only_listed_skills_run_and_they_keep_their_arguments() {
        let skills = skills();
        assert_eq!(
            input_intent("/review-pr 42 --verbose", &skills),
            InputIntent::RunSkill {
                name: "review-pr".into(),
                args: "42 --verbose".into()
            }
        );
        assert_eq!(
            input_intent("/pr", &skills),
            InputIntent::RunSkill {
                name: "review-pr".into(),
                args: String::new()
            },
            "an alias runs its command"
        );
        assert_eq!(
            input_intent("/review-pr", &[]),
            InputIntent::UnknownCommand,
            "nothing runs while the runtime listed no skills"
        );
    }

    #[test]
    fn context_inclusion_follows_the_intent() {
        assert!(include_current(&InputIntent::AddContext, false));
        assert!(!include_current(
            &InputIntent::Message("hello".into()),
            false
        ));
        assert!(include_current(&InputIntent::Message("hello".into()), true));
        assert!(!include_current(&InputIntent::EnableYolo, true));
    }

    fn models() -> Vec<ModelChoice> {
        vec![
            ModelChoice {
                id: "gpt-5".into(),
                label: "GPT-5".into(),
                efforts: vec!["low".into(), "high".into()],
                default_effort: Some("low".into()),
            },
            ModelChoice {
                id: "haiku".into(),
                label: "Claude Haiku".into(),
                efforts: Vec::new(),
                default_effort: None,
            },
        ]
    }

    #[test]
    fn the_toolbar_lists_runtime_models_and_agents() {
        let options = model_options(&models());
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].label, "GPT-5");
        assert_eq!(options[0].efforts, ["low", "high"]);
        assert!(options[1].efforts.is_empty());
        let agents = agent_options(&[Choice {
            id: "subspace".into(),
            label: "Subspace".into(),
        }]);
        assert_eq!(agents[0].id, "subspace");
    }

    #[test]
    fn an_effort_is_only_sent_when_the_chosen_model_supports_it() {
        let models = models();
        assert_eq!(
            effort_for(&models, "gpt-5", Some("high")),
            Some("high".into())
        );
        assert_eq!(effort_for(&models, "gpt-5", Some("extreme")), None);
        assert_eq!(effort_for(&models, "haiku", Some("high")), None);
        assert_eq!(effort_for(&models, "gone", Some("high")), None);
        assert_eq!(effort_for(&models, "gpt-5", None), None);
    }

    #[test]
    fn modes_map_both_ways() {
        for mode in [ChatMode::Interactive, ChatMode::Plan, ChatMode::Autopilot] {
            assert_eq!(sdk_mode(mode_option(mode)), mode);
        }
        assert_eq!(mode_option(ChatMode::Plan), ChatModeOption::Plan);
    }

    #[test]
    fn the_slash_list_takes_the_runtimes_skills() {
        let commands = skill_commands(&skills());
        assert_eq!(commands[0].name, "review-pr");
        assert_eq!(commands[0].aliases, ["pr"]);
        assert_eq!(commands[0].kind, SlashKind::Skill);
    }

    #[test]
    fn a_batch_reports_the_latest_skills_and_a_new_session() {
        let events = vec![
            Event::Commands(skills()),
            Event::Commands(Vec::new()),
            Event::of(
                1,
                ChatEvent::Ready {
                    session_id: "1".into(),
                },
            ),
        ];
        let done = [Event::of(1, ChatEvent::TurnDone)];
        assert_eq!(latest_commands(&events), Some(Vec::new()));
        assert!(became_ready(&events));
        assert!(!became_ready(&done));
        assert_eq!(latest_commands(&done), None);
    }

    #[test]
    fn resolving_one_callback_preserves_other_pending_requests() {
        let mut pending = vec![
            Pending::Approval {
                id: "write:1".into(),
                summary: "Update task".into(),
            },
            Pending::Question {
                id: "question:1".into(),
                text: "Which task?".into(),
                choices: vec![],
            },
        ];
        resolve_pending(&mut pending, "write:1");
        assert_eq!(pending.len(), 1);
        assert!(matches!(&pending[0], Pending::Question { id, .. } if id == "question:1"));
    }

    #[test]
    fn removing_current_context_disables_auto_without_removing_other_references() {
        let current = ContextRef {
            key: "task:1".into(),
            label: "Task 1".into(),
            content: "current".into(),
        };
        let other = ContextRef {
            key: "file:1".into(),
            label: "File".into(),
            content: "file".into(),
        };
        let mut contexts = vec![current.clone(), other.clone()];
        let mut automatic = true;
        assert!(remove_reference(
            &mut contexts,
            Some(&current),
            &mut automatic,
            &current.key
        ));
        assert!(!automatic);
        assert_eq!(contexts, vec![other.clone()]);
        automatic = true;
        assert!(!remove_reference(
            &mut contexts,
            Some(&current),
            &mut automatic,
            &other.key
        ));
        assert!(automatic);
        assert!(contexts.is_empty());
    }

    #[test]
    fn automatic_context_uses_latest_snapshot_without_mutating_pinned_context() {
        let pinned = vec![ContextRef {
            key: "task:1".into(),
            label: "Task 1".into(),
            content: "old".into(),
        }];
        let current = ContextRef {
            content: "latest".into(),
            ..pinned[0].clone()
        };
        let automatic = context_snapshot(&pinned, Some(current.clone()), true);
        assert_eq!(automatic.len(), 1);
        assert_eq!(automatic[0].content, "latest");
        assert_eq!(context_snapshot(&pinned, Some(current), false), pinned);
        assert_eq!(pinned[0].content, "old");
    }

    #[test]
    fn authoritative_messages_replace_streams_and_duplicate_events() {
        let mut entries = Vec::new();
        for text in ["hel", "lo"] {
            apply_transcript(
                &mut entries,
                Some(ChatEvent::Delta {
                    kind: EntryKind::Assistant,
                    text: text.into(),
                }),
            );
        }
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry.text(), "hello");
        assert!(is_streaming(&entries[0].id));
        for text in ["hello world", "corrected"] {
            apply_transcript(
                &mut entries,
                Some(ChatEvent::Entry(TranscriptEntry {
                    id: "sdk:1".into(),
                    entry: Entry::Assistant(text.into()),
                })),
            );
        }
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "sdk:1");
        assert_eq!(entries[0].entry.text(), "corrected");
    }

    #[test]
    fn a_tool_call_keeps_one_row_from_start_to_completion() {
        let mut entries = Vec::new();
        let call = |status, result: &str| {
            Some(ChatEvent::Entry(TranscriptEntry {
                id: "tool:c1".into(),
                entry: Entry::Tool(ToolCall {
                    call_id: "c1".into(),
                    title: "Find the critical risks".into(),
                    detail: "github_search query=bug".into(),
                    status,
                    result: result.into(),
                }),
            }))
        };
        apply_transcript(&mut entries, call(ToolStatus::Running, ""));
        apply_transcript(&mut entries, call(ToolStatus::Done, "3 risks"));
        assert_eq!(entries.len(), 1, "start and completion share a row");
        let tool = entries[0].entry.tool().expect("a tool row");
        assert_eq!(tool.status, ToolStatus::Done);
        assert_eq!(tool.result, "3 risks");
        // A second call opens its own row even though it arrives with a new id.
        apply_transcript(
            &mut entries,
            Some(ChatEvent::Entry(TranscriptEntry {
                id: "tool:c2".into(),
                entry: Entry::Tool(ToolCall {
                    call_id: "c2".into(),
                    title: "Read".into(),
                    detail: String::new(),
                    status: ToolStatus::Running,
                    result: String::new(),
                }),
            })),
        );
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn reasoning_deltas_stream_into_one_thinking_row() {
        let mut entries = Vec::new();
        for text in ["step ", "one"] {
            apply_transcript(
                &mut entries,
                Some(ChatEvent::Delta {
                    kind: EntryKind::Thinking,
                    text: text.into(),
                }),
            );
        }
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].entry,
            Entry::Thinking {
                text: "step one".into(),
                seconds: None
            }
        );
        apply_transcript(
            &mut entries,
            Some(ChatEvent::Entry(TranscriptEntry {
                id: "r1".into(),
                entry: Entry::Thinking {
                    text: "step one, step two".into(),
                    seconds: Some(49),
                },
            })),
        );
        assert_eq!(entries.len(), 1, "the confirmed row replaces the stream");
        assert_eq!(entries[0].id, "r1");
    }

    #[test]
    fn thinking_rows_read_their_state_and_replies_wait_for_the_turn() {
        assert_eq!(thinking_label(Some(49), false), "Thought for 49s");
        assert_eq!(thinking_label(None, false), "Thought");
        assert_eq!(thinking_label(Some(49), true), "Thinking…");
        assert_eq!(thinking_label(None, true), "Thinking…");
        assert!(
            reply_is_complete(0, 2, true),
            "an earlier reply is already finished"
        );
        assert!(!reply_is_complete(1, 2, true), "the last reply still runs");
        assert!(reply_is_complete(1, 2, false));
    }

    #[test]
    fn width_limits_leave_room_for_dashboard_and_allow_keyboard_steps() {
        assert_eq!(clamped_width(100., 1000.), 300.);
        assert_eq!(clamped_width(900., 1000.), 600.);
        assert_eq!(clamped_width(420., 1000.), 420.);
        assert_eq!(clamped_width(420., 200.), 300.);
    }

    #[test]
    fn preference_toggles_are_independent_and_never_persist_yolo() {
        let (mut tools, mut thinking, mut automatic) = (false, false, true);
        assert_eq!(
            change_preference(Preference::Tools, &mut tools, &mut thinking, &mut automatic),
            ("chat.tools", true)
        );
        assert_eq!(
            change_preference(
                Preference::Thinking,
                &mut tools,
                &mut thinking,
                &mut automatic
            ),
            ("chat.thinking", true)
        );
        assert_eq!(
            change_preference(
                Preference::AutoContext,
                &mut tools,
                &mut thinking,
                &mut automatic
            ),
            ("chat.auto-context", false)
        );
        assert_eq!(bool_preference(true), "1");
        assert_eq!(bool_preference(false), "0");
    }

    #[test]
    fn status_labels_distinguish_activity() {
        assert_eq!(activity_label(false, false), "Chat");
        assert_eq!(activity_label(true, false), "Chat …");
        assert_eq!(activity_label(false, true), "Chat !");
        assert_eq!(activity_label(true, true), "Chat !");
    }

    #[test]
    fn latest_snapshot_replaces_duplicate_context() {
        let mut contexts = vec![];
        for content in ["before", "after"] {
            upsert_context(
                &mut contexts,
                ContextRef {
                    key: "board:1".into(),
                    label: "Task 1".into(),
                    content: content.into(),
                },
            );
        }
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].content, "after");
    }
}
