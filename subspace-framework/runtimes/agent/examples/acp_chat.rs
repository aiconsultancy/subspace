//! Sends one prompt to an ACP agent through the real chat runtime and prints
//! what the panel would receive. For manual checks against installed agents:
//!
//! ```sh
//! cargo run -p subspace-agent --example acp_chat -- opencode "Say hello"
//! ```
//!
//! Approval cards are answered with `ACP_APPROVE` (`1` allows, anything else
//! denies). `ACP_AGENT_HOME` sets the agent's `HOME`, so Subspace's own state
//! can live in a throwaway `HOME` while the agent keeps its sign-in. The agent
//! also receives this example as its `subspace` MCP server; started as
//! `acp_chat mcp`, it exits at once, so the agent runs without board tools.
use std::sync::Arc;
use std::time::{Duration, Instant};
use subspace_agent::{AgentHandle, ChatCommand, ChatEvent, Command, Entry, Event};
use subspace_core::config::{AcpAgent, ChatEngine, Config};
use subspace_core::credentials::Credentials;
use subspace_core::features::ACP_CHAT;

const TIMEOUT: Duration = Duration::from_secs(300);
const POLL: Duration = Duration::from_millis(50);

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("mcp") {
        return Ok(());
    }
    let agent = first
        .as_deref()
        .and_then(AcpAgent::from_id)
        .ok_or("usage: acp_chat <opencode|claude|codex|gemini|custom> <prompt>")?;
    let prompt = args.collect::<Vec<_>>().join(" ");
    let handle = start(agent)?;
    let approve = std::env::var("ACP_APPROVE").as_deref() == Ok("1");
    drive(&handle, &prompt, approve)
}

fn start(agent: AcpAgent) -> Result<AgentHandle, String> {
    let mut config = Config::default();
    config.chat.engine = ChatEngine::Acp;
    config.chat.acp.agent = agent;
    config.features.insert(ACP_CHAT.id.into(), true);
    if let Ok(home) = std::env::var("ACP_AGENT_HOME") {
        config.chat.acp.env.insert("HOME".into(), home);
    }
    let credentials = Arc::new(Credentials::from_system(&config));
    AgentHandle::start(Arc::new(config), credentials, Default::default())
}

fn drive(handle: &AgentHandle, prompt: &str, approve: bool) -> Result<(), String> {
    let started = Instant::now();
    let mut sent = false;
    while started.elapsed() < TIMEOUT {
        for event in handle.take_events() {
            match show(event) {
                Step::Ready(chat) if !sent => {
                    sent = true;
                    let command = ChatCommand::Send {
                        text: prompt.to_owned(),
                        context: Vec::new(),
                        images: Vec::new(),
                    };
                    handle.send(Command::Chat { chat, command })?;
                }
                Step::Approval(request_id) => {
                    println!(
                        "[approval] answered {}",
                        if approve { "allow" } else { "deny" }
                    );
                    handle.send(Command::Approve {
                        request_id,
                        allow: approve,
                    })?;
                }
                Step::Done if sent => return Ok(()),
                Step::Failed(error) => return Err(error),
                _ => {}
            }
        }
        if handle.is_stopped() {
            return Err("The runtime stopped.".into());
        }
        std::thread::sleep(POLL);
    }
    Err("The agent did not finish within five minutes.".into())
}

enum Step {
    Ready(u64),
    Approval(String),
    Done,
    Failed(String),
    Other,
}

fn show(event: Event) -> Step {
    match event {
        Event::Chat { chat, event } => show_chat(chat, event),
        Event::Models(models) => {
            let ids: Vec<_> = models.iter().map(|model| model.id.as_str()).collect();
            println!("[models] {}", ids.join(", "));
            Step::Other
        }
        Event::Commands(commands) => {
            let names: Vec<_> = commands
                .iter()
                .map(|command| command.name.as_str())
                .collect();
            println!("[commands] {}", names.join(", "));
            Step::Other
        }
        Event::Error(error) => Step::Failed(error),
        Event::Notice(notice) => {
            println!("[notice] {notice}");
            Step::Other
        }
        _ => Step::Other,
    }
}

fn show_chat(chat: u64, event: ChatEvent) -> Step {
    match event {
        ChatEvent::Ready { session_id } => {
            println!("[ready] session {session_id}");
            return Step::Ready(chat);
        }
        ChatEvent::Delta { text, .. } => print!("{text}"),
        ChatEvent::Entry(entry) => show_entry(&entry.entry),
        ChatEvent::SelectedModel { model, .. } => println!("[model] {model}"),
        ChatEvent::Title(title) => println!("[title] {title}"),
        ChatEvent::Usage(usage) => println!("[usage] {usage:?}"),
        ChatEvent::Approval {
            request_id,
            summary,
        } => {
            println!("[approval] {summary}");
            return Step::Approval(request_id);
        }
        ChatEvent::Error(error) => println!("[error] {error}"),
        ChatEvent::TurnDone => {
            println!("[turn done]");
            return Step::Done;
        }
        _ => {}
    }
    Step::Other
}

fn show_entry(entry: &Entry) {
    match entry {
        Entry::Tool(call) => println!(
            "\n[tool] {} {} — {:?}: {}",
            call.title,
            call.detail,
            call.status,
            call.result.lines().next().unwrap_or_default()
        ),
        Entry::User { text, .. } => println!("[you] {text}"),
        Entry::Assistant(text) => println!("\n[reply] {text}"),
        Entry::Thinking { text, .. } => println!("\n[thought] {text}"),
        Entry::Info(text) => println!("\n[info] {text}"),
    }
}
