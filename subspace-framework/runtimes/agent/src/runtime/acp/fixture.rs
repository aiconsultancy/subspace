//! A fake ACP agent: a shell script that speaks just enough of the protocol
//! to drive a real actor, and the helpers that watch what it reports.
use super::process::Launch;
use super::{AcpActor, AcpActorInit, AcpFiles, decisions};
use crate::runtime::pi::fixture_tests::write_executable;
use crate::runtime::{EventQueue, lock};
use crate::{ChatCommand, ChatEvent, ChatPreferences, Command, Entry, Event};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use subspace_agent_tools::{Approval, ChatToolEndpoint, ChatToolServer};
use tokio::sync::mpsc;

const WAIT: Duration = Duration::from_secs(10);

/// The app's tools for the fixture: a fixed endpoint and token, and the
/// approvals the sessions were opened with.
pub(super) struct FakeTools {
    pub(super) instructions: &'static str,
    pub(super) approvals: std::sync::Mutex<Vec<Arc<dyn Approval>>>,
}

pub(super) const FAKE_TOKEN: &str = "test-only-chat-endpoint-token";

#[async_trait::async_trait]
impl ChatToolServer for FakeTools {
    async fn open(&self, approval: Arc<dyn Approval>) -> Result<ChatToolEndpoint, String> {
        lock(&self.approvals).push(approval);
        Ok(ChatToolEndpoint {
            url: "http://127.0.0.1:9/mcp".into(),
            token: subspace_core::Secret::new(FAKE_TOKEN),
            lease: Box::new(()),
        })
    }

    fn instructions(&self) -> String {
        self.instructions.to_owned()
    }
}

pub(super) const FAKE_AGENT: &str = r#"#!/bin/sh
reply() { printf '{"jsonrpc":"2.0","id":%s,"result":%s}\n' "$1" "$2"; }
update() { printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"fake-1","update":%s}}\n' "$1"; }
models() { printf '[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"%s","options":[{"value":"fast","name":"Fast"},{"value":"deep","name":"Deep"}]}]' "$1"; }
text() { update "{\"sessionUpdate\":\"$1\",\"content\":{\"type\":\"text\",\"text\":\"$2\"}}"; }
while IFS= read -r line; do
  id=$(printf '%s' "$line" | grep -o '"id":[0-9]*' | head -n 1 | cut -d: -f2)
  case "$line" in
    *'"method":"initialize"'*)
      reply "$id" '{"protocolVersion":1,"agentCapabilities":{"loadSession":true},"authMethods":[]}' ;;
    *'"method":"session/new"'*)
      case "$line" in *'"mcpServers":[]'*) ;; *) echo "unexpected MCP servers" >&2; exit 4 ;; esac
      reply "$id" "{\"sessionId\":\"fake-1\",\"configOptions\":$(models fast)}"
      update '{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"review","description":"Review changes"}]}' ;;
    *'"method":"session/load"'*)
      text user_message_chunk "Earlier question"
      text agent_message_chunk "Earlier answer"
      reply "$id" "{\"configOptions\":$(models fast)}" ;;
    *'"method":"session/set_config_option"'*)
      reply "$id" "{\"configOptions\":$(models deep)}" ;;
    *'"method":"session/cancel"'*)
      reply "$waiting" '{"stopReason":"cancelled"}' ;;
    *'"method":"session/prompt"'*'Wait'*)
      waiting=$id
      text agent_message_chunk "Working "
      update '{"sessionUpdate":"tool_call","toolCallId":"slow-1","title":"Long task","status":"in_progress"}' ;;
    *'"method":"session/prompt"'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":"fs-1","method":"fs/read_text_file","params":{"sessionId":"fake-1","path":"/etc/hosts"}}'
      IFS= read -r refused
      case "$refused" in *-32601*) text agent_thought_chunk "No file access. " ;; esac
      text agent_message_chunk "Writing "
      update '{"sessionUpdate":"tool_call","toolCallId":"call-1","title":"Write notes.txt","kind":"edit","status":"pending","rawInput":{"path":"notes.txt"}}'
      printf '%s\n' '{"jsonrpc":"2.0","id":"perm-1","method":"session/request_permission","params":{"sessionId":"fake-1","toolCall":{"toolCallId":"call-1","title":"Write notes.txt","rawInput":{"path":"notes.txt"}},"options":[{"optionId":"yes","name":"Allow","kind":"allow_once"},{"optionId":"no","name":"Reject","kind":"reject_once"}]}}'
      IFS= read -r answer
      case "$answer" in
        *'"optionId":"yes"'*) update '{"sessionUpdate":"tool_call_update","toolCallId":"call-1","status":"completed","content":[{"type":"content","content":{"type":"text","text":"wrote"}}]}' ;;
        *) update '{"sessionUpdate":"tool_call_update","toolCallId":"call-1","status":"failed"}' ;;
      esac
      text agent_message_chunk "done."
      reply "$id" '{"stopReason":"end_turn","usage":{"inputTokens":3,"outputTokens":5}}' ;;
  esac
done
"#;

pub(super) struct Fixture {
    /// The agent's working directory, where a fixture script can log.
    pub(super) directory: tempfile::TempDir,
    events: Arc<EventQueue>,
    commands: mpsc::Sender<Command>,
    pub(super) seen: Vec<Event>,
    cursor: usize,
}

pub(super) fn start(script: &str, saved_tabs: Option<&str>) -> Fixture {
    start_with_tools(script, saved_tabs, None)
}

/// As [`start`], with `tools` as the app's tools for each session.
pub(super) fn start_with_tools(
    script: &str,
    saved_tabs: Option<&str>,
    tools: Option<Arc<dyn ChatToolServer>>,
) -> Fixture {
    let (directory, actor, events) = actor(script, saved_tabs, tools);
    let (commands, receiver) = mpsc::channel(16);
    tokio::spawn(actor.listen(receiver, ChatPreferences::default()));
    Fixture {
        directory,
        events,
        commands,
        seen: Vec::new(),
        cursor: 0,
    }
}

/// The same real-process fixture with explicit actor driving and clock control.
pub(super) fn actor(
    script: &str,
    saved_tabs: Option<&str>,
    tools: Option<Arc<dyn ChatToolServer>>,
) -> (tempfile::TempDir, AcpActor, Arc<EventQueue>) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("fake-acp");
    write_executable(&executable, script);
    let files = AcpFiles {
        tabs: directory.path().join("tabs.json"),
        library: directory.path().join("library.json"),
        workspace: directory.path().to_path_buf(),
    };
    if let Some(tabs) = saved_tabs {
        std::fs::write(&files.tabs, tabs).unwrap();
    }
    let events = Arc::new(EventQueue::default());
    let actor = AcpActor::new(AcpActorInit {
        agent: "Fake agent",
        launch: Launch {
            executable,
            args: Vec::new(),
            env: BTreeMap::new(),
        },
        tools,
        files,
        decisions: decisions(events.clone(), Path::new("/no/such/workspace")),
        events: events.clone(),
    });
    (directory, actor, events)
}

impl Fixture {
    /// Waits for the first event after the last match that `wanted` accepts.
    pub(super) async fn wait_for(&mut self, wanted: impl Fn(&Event) -> bool) -> Event {
        tokio::time::timeout(WAIT, async {
            loop {
                let drained: Vec<Event> = lock(&self.events.items).drain(..).collect();
                self.events.space.notify_one();
                self.seen.extend(drained);
                let start = self.cursor;
                if let Some(index) = self.seen[start..].iter().position(&wanted) {
                    self.cursor = start + index + 1;
                    return self.seen[start + index].clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no matching event; saw {:#?}", self.seen))
    }
}

pub(super) async fn send(fixture: &Fixture, command: Command) {
    fixture.commands.send(command).await.unwrap();
}

pub(super) async fn say(fixture: &Fixture, text: &str) {
    let command = chat(ChatCommand::Send {
        text: text.into(),
        context: Vec::new(),
        images: Vec::new(),
    });
    send(fixture, command).await;
}

/// The rows the panel would show, confirmed rows replacing their id.
pub(super) fn rows(seen: &[Event]) -> Vec<Entry> {
    let mut rows: Vec<(String, Entry)> = Vec::new();
    for event in seen {
        if let Event::Chat {
            event: ChatEvent::Entry(entry),
            ..
        } = event
        {
            match rows.iter_mut().find(|(id, _)| *id == entry.id) {
                Some(row) => row.1 = entry.entry.clone(),
                None => rows.push((entry.id.clone(), entry.entry.clone())),
            }
        }
    }
    rows.into_iter().map(|(_, entry)| entry).collect()
}

pub(super) fn chat(command: ChatCommand) -> Command {
    Command::Chat { chat: 1, command }
}

pub(super) fn is_chat(event: &Event, wanted: impl Fn(&ChatEvent) -> bool) -> bool {
    matches!(event, Event::Chat { chat: 1, event } if wanted(event))
}

pub(super) fn ready(event: &Event) -> bool {
    is_chat(event, |event| matches!(event, ChatEvent::Ready { .. }))
}

pub(super) fn turn_done(event: &Event) -> bool {
    is_chat(event, |event| matches!(event, ChatEvent::TurnDone))
}

pub(super) async fn approve(fixture: &mut Fixture, allow: bool) -> String {
    let approval = fixture
        .wait_for(|event| is_chat(event, |event| matches!(event, ChatEvent::Approval { .. })))
        .await;
    let Event::Chat {
        event: ChatEvent::Approval {
            request_id,
            summary,
        },
        ..
    } = approval
    else {
        unreachable!()
    };
    send(fixture, Command::Approve { request_id, allow }).await;
    summary
}
