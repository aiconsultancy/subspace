//! Toolbar controls specific to a crew checkout.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{Context, Div, Entity, ParentElement as _, Styled as _, Window, px};
use subspace_client_fleet::diff::DiffMode;
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::segmented::{self, Segment};
use subspace_framework::dashboard::{PaletteCommand, Tokens, truncated};

use super::{BAR_PAD_PX, DiffView, source::FleetSource};

const MODES: [(&str, DiffMode); 3] = [
    ("All changes", DiffMode::Branch),
    ("Committed only", DiffMode::Committed),
    ("Uncommitted only", DiffMode::Working),
];

impl DiffView {
    pub(crate) fn crew_commands(view: &Entity<Self>) -> Vec<PaletteCommand> {
        let mut commands = Vec::from(MODES.map(|(label, mode)| {
            let view = view.downgrade();
            PaletteCommand::callback(label, move |window, cx| {
                _ = view.update(cx, |view, cx| view.set_mode(mode, Some(window), cx));
            })
        }));
        let view = view.downgrade();
        commands.push(PaletteCommand::callback(
            "Refresh crew diff",
            move |window, cx| {
                _ = view.update(cx, |view, cx| view.reload(window, cx));
            },
        ));
        commands
    }

    pub(super) fn fleet_bar(
        &self,
        source: &FleetSource,
        tokens: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let project = source
            .target
            .project
            .as_deref()
            .unwrap_or("Default project");
        let title = format!("{} · {project}", source.target.member);
        v_flex()
            .flex_shrink_0()
            .child(
                Self::bar(tokens, cx)
                    .child(truncated::text("crew-diff-title", title).min_w_0().flex_1())
                    .child(self.layout_toggles(tokens, cx))
                    .child(
                        action_button(
                            "crew-diff-refresh",
                            Intent::Neutral,
                            Some(IconName::RefreshCw),
                            "Refresh",
                        )
                        .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
                    ),
            )
            .child(
                h_flex()
                    .h(tokens.toolbar_height)
                    .px(px(BAR_PAD_PX))
                    .border_b_1()
                    .border_color(tokens.line)
                    .child(self.mode_control(tokens, cx)),
            )
            .children(self.fleet_notice(tokens, cx))
    }

    fn fleet_notice(&self, tokens: &Tokens, cx: &Context<Self>) -> Option<Div> {
        let message = self.fleet.banner()?;
        Some(
            h_flex()
                .gap_2()
                .flex_wrap()
                .px(px(BAR_PAD_PX))
                .text_color(tokens.muted)
                .child(format!("{message} ·"))
                .child(
                    action_button(
                        "crew-diff-banner-refresh",
                        Intent::Quiet,
                        Some(IconName::RefreshCw),
                        "Refresh",
                    )
                    .on_click(cx.listener(|view, _, window, cx| view.reload(window, cx))),
                ),
        )
    }

    fn mode_control(&self, tokens: &Tokens, cx: &Context<Self>) -> Div {
        let weak = cx.entity().downgrade();
        segmented::control(
            "crew-diff-mode",
            MODES.map(|(name, _)| Segment::new(name)),
            MODES
                .iter()
                .position(|(_, value)| *value == self.fleet.mode),
            tokens,
            move |index, _, window, cx| {
                if let Some((_, mode)) = MODES.get(index) {
                    _ = weak.update(cx, |view, cx| view.set_mode(*mode, Some(window), cx));
                }
            },
        )
    }

    pub(super) fn set_mode(
        &mut self,
        mode: DiffMode,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.fleet.mode != mode {
            self.close_binary(window, cx);
            self.fleet.mode = mode;
            self.load(cx);
        }
    }
}
