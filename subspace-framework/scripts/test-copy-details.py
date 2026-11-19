"""Inventory shared error actions and occurrence logging in production tokens.

This bounded guard checks composition, not Rust control flow. Headless view
tests verify visibility and clipboard delivery for the corresponding states.
"""

import unittest
from pathlib import Path

from architecture_source import closing, literal
from icon_button_source import production_tokens


ROOT = Path(__file__).resolve().parent.parent
SURFACES = {
    'update failure in About': (
        'host/app/src/app/about_dialog/update_panel/view.rs', 'update-copy-details'),
    'update failure in Settings': (
        'host/app/src/app/about_dialog/update_panel/compact.rs', 'settings-update-copy-details'),
    'Copilot turn failure': (
        'host/chat/src/agent_chat/failure_view.rs', 'chat-failure-copy'),
    'Copilot startup failure': (
        'host/chat/src/agent_chat/failure_view/legacy.rs', 'chat-error-copy'),
    'About diagnostics': (
        'host/app/src/app/about_dialog.rs', 'about-copy-diagnostics'),
}

CHAT = 'host/chat/src/agent_chat/'
UPDATE = 'host/app/src/app/about_dialog/update_panel.rs'
# Queued chat facts are emitted by the event callback, never by a renderer.
LOGGING = {
    'update failure in About': (UPDATE, 'update-error', UPDATE, 'accepted_failure'),
    'update failure in Settings': (UPDATE, 'update-error', UPDATE, 'accepted_failure'),
    'Copilot turn failure': (CHAT + 'error_logging.rs', 'chat-failure',
                              CHAT + 'attention.rs', 'accept_failure'),
    'Copilot startup failure': (CHAT + 'error_logging.rs', 'chat-error',
                                 CHAT + 'state.rs', 'set_error'),
}


def has_action(source: str, identifier: str, constructor: str = 'new') -> bool:
    items = production_tokens(source)
    for index, item in enumerate(items):
        if item.text != 'CopyDetails' or [part.text for part in items[index + 1:index + 5]] != [
            ':', ':', constructor, '(',
        ]:
            continue
        end = closing(items, index + 4)
        if any(part.kind in {'string', 'raw'} and literal(part) == identifier
               for part in items[index + 5:end]):
            return True
    return False


def has_call(source: str, name: str) -> bool:
    items = production_tokens(source)
    return any(
        item.text == name
        and index + 1 < len(items)
        and items[index + 1].text == '('
        and (index == 0 or items[index - 1].text != 'fn')
        for index, item in enumerate(items)
    )


def has_logged_surface(source: str, surface: str) -> bool:
    items = production_tokens(source)
    strings = {literal(item) for item in items if item.kind in {'string', 'raw'}}
    if surface not in strings:
        return False
    for index, item in enumerate(items[:-1]):
        if item.text != 'log_error' or items[index + 1].text != '(':
            continue
        if index > 0 and items[index - 1].text == 'fn':
            continue
        end = closing(items, index + 1)
        arguments = items[index + 2:end]
        if any(part.kind in {'string', 'raw'} and literal(part) == surface
               or part.kind == 'word' and part.text == 'surface' for part in arguments):
            return True
    return False


class CopyDetailsInventoryTests(unittest.TestCase):
    def test_every_error_has_an_occurrence_logger_and_acceptance_hook(self) -> None:
        self.assertEqual(set(LOGGING), set(SURFACES) - {'About diagnostics'})
        for name, (logger, surface, boundary, hook) in LOGGING.items():
            with self.subTest(surface=name):
                self.assertTrue(has_logged_surface((ROOT / logger).read_text(), surface), logger)
                self.assertTrue(has_call((ROOT / boundary).read_text(), hook), boundary)
        self.assertTrue(has_call((ROOT / (CHAT + 'effects.rs')).read_text(), 'log_accepted_errors'))

    def test_removing_a_logger_or_acceptance_hook_fails_the_inventory(self) -> None:
        for name, (logger, surface, boundary, hook) in LOGGING.items():
            with self.subTest(surface=name):
                source = (ROOT / logger).read_text()
                self.assertFalse(has_logged_surface(source.replace('log_error', 'removed_logger'), surface))
                self.assertFalse(has_logged_surface(source.replace(f'"{surface}"', '"unrelated"'), surface))
                source = (ROOT / boundary).read_text()
                self.assertFalse(has_call(source.replace(hook, 'removed_acceptance'), hook))

    def test_a_logging_hook_must_be_called_in_production(self) -> None:
        self.assertTrue(has_call('fn accept() { log_error("surface", error); }', 'log_error'))
        for source in [
            'fn log_error() {}',
            '// log_error("surface", error);',
            '#[cfg(test)] fn test() { log_error("surface", error); }',
            'use logging::log_error;',
        ]:
            self.assertFalse(has_call(source, 'log_error'))

    def test_shared_error_composition_opens_the_existing_log_command(self) -> None:
        source = ROOT / 'board-kit/dashboard/src/copy_details.rs'
        items = production_tokens(source.read_text())
        words = {item.text for item in items if item.kind == 'word'}
        strings = {literal(item) for item in items if item.kind in {'string', 'raw'}}
        self.assertIn('OpenLogView', words)
        self.assertIn('dispatch_action', words)
        self.assertIn('Show log', strings)
        self.assertIn('show-log', strings)

    def test_copilot_view_composes_the_shared_legacy_action(self) -> None:
        source = ROOT / 'host/chat/src/agent_chat/view.rs'
        words = {item.text for item in production_tokens(source.read_text()) if item.kind == 'word'}
        self.assertIn('copy_legacy_error', words)

    def test_every_named_error_surface_uses_the_shared_action(self) -> None:
        for name, (path, identifier) in SURFACES.items():
            with self.subTest(surface=name):
                self.assertTrue(
                    has_action((ROOT / path).read_text(), identifier,
                               'diagnostics' if name == 'About diagnostics' else 'new'),
                    f'{path}: restore the shared CopyDetails action for {name}',
                )

    def test_comments_and_test_only_controls_do_not_satisfy_inventory(self) -> None:
        code = '''
            // CopyDetails::new("copy-error", error);
            #[cfg(test)] mod tests {
                fn example() { CopyDetails::new("copy-error", error); }
            }
        '''
        self.assertFalse(has_action(code, 'copy-error'))
        self.assertTrue(has_action('fn view() { CopyDetails::new("copy-error", error); }', 'copy-error'))
        self.assertFalse(has_action('fn view() { CopyDetails::diagnostics("copy-error"); }', 'copy-error'))

    def test_removing_any_named_action_fails_its_contract(self) -> None:
        for _, identifier in SURFACES.values():
            with self.subTest(action=identifier):
                code = f'fn view() {{ CopyDetails::new("{identifier}", error); }}'
                self.assertTrue(has_action(code, identifier))
                self.assertFalse(has_action(code.replace('CopyDetails', 'OtherControl'), identifier))
                self.assertFalse(has_action(code.replace(identifier, 'unrelated-action'), identifier))


if __name__ == '__main__':
    unittest.main()
