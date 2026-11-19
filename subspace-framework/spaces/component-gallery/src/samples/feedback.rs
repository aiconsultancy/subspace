//! Feedback: loading, empty, configuration and error states, and toasts.
//!
//! State boxes pad their top by 120 px and fill their parent, so each sample
//! sits in a fixed-height clipped box. Each `instance` is unique.

mod lines;

use gpui_kit::{Context, Window};
use subspace_dashboard::action::{self, Intent};
use subspace_dashboard::{DashboardEvent, Toast, Tokens, feedback};
use subspace_lucide::IconName;

pub(super) use self::lines::{empty_line, loading_line, skeleton};
use super::{Sample, bounded, ignore_click, sample};
use crate::ComponentGallery;
use crate::catalog::Item;

const FEEDBACK: &str = "board-kit/dashboard/src/feedback.rs";
const STATE_BOX: (f32, f32) = (560., 260.);
const LINE_BOX: (f32, f32) = (560., 120.);
const MESSAGE: &str = "The fixture service answered 503 Service Unavailable.";

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "copy_details::CopyDetails",
        source: "board-kit/dashboard/src/copy_details.rs",
        used_in: "Error details in chat failures, update errors and About diagnostics. Fixture diagnostics only in this gallery.",
        samples: super::copy_details::samples,
        kit: None,
    },
    Item {
        name: "github_sign_in::status",
        source: "board-kit/dashboard/src/github_sign_in/view.rs",
        used_in: "Sign in with GitHub in Settings (GitHub Spaces) and in chat when its account is missing or rejected. Fixture codes only in this gallery.",
        samples: super::github_sign_in::samples,
        kit: None,
    },
    Item {
        name: "feedback::skeleton",
        source: FEEDBACK,
        used_in: "First-load placeholders in GitHub diff and the shared list.",
        samples: skeleton,
        kit: None,
    },
    Item {
        name: "feedback::loading_line",
        source: FEEDBACK,
        used_in: "Inline loading text in Actions, GitHub and Web, and loading notes under shared list groups.",
        samples: loading_line,
        kit: None,
    },
    Item {
        name: "feedback::empty_line",
        source: FEEDBACK,
        used_in: "Empty notes under shared list groups, such as an Actions repository whose filter matches no run.",
        samples: empty_line,
        kit: None,
    },
    Item {
        name: "feedback::empty",
        source: FEEDBACK,
        used_in: "Empty states in Actions, GitHub diff and Terminal.",
        samples: empty,
        kit: None,
    },
    Item {
        name: "feedback::configuration",
        source: FEEDBACK,
        used_in: "The shared list's empty message with a hint.",
        samples: configuration,
        kit: None,
    },
    Item {
        name: "feedback::error_box",
        source: FEEDBACK,
        used_in: "Board-wide load failures in GitHub diff and Web.",
        samples: error_box,
        kit: None,
    },
    Item {
        name: "feedback::error_box_with_settings",
        source: FEEDBACK,
        used_in: "Shared list load failures that offer Configure.",
        samples: error_box_with_settings,
        kit: None,
    },
    Item {
        name: "feedback::error_box_with_actions",
        source: FEEDBACK,
        used_in: "Load failures with board-owned actions in Terminal.",
        samples: error_box_with_actions,
        kit: None,
    },
    Item {
        name: "feedback::error_banner",
        source: FEEDBACK,
        used_in: "Partial failures above content in Copilot Agents, GitHub diff and the shared list.",
        samples: error_banner,
        kit: None,
    },
    Item {
        name: "feedback::error_banner_with_actions",
        source: FEEDBACK,
        used_in: "The Actions run graph banner.",
        samples: error_banner_with_actions,
        kit: None,
    },
    Item {
        name: "Toast",
        source: "board-kit/dashboard/src/lib.rs",
        used_in: "Short results in every board through DashboardEvent::Toast; the shell draws the stack.",
        samples: toasts,
        kit: None,
    },
];

pub(super) fn empty(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = feedback::empty(
        tokens,
        "gallery-sample-empty",
        "Nothing to show",
        "No fixture rows match the filter.",
    );
    vec![sample("Empty", bounded(STATE_BOX, tokens, state))]
}

pub(super) fn configuration(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = feedback::configuration(
        tokens,
        "gallery-sample-configuration",
        "Not configured",
        "Add a fixture service in Settings.",
    );
    vec![sample(
        "Needs configuration",
        bounded(STATE_BOX, tokens, state),
    )]
}

pub(super) fn error_box(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = feedback::error_box(
        tokens,
        "gallery-sample-error-box",
        "Could not load",
        MESSAGE,
        ignore_click,
    );
    vec![sample(
        "Error with Retry",
        bounded(STATE_BOX, tokens, state),
    )]
}

pub(super) fn error_box_with_settings(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let state = feedback::error_box_with_settings(
        tokens,
        "gallery-sample-error-box-settings",
        "Could not sign in",
        MESSAGE,
        ignore_click,
    );
    vec![sample(
        "Error with Retry and Configure",
        bounded(STATE_BOX, tokens, state),
    )]
}

pub(super) fn error_box_with_actions(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let actions = [
        feedback::retry_action("gallery-sample:error-box-actions-retry", ignore_click),
        action::action_button(
            "gallery-sample:error-box-actions-open",
            Intent::Neutral,
            Some(IconName::ExternalLink),
            "Open logs",
        )
        .on_click(ignore_click),
    ];
    let state = feedback::error_box_with_actions(
        tokens,
        "gallery-sample-error-box-actions",
        "Could not load",
        MESSAGE,
        actions,
    );
    vec![sample(
        "Error with actions",
        bounded(STATE_BOX, tokens, state),
    )]
}

pub(super) fn error_banner(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let banner = feedback::error_banner(
        tokens,
        "gallery-sample-error-banner",
        "Some rows are missing",
        MESSAGE,
        ignore_click,
    );
    vec![sample(
        "Error with Retry",
        bounded(LINE_BOX, tokens, banner),
    )]
}

pub(super) fn error_banner_with_actions(
    _: &ComponentGallery,
    tokens: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let dismiss = action::action_button(
        "gallery-sample:error-banner-actions-dismiss",
        Intent::Quiet,
        Some(IconName::X),
        "Dismiss",
    )
    .on_click(ignore_click);
    let banner = feedback::error_banner_with_actions(
        tokens,
        "gallery-sample-error-banner-actions",
        "Some runs are missing",
        MESSAGE,
        [dismiss],
    );
    vec![sample(
        "Error with actions",
        bounded(LINE_BOX, tokens, banner),
    )]
}

pub(super) fn toasts(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    cx: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    let info = action::action_button(
        "gallery-sample:toast-info",
        Intent::Neutral,
        Some(IconName::Info),
        "Show info toast",
    )
    .on_click(cx.listener(|_, _, _, cx| {
        cx.emit(DashboardEvent::Toast(Toast::info(
            "Sample information toast from the Component gallery.",
        )));
    }));
    let error = action::action_button(
        "gallery-sample:toast-error",
        Intent::Neutral,
        Some(IconName::TriangleAlert),
        "Show error toast",
    )
    .on_click(cx.listener(|_, _, _, cx| {
        cx.emit(DashboardEvent::Toast(Toast::error(
            "Sample error toast from the Component gallery.",
        )));
    }));
    vec![sample("Info", info), sample("Error", error)]
}
