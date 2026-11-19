//! Native banners share the sound policy without changing their activation route.

use gpui_kit::{App, SystemNotification};

pub(super) fn post(notification: SystemNotification, cx: &mut App) {
    let silent = !subspace_dashboard::notifications::sounds_enabled(cx);
    #[cfg(test)]
    if cx.has_global::<TestPosts>() {
        cx.global_mut::<TestPosts>().0.push((notification, silent));
        return;
    }
    #[cfg(target_os = "linux")]
    if silent {
        post_silent(notification, cx);
        return;
    }
    // Kit's macOS adapter sets no sound; Windows emits <audio silent="true"/>.
    // Linux needs an explicit suppress-sound hint when globally muted.
    let _ = silent;
    cx.show_system_notification(notification);
}

#[cfg(target_os = "linux")]
fn silent_banner(notification: &SystemNotification) -> notify_rust::Notification {
    let mut banner = notify_rust::Notification::new();
    banner
        .summary(&notification.title)
        .body(&notification.body)
        .hint(notify_rust::Hint::SuppressSound(true))
        .action("default", "Open");
    banner.finalize()
}

#[cfg(target_os = "linux")]
fn post_silent(notification: SystemNotification, cx: &mut App) {
    let banner = silent_banner(&notification);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("silent-notification".into())
        .spawn(move || {
            let _ = sender.send(show_banner(banner));
        });
    if let Err(error) = spawned {
        eprintln!("subspace: could not post silent notification: {error}");
        return;
    }
    cx.spawn(async move |cx| {
        if receiver.await == Ok(true) {
            cx.update(|cx| super::runtime::respond(&notification.tag, cx));
        }
    })
    .detach();
}

#[cfg(target_os = "linux")]
fn show_banner(banner: notify_rust::Notification) -> bool {
    let mut activated = false;
    match banner.show() {
        Ok(handle) => handle.wait_for_action(|action| activated = action == "default"),
        Err(error) => eprintln!("subspace: could not post silent notification: {error}"),
    }
    activated
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct TestPosts(pub Vec<(SystemNotification, bool)>);
#[cfg(test)]
impl gpui_kit::Global for TestPosts {}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #[test]
    fn linux_muted_banners_explicitly_suppress_sound() {
        let notification = gpui_kit::SystemNotification {
            tag: "fixture".into(),
            title: "Activity".into(),
            body: "A fixture changed.".into(),
            actions: Vec::new(),
        };
        let banner = super::silent_banner(&notification);
        assert_eq!(banner.summary, "Activity");
        assert_eq!(banner.body, "A fixture changed.");
        assert!(
            banner
                .hints
                .contains(&notify_rust::Hint::SuppressSound(true))
        );
    }
}
