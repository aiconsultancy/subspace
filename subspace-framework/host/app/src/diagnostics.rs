//! Explicit local-copy and issue profiles share build identities, never app state.

#[cfg(feature = "report-issue")]
pub(crate) mod issue;
mod runtime;

pub(crate) use runtime::local_copy;

use subspace_core::diagnostics::BuildIdentity;
use subspace_product::{Channel, OsKind, ProductContext};

fn build_identity(context: &ProductContext) -> BuildIdentity {
    BuildIdentity::new(context.product.id.as_str(), &context.version)
        .with_build(report_channel(), context.build.commit_label())
        .with_platform(os_name(context.env.os), std::env::consts::ARCH)
}

fn report_channel() -> &'static str {
    match (cfg!(debug_assertions), Channel::current()) {
        (true, Channel::Stable) => "dev",
        (_, channel) => channel.as_str(),
    }
}

fn os_name(os: OsKind) -> &'static str {
    match os {
        OsKind::Macos => "macos",
        OsKind::Windows => "windows",
        OsKind::Linux => "linux",
    }
}
