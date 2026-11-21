//! Hand-rolled one-offs that boards draw instead of a shared component, with
//! examples from the Subspace App's Spaces.
//!
//! The gallery does not copy board code. Each item names the board source (in
//! the Subspace App's repository for its Spaces) and how it differs, and its samples show the closest shared component with the
//! same fixture. The size and radius strip draws plain replicas of measured
//! values.

pub(super) mod controls;
pub(super) mod strip;
pub(super) mod surfaces;

use crate::catalog::Item;

pub(crate) const ITEMS: &[Item] = &[
    Item {
        name: "github::mode_switch",
        source: "Subspace App example: spaces/github/src/detail.rs",
        used_in: "Review composer mode tabs (Approve, Request changes, Comment) drawn as Divs with px_2, 2 px vertical padding, a 4 px literal radius and a select background when chosen, with no border, role or accessible label; closest shared: segmented::control.",
        samples: controls::mode_switch,
        kit: None,
    },
    Item {
        name: "space-lab::row_actions",
        source: "Subspace App example: spaces/space-lab/src/project_settings/view.rs",
        used_in: "Rows of Neutral action buttons with selected() and toggled() for one exclusive choice (Keep, Replace, Clear); the project browser marks the open project with selected() on Quiet action buttons; closest shared: segmented::control.",
        samples: controls::selected_button_row,
        kit: None,
    },
    Item {
        name: "github::check_run_link",
        source: "Subspace App example: spaces/github/src/detail/panel.rs",
        used_in: "Clickable accent arrow Div on check runs that calls links::open; closest shared: links::external.",
        samples: controls::check_run_link,
        kit: None,
    },
    Item {
        name: "inline-confirmations",
        source: "Subspace App example: spaces/web/src/manager/view.rs",
        used_in: "The Web manager's remove-board confirmation puts the question, a note, Keep board (Neutral) and Remove board (Reject) inline without a frame; closest shared: dialog::frame with dialog::footer.",
        samples: surfaces::inline_confirmation,
        kit: None,
    },
    Item {
        name: "inconsistencies::size_and_radius_strip",
        source: "spaces/component-gallery/src/samples/inconsistencies/strip.rs",
        used_in: "Plain replica boxes of the measured heights, radii and padding of one-offs and shared controls, side by side; the real one-offs stay in their boards.",
        samples: strip::size_and_radius,
        kit: None,
    },
];
