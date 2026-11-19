//! Local-copy examples use explicit synthetic diagnostics, never the host's data.

use gpui_kit::{Context, Window};
use subspace_core::diagnostics::{
    BuildIdentity, ErrorDetails, LaunchKind, LocalDiagnostics, RuntimeDetails,
};
use subspace_dashboard::Tokens;
use subspace_dashboard::copy_details::CopyDetails;

use super::{Sample, sample};
use crate::ComponentGallery;

pub(super) fn samples(
    _: &ComponentGallery,
    _: &Tokens,
    _: &mut Window,
    _: &mut Context<ComponentGallery>,
) -> Vec<Sample> {
    vec![
        sample(
            "Error, activate to copy and confirm",
            CopyDetails::new(
                "gallery-sample:copy-details",
                ErrorDetails::new(
                    "Fixture service unavailable",
                    "The fixture request returned 503 Service Unavailable.",
                    "Fixture request",
                )
                .with_code("503"),
            )
            .with_diagnostics(diagnostics()),
        ),
        sample(
            "About, without an error",
            CopyDetails::diagnostics("gallery-sample:copy-diagnostics")
                .with_diagnostics(diagnostics()),
        ),
    ]
}

fn diagnostics() -> LocalDiagnostics {
    LocalDiagnostics::new(
        BuildIdentity::new("subspace", "0.0.0-fixture")
            .with_build("fixture", "fixture-commit")
            .with_platform("macos", "aarch64"),
        "0.0.0-fixture",
    )
    .with_os_version("fixture OS version")
    .with_runtime(RuntimeDetails::new(
        "/Applications/Fixture.app/Contents/MacOS/fixture",
        LaunchKind::Desktop,
        "fixture desktop PATH source",
        None,
    ))
}
