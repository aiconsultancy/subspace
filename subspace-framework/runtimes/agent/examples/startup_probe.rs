//! Verify SDK session readiness, sandbox policy and model discovery without a
//! prompt. Select Copilot in Subspace Settings first. Run with `mise exec -- cargo run -p subspace-agent --example startup_probe`.
use std::sync::Arc;
use std::time::{Duration, Instant};

use subspace_agent::{AgentHandle, ChatEvent, Command, Event};
use subspace_core::{config, credentials::Credentials};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Default)]
struct StartupProgress {
    ready: bool,
    models: bool,
}

impl StartupProgress {
    fn observe(&mut self, events: Vec<Event>) -> Result<bool, String> {
        for event in events {
            match event {
                Event::Chat {
                    event: ChatEvent::Ready { .. },
                    ..
                } => self.ready = true,
                Event::Models(_) => self.models = true,
                Event::Chat {
                    event: ChatEvent::Error(error),
                    ..
                }
                | Event::Error(error) => return Err(error),
                _ => {}
            }
        }
        Ok(self.ready && self.models)
    }
}

fn main() -> Result<(), String> {
    let file = config::path().ok_or("No platform config directory")?;
    let (config, notices) = config::load(&file);
    if !notices.is_empty() {
        return Err("Service configuration has notices; inspect it in Subspace first".into());
    }
    let credentials = Arc::new(Credentials::from_system(&config));
    // The probe needs no board tools; it only starts a session.
    let agent = AgentHandle::start(Arc::new(config), credentials, Default::default())?;
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut progress = StartupProgress::default();
    loop {
        match progress.observe(agent.take_events()) {
            Ok(true) => {
                println!(
                    "SDK session and model list ready with default sandbox policy; no prompt sent."
                );
                agent.send(Command::Shutdown)?;
                return Ok(());
            }
            Err(error) => {
                let _ = agent.send(Command::Shutdown);
                return Err(error);
            }
            Ok(false) => {}
        }
        if Instant::now() >= deadline {
            let _ = agent.send(Command::Shutdown);
            return Err("SDK session or model discovery timed out without sending a prompt".into());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::StartupProgress;
    use subspace_agent::{ChatEvent, Event};

    fn ready() -> Event {
        Event::Chat {
            chat: 1,
            event: ChatEvent::Ready {
                session_id: "test-session".into(),
            },
        }
    }

    #[test]
    fn readiness_waits_for_models_and_reports_a_later_failure() {
        let mut progress = StartupProgress::default();
        assert!(!progress.observe(vec![ready()]).unwrap());
        assert_eq!(
            progress.observe(vec![Event::Error("model discovery failed".into())]),
            Err("model discovery failed".into())
        );
    }

    #[test]
    fn readiness_requires_both_events_in_either_order() {
        for (first, second) in [
            (ready(), Event::Models(Vec::new())),
            (Event::Models(Vec::new()), ready()),
        ] {
            let mut progress = StartupProgress::default();
            assert!(!progress.observe(vec![first]).unwrap());
            assert!(progress.observe(vec![second]).unwrap());
        }
    }

    #[test]
    fn an_error_in_the_ready_batch_is_not_hidden_by_success_events() {
        let mut progress = StartupProgress::default();
        assert_eq!(
            progress.observe(vec![
                ready(),
                Event::Models(Vec::new()),
                Event::Error("startup failed".into()),
            ]),
            Err("startup failed".into())
        );
    }
}
