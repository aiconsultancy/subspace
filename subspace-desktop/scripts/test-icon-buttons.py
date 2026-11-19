"""Fixtures for the bounded production icon-button source policy."""

import unittest
from pathlib import Path

import icon_button_source


ROOT = Path('/repo')
FILE = ROOT / 'spaces/actions/src/view.rs'


class IconButtonSourceTests(unittest.TestCase):
    def failures(self, code: str, file: Path = FILE) -> list[str]:
        return icon_button_source.violations(ROOT, file, code)

    def test_direct_buttons_and_aliases(self) -> None:
        for code in (
            'Button::new("refresh").icon(IconName::Refresh);',
            'gpui_kit::component::Button::new("x").child(Glyph::new("x"));',
            'Button::new("x").child(Icon::new(IconName::X));',
            'Button::new("x").child(IconName::ChevronDown).child("Contents");',
            'Button::new("x").child(svg().path("x.svg"));',
            'styled(Button::new("x"), intent).icon(icon);',
            'Button::new("x").label("× Close");',
            'Button::new("x").label(r#"☑ Viewed"#);',
            'Button::new("x").label("\\u{00d7} Close");',
            'use gpui_kit::component::Button as NativeButton; NativeButton::new("x").icon(icon);',
            'let b = Button::new("x"); let b = b.icon(icon);',
            'InputGroupButton::new("x").icon(icon);',
        ):
            with self.subTest(code=code):
                self.assertTrue(self.failures('fn render() { ' + code + ' }'))

    def test_clickable_icon_only_not_whole_row(self) -> None:
        self.assertTrue(self.failures('fn render() { div().child(Glyph::new("x")).on_click(handler); }'))
        self.assertFalse(self.failures('fn render() { div().child(Glyph::new("x")).child("Details").on_click(handler); }'))

    def test_native_widgets_inert_icons_and_nested_menu(self) -> None:
        code = '''fn render() {
            PopupMenuItem::new("x").icon(IconName::X);
            Tab::new("x").icon(IconName::X);
            Tree::new("x").icon(IconName::X);
            div().child(Glyph::new("x"));
            Button::new("ok").label("OK").on_click(|_, _, _| menu(PopupMenuItem::new("x").icon(IconName::X)));
            Button::new("open").label("Open…");
            // Button::new("x").icon(icon)
            let example = "Button::new(\\"x\\").icon(icon)";
        }'''
        self.assertEqual(self.failures(code), [])

    def test_test_code_and_exact_component_are_excluded(self) -> None:
        code = 'fn render() { Button::new("x").icon(icon); }'
        self.assertEqual(self.failures(code, ROOT / 'spaces/actions/tests/ui.rs'), [])
        self.assertEqual(self.failures(code, ROOT / 'spaces/actions/src/view/tests/fixture.rs'), [])
        self.assertEqual(self.failures('#[cfg(test)] mod tests { ' + code + ' }'), [])
        self.assertEqual(self.failures('#[cfg(test)] pub(crate) mod tests { ' + code + ' }'), [])
        self.assertEqual(self.failures('#[test] ' + code), [])
        self.assertTrue(self.failures('struct S { #[cfg(test)] field: bool } ' + code))
        self.assertTrue(self.failures(code, ROOT / 'products/subspace/src/main.rs'))


if __name__ == '__main__':
    unittest.main()
