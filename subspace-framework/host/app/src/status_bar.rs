//! The window's bottom bar: the active dashboard's status on the left,
//! the chat engine's mark and state and the main keys on the right.

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, TestSupportExt as _, div, px,
};
use subspace_dashboard::action::{Intent, action_button};
use subspace_dashboard::icon_button::{IconButton, compact_glyph};
use subspace_dashboard::tokens::{self, Tokens};
use subspace_lucide::{IconName, SvgIcon};

#[cfg(feature = "report-issue")]
use crate::app::OpenReportIssue;
use crate::app::{OpenAbout, ToggleHelp, TogglePalette, palette_key};

/// Clears the rounded bottom corners of a macOS window.
const CORNER_INSET: f32 = 12.;

/// What the bar shows; the shell fills it from the dashboard and the chat.
pub struct StatusFacts {
    pub status: Option<SharedString>,
    pub loading: bool,
    /// The selected chat engine's name, such as `Copilot` or `Codex`.
    pub chat_engine: &'static str,
    /// The selected chat engine's mark.
    pub chat_icon: SvgIcon,
    pub chat_disabled: bool,
    pub copilot_busy: bool,
    pub copilot_attention: bool,
    /// The chat runtime failed to start or stopped.
    pub copilot_unavailable: bool,
    pub product_version: String,
    /// Only a newer release that is currently available to install.
    pub available_update: Option<SharedString>,
    /// Whether the app offers Report issue (it names a repository).
    #[cfg(feature = "report-issue")]
    pub report_issue: bool,
}

/// The chat engine's state in words: a stopped runtime matters most, then a
/// pending approval, then a turn.
pub fn copilot_state(engine: &str, busy: bool, attention: bool, unavailable: bool) -> String {
    let state = match (busy, attention, unavailable) {
        (_, _, true) => "unavailable",
        (_, true, false) => "needs you",
        (true, false, false) => "is working",
        (false, false, false) => "ready",
    };
    format!("{engine} {state}")
}

pub fn render(facts: StatusFacts, tokens: &Tokens) -> impl IntoElement {
    let copilot = if facts.chat_disabled {
        "Chat off".into()
    } else {
        copilot_state(
            facts.chat_engine,
            facts.copilot_busy,
            facts.copilot_attention,
            facts.copilot_unavailable,
        )
    };
    let colour = if facts.copilot_unavailable {
        tokens.red
    } else if facts.copilot_attention {
        tokens.yellow
    } else {
        tokens.muted
    };
    let commands = commands(&facts);
    let bar = StatusBar::new()
        .px(px(CORNER_INSET))
        .text_color(tokens.muted)
        .left(
            h_flex()
                .gap_2()
                .items_center()
                .when(facts.loading, |left| {
                    left.child(Spinner::new().color(tokens.muted))
                })
                .children(facts.status),
        )
        .right(chat_status(facts.chat_icon, copilot, colour, tokens));
    commands.into_iter().fold(bar, StatusBar::right)
}

/// About, Report issue (when offered), Commands and Help, in that order.
pub fn commands(facts: &StatusFacts) -> Vec<IconButton> {
    let version = version_button(
        facts.product_version.clone(),
        facts.available_update.clone(),
    );
    std::iter::once(version)
        .chain(report_issue_command(facts))
        .chain(main_commands())
        .collect()
}

#[cfg(feature = "report-issue")]
fn report_issue_command(facts: &StatusFacts) -> Option<IconButton> {
    facts.report_issue.then(|| {
        status_command(
            "status-report-issue",
            Some(IconName::FaceSlightlyFrowning),
            "Report issue",
            OpenReportIssue,
        )
    })
}

#[cfg(not(feature = "report-issue"))]
fn report_issue_command(_: &StatusFacts) -> Option<IconButton> {
    None
}

/// Commands and Help.
fn main_commands() -> [IconButton; 2] {
    [
        status_command(
            "status-commands",
            // The shortcut label already contains the platform modifier symbol.
            None,
            format!("{} Commands", palette_key()),
            TogglePalette,
        ),
        status_command(
            "status-help",
            Some(IconName::CircleQuestionMark),
            "Help",
            ToggleHelp,
        ),
    ]
}

/// The engine's mark before its state, both in the state's colour, sized
/// like the compact status commands beside it.
fn chat_status(icon: SvgIcon, state: String, colour: Hsla, tokens: &Tokens) -> impl IntoElement {
    let (glyph, gap) = compact_glyph(tokens);
    h_flex()
        .id("status-chat")
        .test_support()
        .gap(gap)
        .text_size(tokens::rem(tokens.controls.compact_button.font))
        .text_color(colour)
        .child(
            div()
                .id("status-chat-mark")
                .test_support()
                .flex_shrink_0()
                .size(glyph)
                .child(Icon::from(icon).size(glyph)),
        )
        .child(state)
}

/// Every status bar command is compact and flat in every state and theme,
/// whatever the theme's toolbar look.
fn status_command(
    id: &'static str,
    icon: Option<IconName>,
    label: impl Into<SharedString>,
    action: impl Action,
) -> IconButton {
    let button = action_button(id, Intent::Quiet, icon, label)
        .compact()
        .flat();
    button.on_click(move |_, window, cx| {
        window.dispatch_action(action.boxed_clone(), cx);
    })
}

fn version_button(version: String, available: Option<SharedString>) -> IconButton {
    let label = available.as_ref().map_or_else(
        || format!("About · {version}"),
        |update| format!("About · {version} · Update {update} available"),
    );
    status_command("status-version", Some(IconName::Info), version, OpenAbout)
        .accessibility_label(label.clone())
        .tooltip(label)
        .when(available.is_some(), |button| {
            button.icon(IconName::Download).child("Update available")
        })
}

#[cfg(test)]
mod tests {
    use super::{StatusFacts, commands, copilot_state};
    use subspace_core::config::AcpAgent;
    use subspace_dashboard::density::Density;
    use subspace_dashboard::icon_button::{Surface, paint};
    use subspace_dashboard::tokens::{Controls, Tokens, ToolbarLook};
    use subspace_lucide::IconName;

    /// A theme that puts toolbar buttons on glass in every state.
    const GLASS: &str = r#"{"--liquid-glass":"1","--toolbar-button-rest":"glass","--toolbar-button-hover":"glass"}"#;

    #[test]
    fn every_status_command_is_flat_whatever_the_toolbar_look() {
        let glass = Controls::from_density(&Density::from_json(GLASS));
        for available in [None, Some("v0.3.0".into())] {
            let facts = facts(available, true);
            for command in commands(&facts) {
                assert_eq!(command.surface(&glass), Surface::Quiet(ToolbarLook::FLAT));
            }
        }
        let tokens = Tokens::with_density(&Density::from_json(GLASS));
        let glyph = tokens.fg;
        for on in [false, true] {
            let flat = paint(ToolbarLook::FLAT, on, &tokens, glyph);
            assert!(!flat.lit && !flat.rims, "never glass");
            assert_eq!(flat.edge.a, 0., "never a border or outline");
            assert!(flat.pressed.a > flat.hover.a);
        }
        let rest = paint(ToolbarLook::FLAT, false, &tokens, glyph);
        assert_eq!(rest.rest.a, 0., "no fill at rest");
        assert_eq!(rest.hover, tokens.button_hover, "the flat hover fill");
    }

    #[cfg_attr(not(feature = "report-issue"), allow(unused_variables))]
    fn facts(available_update: Option<gpui_kit::SharedString>, report_issue: bool) -> StatusFacts {
        StatusFacts {
            status: None,
            loading: false,
            chat_engine: "Copilot",
            chat_icon: IconName::Bot,
            chat_disabled: false,
            copilot_busy: false,
            copilot_attention: false,
            copilot_unavailable: false,
            product_version: "v0.2.1".into(),
            available_update,
            #[cfg(feature = "report-issue")]
            report_issue,
        }
    }

    /// About, Report issue only when the app offers it, Commands, Help.
    #[test]
    fn the_status_bar_lists_report_issue_only_when_offered() {
        let count = |report_issue| commands(&facts(None, report_issue)).len();
        assert_eq!(count(false), 3);
        assert_eq!(
            count(true),
            if cfg!(feature = "report-issue") { 4 } else { 3 }
        );
    }

    #[test]
    fn acp_agents_are_reported_by_their_product_names() {
        let ready =
            AcpAgent::ALL.map(|agent| copilot_state(agent.engine_label(), false, false, false));
        assert_eq!(
            ready,
            [
                "OpenCode ready",
                "Claude ready",
                "Codex ready",
                "Gemini ready",
                "ACP agent ready"
            ]
        );
    }

    #[test]
    fn a_pending_approval_outranks_a_running_turn() {
        assert_eq!(
            copilot_state("Copilot", false, false, false),
            "Copilot ready"
        );
        assert_eq!(
            copilot_state("Copilot", true, false, false),
            "Copilot is working"
        );
        assert_eq!(
            copilot_state("Copilot", true, true, false),
            "Copilot needs you"
        );
        assert_eq!(copilot_state("Pi", false, true, false), "Pi needs you");
    }

    #[test]
    fn a_stopped_runtime_is_never_reported_ready() {
        assert_eq!(
            copilot_state("Copilot", false, false, true),
            "Copilot unavailable"
        );
        assert_eq!(
            copilot_state("OpenCode", true, true, true),
            "OpenCode unavailable"
        );
    }
}
