//! Theme and font select items for Settings > Appearance. Each item keeps its
//! name as the searchable, accessible title and the saved value.

use gpui_kit::component::select::{SearchableVec, SelectDelegate, SelectItem};
use gpui_kit::component::{Icon, IndexPath, Sizable as _, h_flex, try_parse_color};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, Hsla, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled as _, Task, Window, div,
};
use subspace_dashboard::Tokens;
use subspace_lucide::IconName;

use crate::theme::{self, AUTOMATIC_FONT};

/// Theme colour keys for each swatch, in preference order. Subspace tones come
/// first, as in `Tokens::with_tones`, then GPUI Kit semantic and base colours.
const PALETTE_KEYS: [&[&str]; 8] = [
    &["background"],
    &["foreground"],
    &["accent", "primary.background"],
    &["red", "danger.background", "base.red"],
    &["yellow", "warning.background", "base.yellow"],
    &["green", "success.background", "base.green"],
    &["blue", "base.blue", "info.background"],
    &["magenta", "base.magenta", "base.cyan"],
];

/// The first parseable colour for each palette slot; slots a theme does not
/// define are left out.
pub(super) fn palette(lookup: impl Fn(&str) -> Option<String>) -> Vec<Hsla> {
    PALETTE_KEYS
        .iter()
        .filter_map(|keys| {
            keys.iter()
                .filter_map(|key| lookup(key))
                .find_map(|text| try_parse_color(&text).ok())
        })
        .collect()
}

/// A theme name with a palette read from that theme's own document.
#[derive(Clone)]
pub(super) struct ThemeItem {
    name: String,
    palette: Vec<Hsla>,
}

impl ThemeItem {
    pub(super) fn new(name: String, palette: Vec<Hsla>) -> Self {
        Self { name, palette }
    }

    fn label(&self, checked: Option<bool>) -> ThemeLabel {
        ThemeLabel {
            name: self.name.clone().into(),
            palette: self.palette.clone(),
            checked,
        }
    }
}

impl SelectItem for ThemeItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.label(None).into_any_element())
    }

    fn value(&self) -> &String {
        &self.name
    }
}

/// Every registered theme with its palette, in catalogue order.
pub(super) fn theme_items(cx: &App) -> Vec<ThemeItem> {
    theme::theme_names(cx)
        .into_iter()
        .map(|name| {
            let colors = theme::document(&name, cx)
                .map(|document| palette(|key| document.color(key)))
                .unwrap_or_default();
            ThemeItem::new(name, colors)
        })
        .collect()
}

/// A searchable theme list whose rows align the palette to the right edge.
/// The default row sizes its content to the name, so it cannot do this.
pub(super) struct ThemeList(SearchableVec<ThemeItem>);

impl ThemeList {
    pub(super) fn new(items: Vec<ThemeItem>) -> Self {
        Self(SearchableVec::new(items))
    }
}

impl SelectDelegate for ThemeList {
    type Item = ThemeItem;

    fn items_count(&self, section: usize) -> usize {
        self.0.items_count(section)
    }

    fn item(&self, ix: IndexPath) -> Option<&ThemeItem> {
        self.0.item(ix)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        ThemeItem: SelectItem<Value = V>,
        V: PartialEq,
    {
        self.0.position(value)
    }

    fn perform_search(&mut self, query: &str, window: &mut Window, cx: &mut App) -> Task<()> {
        self.0.perform_search(query, window, cx)
    }

    fn render_item(
        &self,
        _: IndexPath,
        item: &ThemeItem,
        checked: bool,
        _: &mut Window,
        _: &mut App,
    ) -> Option<AnyElement> {
        Some(item.label(Some(checked)).into_any_element())
    }
}

/// The name on the left and decorative swatches on the right. List rows pass
/// `checked` to draw the selection mark the custom row replaces.
#[derive(IntoElement)]
struct ThemeLabel {
    name: SharedString,
    palette: Vec<Hsla>,
    checked: Option<bool>,
}

impl RenderOnce for ThemeLabel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = Tokens::current(cx);
        h_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .justify_between()
            .child(div().flex_1().min_w_0().truncate().child(self.name))
            .child(swatches(&self.palette, tokens.line))
            .when_some(self.checked, |row, checked| {
                row.child(
                    Icon::from(IconName::Check)
                        .xsmall()
                        .flex_shrink_0()
                        .text_color(tokens.fg)
                        .when(!checked, |icon| icon.invisible()),
                )
            })
    }
}

fn swatches(palette: &[Hsla], border: Hsla) -> Div {
    h_flex()
        .flex_shrink_0()
        .gap_0p5()
        .children(palette.iter().map(|color| {
            div()
                .size_3()
                .rounded_sm()
                .border_1()
                .border_color(border)
                .bg(*color)
        }))
}

/// A font name previewed in its own family; Automatic uses the UI font.
#[derive(Clone)]
pub(super) struct FontItem {
    name: String,
    preview: bool,
}

impl FontItem {
    pub(super) fn new(name: String) -> Self {
        let preview = name != AUTOMATIC_FONT;
        Self { name, preview }
    }

    fn label(&self) -> Div {
        div()
            .truncate()
            .when(self.preview, |label| label.font_family(self.name.clone()))
            .child(self.name.clone())
    }
}

impl SelectItem for FontItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(self.label().into_any_element())
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.label()
    }

    fn value(&self) -> &String {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::{FontItem, ThemeItem, palette, theme_items};
    use crate::theme::{self, AUTOMATIC_FONT, ThemeDocument};
    use gpui_kit::component::select::SelectItem as _;
    use gpui_kit::component::try_parse_color;

    fn document(colors: &str, tones: &str) -> ThemeDocument {
        ThemeDocument::parse(&format!(
            r#"{{"name":"Fixture","themes":[{{"name":"Fixture","mode":"dark",
            "subspace":{{"version":1,"tones":{tones}}},"colors":{colors}}}]}}"#
        ))
        .unwrap()
    }

    fn parsed(text: &str) -> gpui_kit::Hsla {
        try_parse_color(text).unwrap()
    }

    #[test]
    fn the_palette_prefers_tones_then_semantic_then_base_colours() {
        let theme = document(
            r##"{"background":"#101010","foreground":"#f0f0f0","primary.background":"#0000aa",
            "danger.background":"#aa0000","base.red":"#ff0000","base.yellow":"#ffff00",
            "success.background":"#00aa00","base.blue":"#0000ff","base.cyan":"#00ffff"}"##,
            r##"{"accent":"#123456"}"##,
        );
        let colors = palette(|key| theme.color(key));
        let expected = [
            "#101010", "#f0f0f0", "#123456", "#aa0000", "#ffff00", "#00aa00", "#0000ff", "#00ffff",
        ]
        .map(parsed);
        assert_eq!(colors, expected);
    }

    #[test]
    fn missing_or_invalid_palette_colours_are_left_out() {
        let colors = palette(|key| match key {
            "background" => Some("not a colour".into()),
            "foreground" => Some("#ffffff".into()),
            _ => None,
        });
        assert_eq!(colors, [parsed("#ffffff")]);
    }

    #[gpui_kit::test]
    fn every_registered_theme_shows_its_own_palette(cx: &mut gpui_kit::TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            theme::preview_tests::init_app(cx, root.path(), root.path().join("state.json"));
            theme::initialize(cx);
            let items = theme_items(cx);
            assert!(items.len() > 1);
            for item in &items {
                assert!(item.palette.len() >= 6, "{}", item.name);
            }
            assert!(
                items
                    .windows(2)
                    .any(|pair| pair[0].palette != pair[1].palette)
            );
        });
    }

    #[test]
    fn items_keep_their_name_as_title_and_saved_value() {
        let theme = ThemeItem::new("Shell Dark".into(), Vec::new());
        assert_eq!(theme.title().as_ref(), "Shell Dark");
        assert_eq!(theme.value(), "Shell Dark");
        let font = FontItem::new("Menlo".into());
        assert_eq!(font.title().as_ref(), "Menlo");
        assert_eq!(font.value(), "Menlo");
        assert!(font.preview);
        assert!(!FontItem::new(AUTOMATIC_FONT.into()).preview);
    }
}
