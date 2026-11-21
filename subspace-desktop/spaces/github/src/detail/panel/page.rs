//! The pull request page's reading sections, built from the panel's parts.

use gpui_kit::component::v_flex;
use gpui_kit::{AnyElement, IntoElement as _, Styled as _};
use subspace_framework::dashboard::Tokens;

use super::{PanelInput, activity, description, load_error};

/// The pull request page's reading sections: the load error, the rendered
/// description and the activity, without the action bars, which the page
/// keeps in its toolbar and the inspector's footer.
pub fn page_sections(input: &PanelInput<'_>, tokens: &Tokens) -> AnyElement {
    let key = input.item.key.clone();
    let body = v_flex().gap_1().text_size(tokens.row_font);
    let body = load_error(body, input, tokens);
    let body = description(body, input, tokens, &key);
    activity::events(body, input, tokens, &key).into_any_element()
}
