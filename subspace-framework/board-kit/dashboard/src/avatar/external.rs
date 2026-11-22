//! A person relayed from outside the conversation's own network, such as a
//! stakeholder whose words an agent carried in from another tool: initials
//! in a dashed outline, so they read as present only through someone else.

use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};

use super::SENDER_SIZE_PX;
use crate::Tokens;

/// How many initials an external person shows.
const INITIALS: usize = 2;

/// A relayed person: [`external`] builds it.
#[derive(IntoElement)]
pub struct ExternalAvatar {
    id: ElementId,
    name: SharedString,
}

/// A relayed person's avatar, as large as [`super::sender`] and the same
/// shape: their initials in muted text inside a dashed outline, with no
/// fill. `name` is the accessible name.
pub fn external(id: impl Into<ElementId>, name: impl Into<SharedString>) -> ExternalAvatar {
    ExternalAvatar {
        id: id.into(),
        name: name.into(),
    }
}

/// Up to two initials: the first letters of the first two words, or the
/// first two letters of a single word, in upper case.
pub(crate) fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let letters: String = match words.as_slice() {
        [] => String::new(),
        [only] => only.chars().take(INITIALS).collect(),
        several => several
            .iter()
            .take(INITIALS)
            .filter_map(|word| word.chars().next())
            .collect(),
    };
    letters.to_uppercase()
}

impl RenderOnce for ExternalAvatar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        let shown = initials(&self.name);
        div()
            .id(self.id)
            .test_support()
            .role(Role::Image)
            .aria_label(self.name)
            .size(px(SENDER_SIZE_PX))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(tokens.surface_radius)
            .border_1()
            .border_dashed()
            .border_color(tokens.muted)
            .text_size(tokens.secondary_size())
            .text_color(tokens.muted)
            .child(shown)
    }
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_come_from_the_first_two_words_or_one_word_s_first_letters() {
        assert_eq!(initials("Priya Raman"), "PR");
        assert_eq!(initials("dev okafor jr"), "DO");
        assert_eq!(initials("stakeholder"), "ST");
        assert_eq!(initials("  "), "");
        assert_eq!(initials("Ågot Ødegård"), "ÅØ");
    }
}
