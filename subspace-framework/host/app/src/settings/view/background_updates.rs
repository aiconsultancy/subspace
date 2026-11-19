use gpui_kit::component::Disableable as _;
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, Context, Div, IntoElement as _, ParentElement as _, Styled as _, div, rems,
};
use subspace_dashboard::toggle::toggle_row;
use subspace_dashboard::{Tokens, anchored};

use super::{Settings, SettingsPage, note};

/// The rem width of one interval label character, generous for digits, and
/// the trigger's padding, gap and chevron around the label.
const INTERVAL_CHAR_REM: f32 = 0.55;
const INTERVAL_CHROME_REM: f32 = 3.;

impl Settings {
    /// The Background updates switch and the Update interval select of a
    /// board page, one per column; `None` when the board has no background
    /// work. `locked` disables both while the board is turned off.
    pub(in crate::settings) fn background_update_controls(
        &self,
        page: &SettingsPage,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> Option<[AnyElement; 2]> {
        let SettingsPage::Board(id) = page else {
            return None;
        };
        let index = self
            .background_updates
            .iter()
            .position(|draft| draft.id == *id)?;
        let draft = &self.background_updates[index];
        let toggle = toggle_row(
            "Background updates",
            subspace_dashboard::toggle::switch(("background-updates", index), cx)
                .checked(draft.enabled)
                .disabled(locked)
                .on_click(cx.listener(move |settings, enabled: &bool, _, cx| {
                    settings.background_updates[index].enabled = *enabled;
                    cx.notify();
                })),
        )
        .items_center();
        let interval = h_flex()
            .w_full()
            .gap_3()
            .child(div().flex_1().min_w_0().child("Update interval"))
            .child(
                div()
                    .w(rems(interval_width_rems(draft.labels())))
                    .flex_shrink_0()
                    .child(
                        anchored::select(&draft.interval)
                            .accessibility_label("Background update interval")
                            .disabled(locked),
                    ),
            );
        Some([toggle.into_any_element(), interval.into_any_element()])
    }
}

/// What background updates do, shown under their row.
pub(in crate::settings) fn background_update_notes(tokens: &Tokens) -> [Div; 2] {
    [
        note(
            tokens,
            "Save applies without restart. Off pauses automatic reads, including notification and task polling. Manual refresh and opening this Space still load data.",
        ),
        note(
            tokens,
            "Default keeps each background task's existing interval. The global automatic-refresh setting must also be enabled.",
        ),
    ]
}

/// A select just wide enough for its longest label.
fn interval_width_rems<'a>(labels: impl Iterator<Item = &'a str>) -> f32 {
    let longest = labels.map(|label| label.chars().count()).max().unwrap_or(0);
    longest as f32 * INTERVAL_CHAR_REM + INTERVAL_CHROME_REM
}

#[cfg(test)]
mod tests {
    use super::interval_width_rems;

    #[test]
    fn the_interval_select_fits_its_longest_label() {
        let standard = interval_width_rems(["Default", "30 seconds", "1 hour"].into_iter());
        assert_eq!(standard, interval_width_rems(["30 seconds"].into_iter()));
        assert!(standard > interval_width_rems(["1 hour"].into_iter()));
        assert!(interval_width_rems(["45 seconds (saved)"].into_iter()) > standard);
    }
}
