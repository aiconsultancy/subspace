use super::super::{ChatId, mpsc};
use super::SessionMessage;
use github_copilot_sdk::subscription::{EventSubscription, RecvErrorKind};
use tokio::task::JoinHandle;

pub(super) fn forward(
    chat: ChatId,
    mut subscription: EventSubscription,
    sender: mpsc::Sender<SessionMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let event = subscription.recv().await;
            let closed =
                matches!(&event, Err(error) if !matches!(error.kind(), RecvErrorKind::Lagged(_)));
            if sender.send((chat, event)).await.is_err() || closed {
                return;
            }
        }
    })
}
