//! The approvals and questions one chat is waiting for. Every pending
//! callback remembers its chat, so an answer reaches the tab that asked.
use super::{APPROVAL_LIMIT, APPROVAL_TIMEOUT, EventQueue, PathBuf, lock, next_id, redact};
use crate::{ChatEvent, ChatId, Event};
use async_trait::async_trait;
#[cfg(feature = "copilot")]
use github_copilot_sdk::handler::{UserInputHandler, UserInputResponse};
#[cfg(feature = "copilot")]
use github_copilot_sdk::types::{RequestId, SessionId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub(super) enum Decision {
    Approval(oneshot::Sender<bool>),
    /// Only Copilot asks the user questions.
    #[cfg_attr(not(feature = "copilot"), allow(dead_code))]
    Answer(oneshot::Sender<String>),
}

/// One pending callback and the chat that is waiting for it.
pub(super) struct Pending {
    pub(super) chat: ChatId,
    pub(super) decision: Decision,
}

pub(super) struct Decisions {
    pub(super) events: Arc<EventQueue>,
    pub(super) pending: Mutex<HashMap<String, Pending>>,
    /// Which chat each open session belongs to, so an SDK callback reaches
    /// the tab that asked for it. Copilot only.
    #[cfg_attr(not(feature = "copilot"), allow(dead_code))]
    pub(super) chats: Mutex<HashMap<String, ChatId>>,
    /// Subspace's private workspace, canonical. Built-in reads inside it need
    /// no prompt; the CLI runs `view`, `glob` and `grep` in its own process,
    /// so a read anywhere else is a host read and is shown. Copilot only.
    #[cfg_attr(not(feature = "copilot"), allow(dead_code))]
    pub(super) workspace: PathBuf,
}

/// The Copilot SDK's callbacks name a session, not a chat.
#[cfg(feature = "copilot")]
impl Decisions {
    pub(super) fn attach(&self, session: &SessionId, chat: ChatId) {
        lock(&self.chats).insert(session.to_string(), chat);
    }

    pub(super) fn detach(&self, session: &SessionId) {
        lock(&self.chats).remove(&session.to_string());
    }

    pub(super) fn chat_of(&self, session: &SessionId) -> Option<ChatId> {
        lock(&self.chats).get(&session.to_string()).copied()
    }
}

impl Decisions {
    pub(super) async fn approval(
        &self,
        chat: ChatId,
        id: String,
        summary: String,
    ) -> Result<bool, String> {
        if subspace_agent_tools::secrets::looks_like_credential(&summary)
            || self.events.has_secret(&summary)
        {
            return Err(
                "The proposed operation includes credential-like data and cannot be approved."
                    .into(),
            );
        }
        let (sender, receiver) = oneshot::channel();
        self.register(id.clone(), chat, Decision::Approval(sender))?;
        self.events
            .push(Event::of(
                chat,
                ChatEvent::Approval {
                    request_id: id.clone(),
                    summary: redact(&summary),
                },
            ))
            .await;
        let result = tokio::time::timeout(APPROVAL_TIMEOUT, receiver).await;
        lock(&self.pending).remove(&id);
        Ok(matches!(result, Ok(Ok(true))))
    }

    fn register(&self, id: String, chat: ChatId, decision: Decision) -> Result<(), String> {
        let mut pending = lock(&self.pending);
        if pending.len() >= APPROVAL_LIMIT || pending.contains_key(&id) {
            return Err("Too many pending approval requests.".into());
        }
        pending.insert(id, Pending { chat, decision });
        Ok(())
    }

    pub(super) fn approve(&self, id: &str, allow: bool) {
        if let Some(Pending {
            decision: Decision::Approval(sender),
            ..
        }) = lock(&self.pending).remove(id)
        {
            let _ = sender.send(allow);
        }
    }

    #[cfg_attr(not(feature = "copilot"), allow(dead_code))]
    pub(super) fn answer(&self, id: &str, answer: String) {
        if let Some(Pending {
            decision: Decision::Answer(sender),
            ..
        }) = lock(&self.pending).remove(id)
        {
            let _ = sender.send(answer);
        }
    }

    /// Drops the requests of one chat, leaving the other chats waiting.
    pub(super) fn cancel(&self, chat: ChatId) {
        lock(&self.pending).retain(|_, pending| pending.chat != chat);
    }

    pub(super) fn cancel_all(&self) {
        lock(&self.pending).clear();
    }
}

/// The write approvals of one chat. Each session is given its own, so a tool
/// call's approval reaches the tab that started the turn.
pub(super) struct ChatApproval {
    pub(super) decisions: Arc<Decisions>,
    pub(super) chat: ChatId,
}

#[async_trait]
impl subspace_agent_tools::Approval for ChatApproval {
    async fn approve(&self, summary: String) -> Result<bool, String> {
        self.decisions
            .approval(self.chat, format!("service-{}", next_id()), summary)
            .await
    }

    async fn approve_in_app(&self, summary: String) -> Result<bool, String> {
        // Always use the manual card, including when SDK approve-all is on.
        self.approve(summary).await
    }
}

#[cfg(feature = "copilot")]
#[async_trait]
impl UserInputHandler for Decisions {
    async fn handle(
        &self,
        session: SessionId,
        _request: Option<RequestId>,
        question: String,
        choices: Option<Vec<String>>,
        _freeform: Option<bool>,
    ) -> Option<UserInputResponse> {
        let chat = self.chat_of(&session)?;
        let id = format!("question-{}", next_id());
        let (sender, receiver) = oneshot::channel();
        self.register(id.clone(), chat, Decision::Answer(sender))
            .ok()?;
        let choices = choices.unwrap_or_default();
        self.events
            .push(Event::of(
                chat,
                ChatEvent::Question {
                    request_id: id.clone(),
                    question: redact(&question),
                    choices: choices.clone(),
                },
            ))
            .await;
        let answer = tokio::time::timeout(APPROVAL_TIMEOUT, receiver).await;
        lock(&self.pending).remove(&id);
        answer.ok()?.ok().map(|answer| UserInputResponse {
            was_freeform: !choices.contains(&answer),
            answer,
        })
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "copilot")]
    use super::UserInputHandler;
    use super::{ChatApproval, Decisions};
    use crate::runtime::{EventQueue, lock};
    use crate::{ChatEvent, Event};
    use std::sync::{Arc, Mutex};
    use subspace_agent_tools::Approval as _;

    #[tokio::test]
    async fn mandatory_in_app_approval_waits_for_the_exact_manual_decision() {
        let events = Arc::new(EventQueue::default());
        let decisions = Arc::new(Decisions {
            events: events.clone(),
            pending: Mutex::new(Default::default()),
            chats: Mutex::new(Default::default()),
            workspace: Default::default(),
        });
        for allow in [false, true] {
            let approval = ChatApproval {
                decisions: decisions.clone(),
                chat: 19,
            };
            let task = tokio::spawn(async move {
                approval
                    .approve_in_app("Fixture config diff. A backup will be made.".into())
                    .await
            });
            let request = wait_for_manual_request(&events).await;
            decisions.approve("unrelated-request", true);
            assert!(!task.is_finished());
            decisions.approve(&request, allow);
            assert_eq!(task.await.unwrap().unwrap(), allow);
        }
        assert!(lock(&decisions.pending).is_empty());
    }

    async fn wait_for_manual_request(events: &EventQueue) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let event = lock(&events.items).pop_front();
                if let Some(Event::Chat {
                    chat,
                    event:
                        ChatEvent::Approval {
                            request_id,
                            summary,
                        },
                }) = event
                {
                    assert_eq!(chat, 19);
                    assert!(summary.contains("backup"));
                    return request_id;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }

    #[cfg(feature = "copilot")]
    #[tokio::test]
    async fn session_questions_route_answers_by_request_and_detached_sessions_ask_nothing() {
        let events = Arc::new(EventQueue::default());
        let decisions = Arc::new(Decisions {
            events: events.clone(),
            pending: Mutex::new(Default::default()),
            chats: Mutex::new(Default::default()),
            workspace: Default::default(),
        });
        let session = "fixture-session".into();
        decisions.attach(&session, 12);
        let callback = decisions.clone();
        let task = tokio::spawn(async move {
            callback
                .handle(
                    "fixture-session".into(),
                    None,
                    "Which fixture?".into(),
                    Some(vec!["first".into(), "second".into()]),
                    Some(true),
                )
                .await
        });
        let request = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let event = lock(&events.items).pop_front();
                if let Some(Event::Chat {
                    chat,
                    event:
                        ChatEvent::Question {
                            request_id,
                            choices,
                            ..
                        },
                }) = event
                {
                    assert_eq!(chat, 12);
                    assert_eq!(choices, ["first", "second"]);
                    break request_id;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        decisions.answer("another-request", "first".into());
        assert!(!task.is_finished());
        decisions.answer(&request, "second".into());
        let result = task.await.unwrap().unwrap();
        assert_eq!(result.answer, "second");
        assert!(!result.was_freeform);
        decisions.detach(&session);
        assert!(
            decisions
                .handle(session, None, "No longer open".into(), None, None)
                .await
                .is_none()
        );
        assert!(lock(&events.items).is_empty());
    }
}
