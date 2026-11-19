//! One process owner for native responses, including windowless activation.

use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::{Notification, NotificationDelivery};
use gpui_kit::{
    AnyWindowHandle, App, Global, SharedString, Subscription, SystemNotification, Task, WeakEntity,
};
use subspace_dashboard::notifications::{self, Notice, Preferences, Sound, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    Board(&'static str, u64),
    Unavailable,
    Test,
}

const MAX_TARGETS: usize = 256;
const TARGET_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
struct Targets {
    entries: VecDeque<(String, Route, Instant)>,
    next: u64,
}
impl Targets {
    fn insert(&mut self, route: Route, now: Instant) -> String {
        self.next += 1;
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tag = format!("subspace-{epoch:x}-{:x}", self.next);
        self.entries.push_back((tag.clone(), route, now));
        while self.entries.len() > MAX_TARGETS {
            self.entries.pop_front();
        }
        tag
    }
    fn take(&mut self, tag: &str, now: Instant) -> Route {
        self.entries
            .retain(|(_, _, created)| now.saturating_duration_since(*created) < TARGET_LIFETIME);
        self.entries
            .iter()
            .position(|(key, _, _)| key == tag)
            .and_then(|index| self.entries.remove(index))
            .map(|(_, route, _)| route)
            .unwrap_or(Route::Unavailable)
    }
}

pub(crate) struct Runtime {
    targets: Targets,
    pub(crate) window: Option<(AnyWindowHandle, WeakEntity<crate::app::SubspaceApp>)>,
    pub(crate) pending: Option<Route>,
    _saver: Task<()>,
    _quit: Subscription,
}
impl Global for Runtime {}

pub(crate) fn init(cx: &mut App) {
    let saver = cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            cx.update(crate::app::save_prefs);
        }
    });
    let quit = cx.on_app_quit(|cx| {
        crate::app::save_prefs(cx);
        cx.global_mut::<Runtime>().targets.entries.clear();
        async {}
    });
    cx.set_global(Runtime {
        targets: Targets::default(),
        window: None,
        pending: None,
        _saver: saver,
        _quit: quit,
    });
    cx.set_global(notifications::Sink(request));
    // Kit init installs a window-bound handler. This intentionally replaces it:
    // app-owned native posts all use this router; Kit toasts are explicitly in-app.
    cx.on_system_notification_response(|response, cx| {
        respond(&response.tag, cx);
    });
}

pub(super) fn respond(tag: &str, cx: &mut App) {
    let target = cx.global_mut::<Runtime>().targets.take(tag, Instant::now());
    // Choosing an OS notification asks for the app, not only its window.
    cx.activate(true);
    route(target, cx);
}

pub(crate) fn register(target: Route, cx: &mut App) -> String {
    cx.global_mut::<Runtime>()
        .targets
        .insert(target, Instant::now())
}

pub(crate) fn route(target: Route, cx: &mut App) {
    cx.global_mut::<Runtime>().pending = Some(target);
    crate::ensure_product_window(cx);
}

fn request(source: &str, target: u64, notice: Notice, cx: &mut App) {
    let Some(source) = cx
        .global::<crate::registry::BoardFactories>()
        .0
        .iter()
        .find(|board| board.id == source && board.settings.notifications)
        .map(|board| board.id)
    else {
        return;
    };
    let preferences = notifications::preferences(source, cx);
    let target = Route::Board(source, target);
    if cx
        .global::<Runtime>()
        .window
        .as_ref()
        .is_some_and(|(_, app)| app.upgrade().is_some())
    {
        // A request can originate in a window event; update it after that borrow ends.
        cx.defer(move |cx| deliver(target, notice, notifications::preferences(source, cx), cx));
    } else {
        // No window means no retained toast, even if a window opens in the next turn.
        deliver_native(target, &notice, preferences, cx);
    }
}

pub(crate) fn deliver(target: Route, notice: Notice, preferences: Preferences, cx: &mut App) {
    let window = cx
        .global::<Runtime>()
        .window
        .clone()
        .filter(|(_, app)| app.upgrade().is_some());
    let (in_app, _, _) = preferences.channels(window.is_some());
    if in_app && let Some((window, _)) = window {
        let toast = toast(notice.text().clone(), preferences.style, target, cx);
        let _ = window.update(cx, |_, window, cx| window.push_notification(toast, cx));
    }
    deliver_native(target, &notice, preferences, cx);
}

pub(crate) fn preview(preferences: Preferences, window: &mut gpui_kit::Window, cx: &mut App) {
    if preferences.channels(true).0 {
        window.push_notification(
            toast(
                "This is a test notification.".into(),
                preferences.style,
                Route::Test,
                cx,
            ),
            cx,
        );
    }
    deliver_native(Route::Test, &Notice::new(GENERIC), preferences, cx);
}

/// The OS text of a notice that is not described.
const GENERIC: &str = "New activity is available.";

fn deliver_native(target: Route, notice: &Notice, preferences: Preferences, cx: &mut App) {
    let (_, system, sound) = preferences.channels(true);
    if system {
        let tag = register(target, cx);
        let (title, body) = system_text(notice, super::product_name(cx));
        super::system::post(
            SystemNotification {
                tag: tag.into(),
                title,
                body,
                actions: Vec::new(),
            },
            cx,
        );
    }
    if sound {
        subspace_dashboard::alerts::play_sound(preferences.sound.value(), cx);
    }
}

/// The OS notification's title and body: the board's description of who and
/// where, or the product name and generic text.
fn system_text(notice: &Notice, product: &str) -> (SharedString, SharedString) {
    match notice.system_body() {
        Some(body) => (notice.text().clone(), body.clone()),
        None => (SharedString::from(product.to_owned()), GENERIC.into()),
    }
}

fn toast(message: SharedString, style: Style, target: Route, cx: &App) -> Notification {
    let toast = Notification::new()
        .message(message)
        .delivery(NotificationDelivery::InApp)
        .autohide(style != Style::Persistent)
        .on_click(move |_, _, cx| {
            cx.defer(move |cx| route(target, cx));
        });
    if style == Style::Compact {
        toast
    } else {
        toast.title(super::product_name(cx).to_owned())
    }
}

pub(crate) fn preview_sound(sound: Sound) -> Result<(), &'static str> {
    if !sound.available() {
        return Err("This system sound is unavailable on this platform.");
    }
    if sound != Sound::Silent {
        subspace_dashboard::alerts::preview_sound(sound.value());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MAX_TARGETS, Route, TARGET_LIFETIME, Targets};
    use std::time::Instant;
    #[test]
    fn native_targets_are_bounded_single_use_and_expire_without_reconstruction() {
        let now = Instant::now();
        let mut targets = Targets::default();
        let old = targets.insert(Route::Board("fixture", 999), now);
        for _ in 0..MAX_TARGETS {
            targets.insert(Route::Test, now);
        }
        assert_eq!(targets.take(&old, now), Route::Unavailable);
        let current = targets.insert(Route::Board("fixture", 42), now);
        assert!(!current.contains("fixture"));
        assert_eq!(targets.take(&current, now), Route::Board("fixture", 42));
        assert_eq!(targets.take(&current, now), Route::Unavailable);
        let expired = targets.insert(Route::Test, now);
        assert_eq!(
            targets.take(&expired, now + TARGET_LIFETIME),
            Route::Unavailable
        );
        assert_eq!(
            targets.take("subspace-chat-42-completed", now),
            Route::Unavailable
        );
    }
}
