//! A one-slot progress channel keeps network work off the UI thread and bounded.
use super::{UpdateFailure, UpdatePanel, display_error, updates::UpdateEvent};
use gpui_kit::{AppContext as _, Context};
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;
use subspace_update::{Installed, Release, UpdateProgress, Updater};

const PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(100);

enum Message {
    Progress(UpdateProgress),
    Finished(Result<Installed, UpdateFailure>),
}

pub(super) fn start(updater: Updater, release: Release, cx: &mut Context<UpdatePanel>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    cx.background_spawn(async move {
        let result = updater.install_with_progress(&release, |progress| {
            if matches!(progress, UpdateProgress::Downloading { .. }) {
                let _ = sender.try_send(Message::Progress(progress));
            } else {
                let _ = sender.send(Message::Progress(progress));
            }
        });
        let _ = sender.send(Message::Finished(result.map_err(display_error)));
    })
    .detach();
    cx.spawn(async move |panel, cx| {
        loop {
            let message = match receiver.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty) => {
                    cx.background_executor().timer(PROGRESS_POLL_INTERVAL).await;
                    continue;
                }
                Err(TryRecvError::Disconnected) => Message::Finished(Err("The update worker stopped before reporting a result. Check for updates to retry.".into())),
            };
            let finished = matches!(&message, Message::Finished(_));
            let updated = panel.update(cx, |panel, cx| match message {
                Message::Progress(progress) => panel.apply_progress(progress, cx),
                Message::Finished(result) => panel.apply(UpdateEvent::InstallResult(result), cx),
            });
            if finished || updated.is_err() {
                return;
            }
        }
    }).detach();
}
