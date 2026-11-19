//! Start the SDK, switch to approve-all (YOLO) and back, and report what the
//! runtime confirms. Sends no prompt and writes to no connected service.
//! Select Copilot in Subspace Settings first. Run with
//! `mise exec -- cargo run -p subspace-agent --example access_probe`.
use std::sync::Arc;
use std::time::{Duration, Instant};

use subspace_agent::{AgentHandle, ChatCommand, ChatEvent, Command, Event};
use subspace_core::{config, credentials::Credentials};

const TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Waits for `Ready` or `Mode`, printing errors; `None` on timeout.
fn wait(agent: &AgentHandle, want_ready: bool) -> Option<Result<bool, String>> {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        for event in agent.take_events() {
            match event {
                Event::Chat {
                    event: ChatEvent::Ready { .. },
                    ..
                } if want_ready => return Some(Ok(true)),
                Event::Chat {
                    event: ChatEvent::Mode(yolo),
                    ..
                } if !want_ready => return Some(Ok(yolo)),
                Event::Chat {
                    event: ChatEvent::Error(error),
                    ..
                }
                | Event::Error(error) => return Some(Err(error)),
                _ => {}
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    None
}

fn main() -> Result<(), String> {
    let file = config::path().ok_or("No platform config directory")?;
    let (config, _) = config::load(&file);
    let credentials = Arc::new(Credentials::from_system(&config));
    // The probe needs no board tools; it only changes the access mode.
    let agent = AgentHandle::start(Arc::new(config), credentials, Default::default())?;
    let ready = wait(&agent, true);
    println!("startup: {ready:?}");
    for enabled in [true, false] {
        agent.send(Command::Chat {
            chat: 1,
            command: ChatCommand::SetYolo(enabled),
        })?;
        println!("SetYolo({enabled}): {:?}", wait(&agent, false));
    }
    agent.send(Command::Shutdown)?;
    Ok(())
}
