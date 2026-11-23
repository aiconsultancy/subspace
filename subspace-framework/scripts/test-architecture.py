"""Framework architecture fixtures use temporary packages, never Cargo or real builds."""

import importlib.util
import os
import tempfile
import unittest
from itertools import product
from pathlib import Path
from unittest.mock import patch

from architecture_source import switch_violations

SPEC = importlib.util.spec_from_file_location(
    "architecture", Path(__file__).with_name("check-architecture.py")
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class SpaceLayoutTests(unittest.TestCase):
    """The Framework's layout: layers, clients, its catalogue Space and the facade."""

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        root_patch = patch.object(CHECK, "ROOT", self.root)
        root_patch.start()
        self.addCleanup(root_patch.stop)

    def test_a_space_lives_in_spaces_and_the_old_boards_folder_is_rejected(self) -> None:
        self.assertEqual(CHECK.group(self.root / "spaces/irc"), ("spaces", "irc"))
        self.assertEqual(CHECK.group(self.root / "spaces/irc/crates/support"), ("spaces", "irc"))
        self.assertEqual(CHECK.group(self.root), ("facade", None))
        self.assertEqual(CHECK.group(self.root / "src"), ("facade", None))
        for unclassified in ("boards/irc", "boards/irc/crates/support", "spaces/irc/extra/nested",
                             "framework/spaces/irc", "framework/host/app", "products/subspace",
                             "terminal/panel"):
            with self.assertRaises(ValueError, msg=unclassified):
                CHECK.group(self.root / unclassified)

    def test_no_framework_layer_or_client_depends_on_a_space(self) -> None:
        space = ("spaces", "irc")
        for layer in ("host", "board-kit", "services", "runtimes", "clients"):
            self.assertFalse(CHECK.allowed((layer, None), space), layer)

    def test_a_space_reaches_only_its_own_crates_and_the_shared_layers(self) -> None:
        own = ("spaces", "irc")
        self.assertTrue(CHECK.allowed(own, ("spaces", "irc")))
        self.assertFalse(CHECK.allowed(own, ("spaces", "github")))
        self.assertFalse(CHECK.allowed(own, ("host", None)))
        for layer in ("board-kit", "services", "runtimes", "clients"):
            self.assertTrue(CHECK.allowed(own, (layer, None)), layer)

    def test_only_the_facade_reaches_every_layer(self) -> None:
        self.assertTrue(CHECK.allowed(("facade", None), ("spaces", "component-gallery")))
        self.assertTrue(CHECK.allowed(("facade", None), ("host", None)))


class SourceArchitectureTests(unittest.TestCase):
    def setUp(self) -> None:
        """Create classified package roots without resolving or building Cargo crates."""
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.packages = {}
        self.root_patch = patch.object(CHECK, "ROOT", self.root)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)
        for name in ("board-kit/dashboard", "board-kit/feature",
                     "clients/github", "host/app", "services/core",
                     "services/agent-tools", "spaces/actions",
                     "spaces/actions/crates/support", "spaces/irc", "spaces/irc/crates/support",
                     "src", "host/terminal", "runtimes/agent",
                     "host/chat", "host/search-ui",
                     "host/board-command"):
            directory = self.root / name
            directory.mkdir(parents=True)
            (directory / "Cargo.toml").write_text("[package]\nname = 'fixture'\n")
            self.packages[directory] = {"name": name, "dependencies": []}

    def write(self, name: str, text: str = "") -> Path:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def include(self, source: str, target: str, syntax: str = "include_str") -> None:
        relative = os.path.relpath(self.root / target, (self.root / source).parent)
        text = (f'#[path = "{relative}"] mod foreign;' if syntax == "path"
                else f'const DATA: &str = {syntax}!("{relative}");')
        self.write(source, text)

    def failures(self) -> list[str]:
        return CHECK.violations(self.packages)

    def test_icon_button_source_policy_is_integrated(self) -> None:
        self.write("spaces/actions/src/view.rs", '''
fn render() {
    Button::new("refresh").icon(IconName::RefreshCw);
}
''')
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("spaces/actions/src/view.rs:3:", failures[0])
        self.assertIn("IconButton", failures[0])

    def test_tool_lookups_use_the_shared_path_service(self) -> None:
        for code in ('for d in std::env::split_paths(&p) {}',
                     'let p = std::env::var_os("PATH");'):
            with self.subTest(code=code):
                self.write("spaces/irc/src/tool.rs", code)
                self.assertTrue(any("find_tool" in failure for failure in self.failures()))
        self.write("spaces/irc/src/tool.rs",
                   'let tool = subspace_core::path::find_tool("gh"); // not split_paths(\n'
                   'let home = std::env::var_os("HOME");')
        self.write("services/core/src/path/lookup.rs", 'std::env::split_paths(&p)')
        self.write("runtimes/agent/src/runtime/child.rs", 'std::env::split_paths(v)')
        self.assertFalse(any("find_tool" in failure for failure in self.failures()))

    def test_switches_are_built_only_by_the_shared_toggle(self) -> None:
        for source in ("spaces/irc/src/view.rs", "host/app/src/settings.rs",
                       "board-kit/dashboard/src/fields.rs"):
            with self.subTest(source=source):
                self.write(source, 'let on = Switch :: new("option").checked(true);')
                self.assertTrue(any("Switch::new" in failure for failure in self.failures()))
                (self.root / source).unlink()
        self.write("board-kit/dashboard/src/toggle.rs", 'Switch::new(id)')
        self.write("board-kit/dashboard/src/toggle/tests.rs", 'Switch::new("row")')
        self.write("spaces/irc/src/view.rs",
                   'toggle::switch("option", cx); // not Switch::new\nlet text = "Switch::new";')
        self.assertFalse(any("Switch::new" in failure for failure in self.failures()))

    def test_dedicated_toolbars_reject_shared_switch_calls(self) -> None:
        sources = (
            "host/app/src/app/toolbar.rs",
            "host/app/src/app/toolbar/chrome.rs",
            "host/app/src/app/toolbar/globals.rs",
            "host/chat/src/agent_chat/toolbar.rs",
            "board-kit/dashboard/src/panel/toolbar.rs",
            "board-kit/dashboard/src/log_view/view/toolbar.rs",
            "board-kit/dashboard/src/native_grid/view/controls/toolbar.rs",
        )
        calls = ('toggle::switch("follow", cx)',
                 'subspace_dashboard :: toggle /* gap */ :: switch\n("follow", cx)',
                 'crate::toggle::switch("follow", cx)')
        for source, call in product(sources, calls):
            with self.subTest(source=source, call=call):
                failures = switch_violations(self.root, self.root / source, "// toolbar\n" + call)
                self.assertEqual(len(failures), 1, failures)
                self.assertIn(f"{source}:2:", failures[0])
                self.assertIn("toolbar switch", failures[0])

    def test_toolbar_switch_guard_runs_in_the_existing_architecture_check(self) -> None:
        self.write("board-kit/dashboard/src/log_view/view/toolbar.rs",
                   'fn render() { crate::toggle::switch("follow", cx); }')
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("toolbar switch", failures[0])

    def test_mixed_views_check_only_the_named_toolbar_helpers(self) -> None:
        for source, name in (
            ("host/search-ui/src/search_view/view.rs", "scope_bar"),
            ("board-kit/dashboard/src/log_view/view.rs", "follow_control"),
        ):
            with self.subTest(source=source):
                text = '''
fn settings() { toggle::switch("setting", cx); }
fn NAME(&self) -> impl IntoElement {
    div().child({ let option = toggle /* comment */ :: switch("toolbar", cx); option })
}
fn other() { toggle::switch("other", cx); }
'''.replace("NAME", name)
                failures = switch_violations(self.root, self.root / source, text)
                self.assertEqual(len(failures), 1, failures)
                self.assertIn(":4:", failures[0])
                self.assertEqual(switch_violations(self.root, self.root / source,
                                                  text.replace(name, "settings_toolbar")), [])

    def test_toolbar_guard_preserves_settings_native_controls_and_quoted_examples(self) -> None:
        settings = (
            "board-kit/dashboard/src/native_grid/view/controls.rs",
            "host/search-ui/src/search_view/view/sources.rs",
            "host/app/src/settings/view.rs",
            "spaces/component-gallery/src/samples/choices.rs",
        )
        for source in settings:
            with self.subTest(source=source):
                self.assertEqual(switch_violations(self.root, self.root / source,
                    'fn options_panel() { toggle::switch("setting", cx).checked(true); }'), [])
        text = '''
// toggle::switch("comment", cx)
/* outer /* toggle::switch("nested comment", cx) */ */
const EXAMPLE: &str = "toggle::switch(quoted, cx)";
const RAW: &str = r###"toggle::switch(raw, cx)"###;
fn render() {
    toggle::toggle_row("Label", option);
    other::switch("unrelated", cx);
    tabs::tab_bar("scope").selected_index(1);
    command.selected(true).checked(true);
    PopupMenuItem::new("Follow").checked(true);
}
'''
        source = self.root / "host/app/src/app/toolbar.rs"
        self.assertEqual(switch_violations(self.root, source, text), [])

    def test_retired_settings_dispatcher_cannot_return_even_unreferenced(self) -> None:
        self.write("host/app/src/settings/view/pages/legacy.rs", "// empty retired dispatcher")
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("legacy settings dispatcher", failures[0])

    def test_settings_cannot_dispatch_on_migrated_board_id_arms(self) -> None:
        source = "host/app/src/settings/view/pages.rs"
        for board in ("actions", "github", "github-copilot-agents"):
            for value in (f'"{board}"', f'r#"{board}"#'):
                with self.subTest(board=board, value=value):
                    self.write(source, f'fn fields(id: &str) {{ match id {{ {value} => render(), _ => empty() }} }}')
                    failures = self.failures()
                    self.assertEqual(len(failures), 1, failures)
                    self.assertIn("board-ID settings dispatch", failures[0])

    def test_authorization_and_general_settings_require_capabilities_not_ids(self) -> None:
        source = "host/app/src/settings/authorization.rs"
        for expression in ('board.id == "github"', '"github" == board.id',
                           'board.id.as_str() != "github-copilot-agents"', 'matches!(board.id, "github")'):
            with self.subTest(expression=expression):
                self.write(source, f'fn start() {{ if {expression} {{ start_query(); }} }}')
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("board-ID settings dispatch", failures[0])

    def test_shared_credential_sections_and_test_fixtures_are_not_board_dispatch(self) -> None:
        self.write("host/app/src/settings/view/connection.rs", '''
            // board.id == "github" is a quoted example, not implementation.
            fn token() { file.token_present("github"); file.token_present("irc"); }
            const EXAMPLE: &str = "board.id == \\\"github\\\"";
        ''')
        self.write("host/app/src/settings/connection_tests/fixture.rs",
                   'fn test_fixture(id: &str) { match id { "github" => fixture(), _ => empty() } }')
        self.assertEqual(self.failures(), [])

    def test_upward_includes_in_normal_test_example_and_build_sources(self) -> None:
        for source, syntax in (("src/lib.rs", "include_str"),
                               ("tests/theme.rs", "include_bytes"),
                               ("examples/theme.rs", "path"),
                               ("build.rs", "include_str")):
            with self.subTest(source=source):
                self.include("board-kit/dashboard/" + source,
                             "host/app/themes/theme.json", syntax)
        failures = self.failures()
        self.assertEqual(len(failures), 4, failures)
        self.assertTrue(all("board-kit -> host forbidden" in f for f in failures))

    def test_agent_tools_rejects_non_service_packages_for_every_dependency_kind(self) -> None:
        """Resolve internal package ownership by path, not an import alias."""
        package = self.packages[self.root / "services/agent-tools"]
        targets = ("board-kit/dashboard", "clients/github", "host/app",
                   "runtimes/agent", "spaces/actions")
        for target, kind in product(targets, (None, "build", "dev")):
            with self.subTest(target=target, kind=kind):
                package["dependencies"] = [{
                    "name": "fixture-package", "rename": "neutral-alias", "kind": kind,
                    "path": str(self.root / target),
                }]
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("services ->", failures[0])
                self.assertIn("forbidden", failures[0])

    def test_agent_tools_source_includes_keep_the_services_only_boundary(self) -> None:
        """Keep Cargo and source references subject to the same package policy."""
        targets = ("board-kit/dashboard", "clients/github", "host/app",
                   "runtimes/agent", "spaces/actions")
        source = "services/agent-tools/tests/regression.rs"
        for target, syntax in product(targets, ("include_str", "include_bytes", "path")):
            with self.subTest(target=target, syntax=syntax):
                self.include(source, target + "/src/lib.rs", syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("services ->", failures[0])
                (self.root / source).unlink()
        self.include(source, "services/core/src/lib.rs", "path")
        self.assertEqual(self.failures(), [])

    def test_agent_tools_rejects_gpui_and_agent_sdk_package_identities(self) -> None:
        """Check all declared edges, even renamed, optional or target-specific ones."""
        package = self.packages[self.root / "services/agent-tools"]
        names = ("gpui", "gpui-kit", "gpui-base", "gpui-component", "gpui-fast",
                 "gpui-pre", "gpui-devtools", "github-copilot-sdk")
        declarations = ({}, {"optional": True}, {"target": 'cfg(windows)'})
        for name, kind, rename, declaration in product(
                names, (None, "build", "dev"), (None, "neutral-alias"), declarations):
            if kind == "dev" and declaration.get("optional"):
                continue  # Cargo does not permit optional dev-dependencies.
            with self.subTest(name=name, kind=kind, rename=rename, declaration=declaration):
                package["dependencies"] = [{
                    "name": name, "kind": kind, "rename": rename, **declaration,
                }]
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn(name, failures[0])
                self.assertIn("no GPUI or agent SDK", failures[0])

    def test_agent_tools_allows_services_and_ordinary_external_dependencies(self) -> None:
        """Allow headless dependencies without banning unrelated SDK package names."""
        package = self.packages[self.root / "services/agent-tools"]
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                package["dependencies"] = [{
                    "name": "subspace-core", "rename": "core-alias", "kind": kind,
                    "path": str(self.root / "services/core"),
                }] + [{"name": name, "kind": kind, "rename": "gpui-kit"}
                      for name in ("serde", "serde_json", "tokio", "async-trait", "base64",
                                   "tempfile", "unrelated-sdk")]
                self.assertEqual(self.failures(), [])

    def test_agent_runtime_cannot_return_to_board_kit(self) -> None:
        """Pin the runtime package's layer even when its dependencies look legal."""
        directory = self.root / "board-kit/agent"
        directory.mkdir()
        self.packages[directory] = {"name": "subspace-agent", "dependencies": []}
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("runtime implementation", failures[0])

    def test_search_ui_package_is_pinned_to_host(self) -> None:
        directory = self.root / "board-kit/feature"
        self.packages[directory]["name"] = "subspace-search-ui"
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("search UI implementation", failures[0])

    def test_board_kit_cannot_define_import_or_reexport_search_view(self) -> None:
        source = "board-kit/feature/tests/regression.rs"
        for code in ("pub mod search_view;", "pub struct SearchView {}",
                     "pub enum SearchViewEvent {}", "impl SearchView {}",
                     "pub use hidden::SearchView;", "pub use hidden::search_view;",
                     "use subspace_search_ui::SearchView as Neutral;",
                     "extern crate subspace_search_ui as neutral;"):
            with self.subTest(code=code):
                self.write(source, "#[cfg(test)] mod test { " + code + " }")
                failures = self.failures()
                self.assertTrue(failures)
                self.assertTrue(all("search UI implementation" in f for f in failures))
        self.write(source, '''
// pub struct SearchView {} mod search_view;
const EXAMPLE: &str = "use subspace_search_ui::SearchView;";
use subspace_search::Service;
''')
        self.assertEqual(self.failures(), [])

    def test_board_kit_search_view_source_cannot_return_under_an_alias(self) -> None:
        self.write("board-kit/dashboard/src/search_view.rs", "struct Neutral {}")
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("search UI implementation", failures[0])

    def test_search_ui_cargo_edges_reject_boards_and_board_kit_in_every_kind(self) -> None:
        for source, kind in product(("spaces/actions", "board-kit/dashboard"),
                                    (None, "build", "dev")):
            with self.subTest(source=source, kind=kind):
                package = self.packages[self.root / source]
                package["dependencies"] = [{"name": "subspace-search-ui", "rename": "neutral",
                                            "kind": kind, "target": "cfg(windows)",
                                            "path": str(self.root / "host/search-ui")}]
                self.assertTrue(any("-> host forbidden" in f for f in self.failures()))
                package["dependencies"] = []
        self.packages[self.root / "host/app"]["dependencies"] = [{
            "name": "subspace-search-ui", "kind": None,
            "path": str(self.root / "host/search-ui"),
        }]
        self.assertEqual(self.failures(), [])

    def test_search_ui_source_includes_reject_boards_and_board_kit(self) -> None:
        for source, syntax in product(("spaces/actions", "board-kit/dashboard"),
                                      ("include_str", "include_bytes", "path")):
            file = source + "/tests/regression.rs"
            with self.subTest(source=source, syntax=syntax):
                self.include(file, "host/search-ui/src/lib.rs", syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("-> host forbidden", failures[0])
                (self.root / file).unlink()

    def test_every_board_kit_client_edge_is_forbidden(self) -> None:
        package = self.packages[self.root / "board-kit/dashboard"]
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                package["dependencies"] = [{"name": "subspace-client-github", "rename": "neutral",
                                            "kind": kind, "target": "cfg(windows)",
                                            "path": str(self.root / "clients/github")}]
                self.assertTrue(any("board-kit -> clients forbidden" in failure for failure in self.failures()))

    def test_board_kit_client_source_includes_are_forbidden(self) -> None:
        source = "board-kit/dashboard/tests/regression.rs"
        for syntax in ("include_str", "include_bytes", "path"):
            with self.subTest(syntax=syntax):
                self.include(source, "clients/github/src/lib.rs", syntax)
                self.assertTrue(self.failures())
        self.write(source, 'const EXAMPLE: &str = "subspace_client_github::Client";')
        self.assertEqual(self.failures(), [])

    def test_board_owned_crate_has_no_sibling_board_privileges(self) -> None:
        package = self.packages[self.root / "spaces/irc/crates/support"]
        for kind in (None, "build", "dev"):
            package["dependencies"] = [{"name": "fixture", "rename": "neutral", "kind": kind,
                                        "path": str(self.root / "spaces/actions/crates/support")}]
            self.assertTrue(any("spaces -> spaces forbidden" in failure for failure in self.failures()))
        package["dependencies"] = [{"name": "fixture", "kind": None,
                                    "path": str(self.root / "clients/github")}]
        self.assertEqual(self.failures(), [])

    def test_shared_offline_client_fixtures_are_dev_dependencies_only(self) -> None:
        package = self.packages[self.root / "spaces/irc/crates/support"]
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                package["dependencies"] = [{"name": "subspace-client-test-support", "rename": "neutral",
                                            "kind": kind, "target": "cfg(windows)"}]
                if kind == "dev":
                    self.assertEqual(self.failures(), [])
                else:
                    self.assertTrue(any("offline client fixtures" in failure for failure in self.failures()))

    def test_board_kit_cannot_import_or_reexport_runtime_implementation(self) -> None:
        """Reject implementation paths in cfg-gated code as well as ordinary imports."""
        source = "board-kit/dashboard/tests/regression.rs"
        for code in ("pub use subspace_agent::AgentHandle;",
                     "use subspace_agent::{Command, Event};",
                     "extern crate subspace_agent as neutral;",
                     "pub use subspace_chat::ChatPanel;"):
            with self.subTest(code=code):
                self.write(source, "#[cfg(test)] mod test { " + code + " }")
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("implementation import", failures[0])
        self.write(source, 'const EXAMPLE: &str = "subspace_agent::AgentHandle"; // subspace_chat::ChatPanel')
        self.assertEqual(self.failures(), [])

    def test_agent_model_has_the_same_headless_guard_as_agent_tools(self) -> None:
        """Keep aliased build and test dependencies headless too."""
        directory = self.root / "services/agent-model"
        directory.mkdir()
        self.packages[directory] = {"name": "subspace-agent-model", "dependencies": []}
        for name, kind in product(("gpui-kit", "github-copilot-sdk"), (None, "dev", "build")):
            with self.subTest(name=name, kind=kind):
                self.packages[directory]["dependencies"] = [{"name": name, "rename": "neutral", "kind": kind}]
                self.assertEqual(len(self.failures()), 1)

    def test_board_kit_runtime_and_chat_edges_reject_aliases_and_every_dependency_kind(self) -> None:
        """Apply normal layer directions to every Cargo edge spelling."""
        package = self.packages[self.root / "board-kit/dashboard"]
        for target, kind in product(("runtimes/agent", "host/chat"), (None, "dev", "build")):
            with self.subTest(target=target, kind=kind):
                package["dependencies"] = [{"name": "implementation", "rename": "neutral", "kind": kind,
                                            "path": str(self.root / target)}]
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("forbidden", failures[0])

    def test_board_kit_runtime_source_includes_reject_every_include_form(self) -> None:
        """Source includes must not bypass the removed runtime dependency."""
        source = "board-kit/dashboard/tests/regression.rs"
        for target, syntax in product(("runtimes/agent", "host/chat"), ("include_str", "include_bytes", "path")):
            with self.subTest(target=target, syntax=syntax):
                self.include(source, target + "/src/lib.rs", syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("forbidden", failures[0])

    def test_reintroducing_original_e7_token_theme_reference_fails(self) -> None:
        self.write("board-kit/dashboard/src/tokens.rs", '''
#[cfg(test)]
mod tests {
    const SHELL_THEME: &str = include_str!("../../../host/app/themes/shell.json");
}
''')
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("host/app/themes/shell.json", failures[0])
        self.assertIn("board-kit -> host forbidden", failures[0])

    def test_own_board_support_is_allowed_but_sibling_board_is_not(self) -> None:
        self.include("spaces/actions/src/lib.rs", "spaces/actions/crates/support/data.txt")
        self.include("spaces/actions/crates/support/src/lib.rs", "spaces/actions/data.txt")
        self.assertEqual(self.failures(), [])
        self.include("spaces/irc/tests/assets.rs", "spaces/actions/crates/support/data.txt")
        self.assertEqual(len(self.failures()), 1)

    def test_facade_and_host_direction_rules_apply_to_source(self) -> None:
        self.include("host/app/src/lib.rs", "services/core/data.txt")
        self.include("src/lib.rs", "host/app/data.txt")
        self.include("host/terminal/src/lib.rs", "services/core/data.txt")
        self.assertEqual(self.failures(), [])
        self.include("host/terminal/tests/test.rs", "spaces/actions/data.txt")
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("host -> spaces forbidden", failures[0])

    def test_comments_strings_and_character_literals_are_not_code(self) -> None:
        self.write("board-kit/dashboard/src/lib.rs", '''
// include_str!("../../../../host/app/theme.json")
/* outer /* include_bytes!(unknown!()) */ #[path = "up.rs"] */
const EXAMPLE: &str = r###"include_str!(unknown!())"###;
const QUOTED: &str = "include_bytes!(\\\"bad\\\")";
const QUOTE: char = '\"';
''')
        self.assertEqual(self.failures(), [])

    def test_multiline_raw_and_manifest_concat_are_resolved(self) -> None:
        self.write("board-kit/dashboard/tests/test.rs", '''
include_str! (
    concat!(env!("CARGO_MANIFEST_DIR"), r#"/../../host/app/themes/theme.json"#),
);
include_bytes! { r##"../../../host/app/themes/theme.json"## }
''')
        self.assertEqual(len(self.failures()), 2)

    def test_raw_path_attribute_and_escaped_literal_paths(self) -> None:
        self.write("board-kit/dashboard/tests/theme.rs", r'''
# [ path = r##"../../../host/app/src/theme.rs"## ] mod foreign;
include_str!("../../../host/app/themes/\u{73}hell.json");
''')
        failures = self.failures()
        self.assertEqual(len(failures), 2, failures)
        self.assertTrue(all("board-kit -> host forbidden" in f for f in failures))

    def test_inline_module_path_uses_rust_module_directory(self) -> None:
        self.write("board-kit/dashboard/src/outer.rs", '''
mod nested {
    #[path = "../../../../../host/app/source.rs"] mod foreign;
    #[path = "local.rs"] mod local;
}
''')
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("board-kit -> host forbidden", failures[0])

    def test_unknown_expressions_and_unowned_source_fail_closed(self) -> None:
        self.write("board-kit/dashboard/src/lib.rs", '''
include_str!(concat!(env!("OTHER_ROOT"), "/theme.json"));
include_bytes!(unknown!());
#[path = unknown!()] mod foreign;
include_str!("../../../../../shared.rs");
''')
        self.assertEqual(len(self.failures()), 4)

    def test_shared_docs_are_data_not_a_source_layer_escape(self) -> None:
        self.include("src/lib.rs", "docs/ui-components.md")
        self.assertEqual(self.failures(), [])
        self.include("services/core/src/lib.rs", "docs/shared.rs", "path")
        self.assertEqual(len(self.failures()), 1)
        self.include("services/core/src/lib.rs", "docs/shared.md", "path")
        self.assertEqual(len(self.failures()), 1, "a module path is source, not shared data")

    def test_package_roots_skip_foreign_and_generated_trees_not_tests(self) -> None:
        for directory in ("target", "vendor", "foreign"):
            self.write(f"spaces/actions/{directory}/bad.rs", "include_str!(unknown!());")
        self.write("spaces/actions/foreign/Cargo.toml", "[package]\nname='foreign'\n")
        self.assertEqual(self.failures(), [])
        self.write("spaces/actions/examples/bad.rs", "include_str!(unknown!());")
        self.assertEqual(len(self.failures()), 1)

    def test_client_source_cannot_include_runtime_files(self) -> None:
        self.include("clients/github/src/lib.rs", "runtimes/agent/data.txt")
        self.assertEqual(len(self.failures()), 1)

    def test_client_runtime_edge_is_forbidden_for_every_dependency_kind(self) -> None:
        package = self.packages[self.root / "clients/github"]
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                package["dependencies"] = [{
                    "name": "subspace-agent", "kind": kind,
                    "path": str(self.root / "runtimes/agent"),
                }]
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("clients -> runtimes forbidden", failures[0])

    def test_board_owned_crate_is_not_a_shared_client(self) -> None:
        directory = self.root / "spaces/irc/crates/support"
        dependency = {"name": "subspace-irc-support", "kind": None, "path": str(directory)}
        self.packages[self.root / "spaces/irc"]["dependencies"] = [dependency]
        self.packages[directory]["dependencies"] = [{
            "name": "subspace-agent", "kind": None,
            "path": str(self.root / "runtimes/agent"),
        }]
        self.assertEqual(CHECK.group(directory), ("spaces", "irc"))
        self.assertEqual(self.failures(), [])
        for source in ("spaces/actions", "host/app", "clients/github"):
            for kind in (None, "build", "dev"):
                with self.subTest(source=source, kind=kind):
                    package = self.packages[self.root / source]
                    package["dependencies"] = [{**dependency, "kind": kind}]
                    failures = self.failures()
                    self.assertEqual(len(failures), 1, failures)
                    self.assertIn("-> spaces forbidden", failures[0])
                    package["dependencies"] = []

    def test_original_e8_host_help_includes_are_forbidden_without_exemptions(self) -> None:
        board = self.root / "spaces/space-lab"
        board.mkdir()
        (board / "Cargo.toml").write_text("[package]\nname = 'space-lab'\n")
        self.packages[board] = {"name": "space-lab", "dependencies": []}
        for source, target, syntax in (
            ("contents_tests.rs", "spaces/actions/help.md", "include_str"),
            ("contents_runtime_tests.rs", "spaces/space-lab/src/help_images.rs", "path"),
            ("image_tests.rs", "spaces/space-lab/help.md", "include_str"),
            ("image_tests.rs", "spaces/space-lab/help-assets/dashboard.webp", "include_bytes"),
        ):
            with self.subTest(source=source, syntax=syntax):
                path = "host/app/src/app/help_dock/" + source
                self.include(path, target, syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("host -> spaces forbidden", failures[0])
                (self.root / path).unlink()

    def test_the_facade_composes_spaces_but_cannot_include_their_files(self) -> None:
        package = self.packages[self.root / "src"]
        for kind in (None, "build", "dev"):
            package["dependencies"] = [{"name": "board-alias", "kind": kind,
                                       "path": str(self.root / "spaces/actions")}]
            self.assertEqual(self.failures(), [])
        for syntax in ("include_str", "include_bytes", "include", "path"):
            with self.subTest(syntax=syntax):
                self.include("src/lib.rs", "spaces/actions/assets/workflow.svg", syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("facade -> spaces forbidden (source include)", failures[0])
                (self.root / "src/lib.rs").unlink()

    def test_host_artwork_cannot_copy_or_rename_a_boards_owned_art(self) -> None:
        self.write("spaces/actions/assets/unique.svg", "board artwork")
        for name, contents in (("unique.svg", "modified board artwork"),
                               ("renamed.svg", "board artwork")):
            with self.subTest(name=name):
                path = self.write("host/app/assets/" + name, contents)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("board-owned artwork", failures[0])
                path.unlink()
        self.write("host/app/assets/framework.svg", "host artwork")
        self.assertEqual(self.failures(), [])

    def test_known_legacy_board_logo_cannot_return_to_host_defaults(self) -> None:
        self.write("host/app/assets/logos/github.svg", "legacy artwork")
        self.assertTrue(any("board-owned artwork" in f for f in self.failures()))

    def test_include_macro_is_source_not_a_shared_docs_escape(self) -> None:
        self.include("src/lib.rs", "docs/shared.md", "include")
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("unowned/outside source include", failures[0])

    def test_generated_icons_remain_supported(self) -> None:
        self.write("board-kit/feature/src/lib.rs",
                   'include!(concat!(env!("OUT_DIR"), "/icons.rs"));')
        self.assertEqual(self.failures(), [])
        self.write("board-kit/feature/src/lib.rs",
                   'include!(concat!(env!("OUT_DIR"), "/../../spaces/actions/src/lib.rs"));')
        self.assertTrue(self.failures())

    def test_generated_output_filenames_keep_ascii_identifiers(self) -> None:
        for name, supported in (("_Icons_09az.rs", True), ("icons.rs", True),
                                ("icôns.rs", False), ("icons١.rs", False)):
            with self.subTest(name=name):
                self.write("board-kit/feature/src/lib.rs",
                           f'include!(concat!(env!("OUT_DIR"), "/{name}"));')
                self.assertEqual(self.failures() == [], supported)

    def test_most_specific_owner_is_used_for_resources_and_source(self) -> None:
        # A broad product package must not hide the classified board package.
        # The fixture deliberately nests roots to distinguish longest-match
        # ownership from insertion order; real board-owned roots nest likewise.
        self.packages[self.root] = {"name": "broad-fixture", "dependencies": []}
        original_group = CHECK.group
        with patch.object(CHECK, "group", side_effect=lambda path:
                          ("facade", None) if path == self.root else original_group(path)):
            self.include("host/app/src/lib.rs", "spaces/actions/crates/support/asset.svg")
            failures = self.failures()
            self.assertEqual(len(failures), 1, failures)
            self.assertIn("host -> spaces forbidden", failures[0])

    def test_inline_path_override_and_cfg_attr_path_fail_explicitly(self) -> None:
        self.write("services/core/src/lib.rs", '''
#[path = "custom"] pub(crate) mod inline { #[path = "local.rs"] mod local; }
#[cfg_attr(test, path = "elsewhere.rs")] mod conditional;
''')
        failures = self.failures()
        self.assertEqual(len(failures), 2, failures)
        self.assertTrue(all("unsupported" in failure for failure in failures))

    def test_retired_harness_packages_and_cargo_edges_fail_closed(self) -> None:
        retired = self.root / "prototypes/space-lab/managed-runtimes"
        with self.assertRaisesRegex(ValueError, "Unclassified internal package"):
            CHECK.group(retired)
        package = self.packages[self.root / "src"]
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                package["dependencies"] = [{
                    "name": "retired-harness", "kind": kind, "path": str(retired),
                }]
                with self.assertRaisesRegex(ValueError, "Unclassified internal package"):
                    self.failures()

    def test_product_source_cannot_read_retired_harness_files(self) -> None:
        for syntax in ("include_str", "include_bytes", "path"):
            with self.subTest(syntax=syntax):
                self.include("src/lib.rs",
                             "prototypes/space-lab/managed-runtimes/src/main.rs", syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("unowned/outside source include", failures[0])

    def test_only_the_facade_includes_the_readme_as_data(self) -> None:
        self.write("README.md", "# Framework")
        self.write("src/lib.rs", 'const README: &str = include_str!("../README.md");')
        self.assertEqual(self.failures(), [])
        self.include("services/core/src/lib.rs", "README.md")
        self.assertEqual(len(self.failures()), 1)

    def test_path_dependencies_cannot_leave_the_framework(self) -> None:
        package = self.packages[self.root / "host/app"]
        for kind in (None, "build", "dev"):
            with self.subTest(kind=kind):
                package["dependencies"] = [{"name": "subspace-irc-board", "kind": kind,
                                            "path": str(self.root.parent / "app/spaces/irc")}]
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("outside the Framework", failures[0])

    def test_the_facade_may_offer_client_fixtures_only_as_an_optional_feature(self) -> None:
        package = self.packages[self.root / "src"]
        package["dependencies"] = [{"name": "subspace-client-test-support", "kind": None,
                                    "optional": True, "path": str(self.root / "clients/github")}]
        self.assertEqual(self.failures(), [])
        package["dependencies"][0]["optional"] = False
        self.assertTrue(any("offline client fixtures" in f for f in self.failures()))

    def test_source_includes_cannot_leave_the_framework(self) -> None:
        self.write("host/app/src/lib.rs", 'const X: &str = include_str!("../../../../app/docs/mcp.md");')
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("unowned/outside source include", failures[0])


if __name__ == "__main__":
    unittest.main()
