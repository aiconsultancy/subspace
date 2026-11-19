use super::{Settings, note};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{Context, Div, ParentElement as _, Styled as _};
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::{Tokens, anchored};
use subspace_lucide::IconName;

pub(super) fn sounds(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    v_flex().gap_3()
        .child(subspace_dashboard::toggle::toggle_row(
            "Play notification sounds",
            subspace_dashboard::toggle::switch("notification-sounds", cx)
                .checked(settings.notifications.sounds)
                .on_click(cx.listener(|settings, enabled: &bool, _, cx| {
                    settings.set_notification(crate::notifications::NotificationChoice::Sounds, *enabled, cx);
                })),
        ))
        .child(note(tokens, "Off mutes every notification sound: Copilot and every Space. Each Space's own Sound choice is kept for when sounds are on."))
}

pub(super) fn render(settings: &Settings, tokens: &Tokens, cx: &mut Context<Settings>) -> Div {
    v_flex().gap_4().children(settings.sources.iter().enumerate().map(|(index, source)| {
        let mut row = v_flex().gap_2().child(source.title.clone())
            .child("Delivery").child(anchored::select(&source.mode).title_prefix("Delivery: "))
            .child("In-app style").child(anchored::select(&source.style).title_prefix("Style: "))
            .child("Sound").child(anchored::select(&source.sound).title_prefix("Sound: "))
            .child(h_flex().gap_2()
                .child(action_button(("preview-source-sound", index), Intent::Neutral, Some(IconName::Volume2), "Preview sound")
                    .on_click(cx.listener(move |settings, _, window, cx| settings.preview_notification(index, true, window, cx))))
                .child(action_button(("test-source-notification", index), Intent::Neutral, Some(IconName::Bell), "Test notification")
                    .on_click(cx.listener(move |settings, _, window, cx| settings.preview_notification(index, false, window, cx)))));
        if source.unavailable_sound {
            row = row.child(note(tokens, "The saved sound is invalid or unavailable. Silent is selected."));
        }
        row.child(note(tokens, "Preview sound plays even when notification sounds are off. Test notification follows the saved Play notification sounds choice. Neither saves. OS appearance, Focus and retention are controlled by System Settings. Only available system sounds are listed."))
    }))
}
