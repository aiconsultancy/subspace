use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div,
};
use subspace_framework::dashboard::Tokens;

use crate::project::ResultUpdate;

pub(super) fn result_metadata(
    content: AnyElement,
    result: Option<&ResultUpdate>,
    id: &str,
    tokens: &Tokens,
) -> AnyElement {
    let Some(result) = result else { return content };
    let timestamp = i64::try_from(result.evaluated_at_ms)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map_or_else(
            || format!("{} ms since Unix epoch", result.evaluated_at_ms),
            |time| time.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
        );
    let metadata = format!("Scope: {} · Reported evaluation: {timestamp}", result.scope);
    v_flex()
        .min_w_0()
        .gap_1()
        .child(content)
        .child(
            div()
                .id(format!("{id}-metadata"))
                .test_support()
                .role(gpui_kit::Role::Paragraph)
                .aria_label(metadata.clone())
                .text_sm()
                .text_color(tokens.muted)
                .child(metadata),
        )
        .into_any_element()
}
