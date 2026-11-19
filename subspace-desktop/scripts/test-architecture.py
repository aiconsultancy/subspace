"""Desktop architecture fixtures use temporary packages, never Cargo or real builds."""

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

KINDS = (None, "build", "dev")


class Fixture(unittest.TestCase):
    """A temporary desktop folder beside a temporary Framework folder."""

    PACKAGES = ("spaces/actions", "spaces/actions/crates/support", "spaces/irc",
                "spaces/irc/crates/support", "products/subspace", "clients/nett",
                "clients/irc", "terminal/panel", "terminal/runtime")

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / "subspace-desktop"
        self.framework = self.root.parent / "subspace-framework"
        for name, value in (("ROOT", self.root), ("FRAMEWORK_ROOT", self.framework),
                            ("PATCHES", {"markdown": (self.framework / "vendor/markdown",
                                                      self.root / "vendor/markdown")}),
                            ("MANIFESTS", ())):
            patcher = patch.object(CHECK, name, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.packages = {}
        for name in self.PACKAGES:
            directory = self.root / name
            directory.mkdir(parents=True)
            (directory / "Cargo.toml").write_text("[package]\nname = 'fixture'\n")
            self.packages[directory] = {"name": "subspace-" + name.replace("/", "-"),
                                        "dependencies": []}

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

    def depend(self, source: str, **dependency) -> None:
        self.packages[self.root / source]["dependencies"] = [
            {"kind": None, "features": [], **dependency}]

    def framework_edge(self, source: str, kind: str | None = None, **extra) -> None:
        self.depend(source, name="subspace-framework", kind=kind,
                    path=str(self.framework), **extra)


class LayoutTests(Fixture):
    """Products compose Spaces; Spaces, clients and terminal crates stay apart."""

    def test_packages_are_classified_by_folder(self) -> None:
        self.assertEqual(CHECK.group(self.root / "spaces/irc"), ("spaces", "irc"))
        self.assertEqual(CHECK.group(self.root / "spaces/irc/crates/support"), ("spaces", "irc"))
        self.assertEqual(CHECK.group(self.root / "terminal/panel"), ("terminal", None))
        self.assertEqual(CHECK.group(self.root / "products/descriptor"), ("products", None))
        for unclassified in ("boards/irc", "boards/irc/crates/support", "spaces/irc/extra/nested",
                             "framework/host/app", "framework/spaces/irc",
                             "prototypes/board-lab/managed-runtimes"):
            with self.assertRaises(ValueError, msg=unclassified):
                CHECK.group(self.root / unclassified)

    def test_no_client_or_terminal_crate_depends_on_a_space(self) -> None:
        for layer in ("clients", "terminal"):
            self.assertFalse(CHECK.allowed((layer, None), ("spaces", "irc")), layer)

    def test_a_space_reaches_only_its_own_crates_and_clients(self) -> None:
        own = ("spaces", "irc")
        self.assertTrue(CHECK.allowed(own, ("spaces", "irc")))
        self.assertTrue(CHECK.allowed(own, ("clients", None)))
        for target in (("spaces", "github"), ("products", None), ("terminal", None)):
            self.assertFalse(CHECK.allowed(own, target), target)

    def test_only_products_compose_spaces_and_the_terminal(self) -> None:
        self.assertTrue(CHECK.allowed(("products", None), ("spaces", "irc")))
        self.assertTrue(CHECK.allowed(("products", None), ("terminal", None)))


class FrameworkEdgeTests(Fixture):
    """The one Framework crate, its path and its features."""

    def test_every_crate_may_use_the_framework_crate(self) -> None:
        for source, kind in product(self.PACKAGES, KINDS):
            with self.subTest(source=source, kind=kind):
                self.framework_edge(source, kind)
                self.assertEqual(self.failures(), [])
                self.packages[self.root / source]["dependencies"] = []
        self.depend("spaces/irc", name="subspace-framework")
        self.assertEqual(self.failures(), [], "the published crate needs no path")

    def test_framework_crates_other_than_the_facade_are_forbidden(self) -> None:
        for name, kind in product(("subspace-dashboard", "subspace-shell", "subspace-core",
                                   "subspace-client-test-support"), KINDS):
            with self.subTest(name=name, kind=kind):
                self.depend("spaces/irc", name=name, kind=kind, rename="neutral",
                            path=str(self.framework / "board-kit/dashboard"))
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("only through subspace-framework", failures[0])

    def test_desktop_crates_may_depend_on_each_other_by_name(self) -> None:
        self.depend("spaces/irc", name="subspace-clients-irc",
                    path=str(self.root / "clients/irc"))
        self.assertEqual(self.failures(), [])

    def test_host_features_are_for_products_only(self) -> None:
        for source, feature, kind in product(("spaces/irc", "spaces/irc/crates/support",
                                              "clients/nett", "terminal/panel"),
                                             ("host", "capture"), KINDS):
            with self.subTest(source=source, feature=feature, kind=kind):
                self.framework_edge(source, kind, features=[feature])
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn(f"only products may enable {feature}", failures[0])
                self.packages[self.root / source]["dependencies"] = []
        self.framework_edge("products/subspace", features=["host", "capture"])
        self.assertEqual(self.failures(), [])

    def test_test_support_is_for_dev_dependencies_only(self) -> None:
        for source, kind in product(("spaces/irc", "products/subspace"), KINDS):
            with self.subTest(source=source, kind=kind):
                self.framework_edge(source, kind, features=["test-support"])
                failures = self.failures()
                if kind == "dev":
                    self.assertEqual(failures, [])
                else:
                    self.assertTrue(any("test-support" in f for f in failures), failures)
                self.packages[self.root / source]["dependencies"] = []

    def test_the_framework_lives_at_its_documented_path(self) -> None:
        self.depend("spaces/irc", name="subspace-framework",
                    path=str(self.root.parent / "elsewhere/subspace-framework"))
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("../subspace-framework or crates.io", failures[0])

    def test_path_dependencies_cannot_leave_the_folder(self) -> None:
        for kind in KINDS:
            with self.subTest(kind=kind):
                self.depend("products/subspace", name="outside", kind=kind,
                            path=str(self.root.parent / "outside"))
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("outside Subspace Desktop", failures[0])

    def test_only_the_framework_markdown_patch_is_permitted(self) -> None:
        def manifest(text: str) -> list[str]:
            self.write("Cargo.toml", text)
            with patch.object(CHECK, "MANIFESTS", ("Cargo.toml",)):
                return self.failures()
        self.assertEqual(manifest(
            '[patch.crates-io]\nmarkdown = { path = "../subspace-framework/vendor/markdown" }\n'), [])
        self.assertEqual(manifest("[workspace]\n"), [])
        self.assertEqual(manifest('[patch.crates-io]\nmarkdown = { path = "vendor/markdown" }\n'), [],
                         "after a repository split the desktop keeps its own copy")
        for text in ('[patch.crates-io]\nmarkdown = { path = "other/markdown" }\n',
                     '[patch.crates-io]\nserde = { path = "../subspace-framework/vendor/serde" }\n'):
            with self.subTest(text=text):
                failures = manifest(text)
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("[patch.crates-io]", failures[0])


class SourceTests(Fixture):
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

    def test_host_modules_are_named_only_by_products(self) -> None:
        for source, code in product(
                ("spaces/irc/src/lib.rs", "spaces/irc/crates/support/tests/t.rs",
                 "clients/nett/src/lib.rs", "terminal/panel/src/panel.rs"),
                ("use subspace_framework::shell::BoardRegistration;",
                 "let x = subspace_framework :: chat :: ChatPanel::new();",
                 "use subspace_framework::{dashboard::Row, search_ui};",
                 "use subspace_framework::{lucide, mcp::Board as B};",
                 "subspace_framework::update::check();")):
            with self.subTest(source=source, code=code):
                self.write(source, code)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("is a host module", failures[0])
                (self.root / source).unlink()
        self.write("products/subspace/src/main.rs", "use subspace_framework::shell::launch;")
        self.write("spaces/irc/src/lib.rs", '''
// use subspace_framework::shell::BoardRegistration;
const EXAMPLE: &str = "subspace_framework::chat::ChatPanel";
use subspace_framework::dashboard::{update, shell};
use subspace_framework::{dashboard::{chat_input, prefs}, lucide};
''')
        self.assertEqual(self.failures(), [])

    def test_tool_lookups_use_the_shared_path_service(self) -> None:
        for code in ('for d in std::env::split_paths(&p) {}',
                     'let p = std::env::var_os("PATH");'):
            with self.subTest(code=code):
                self.write("spaces/irc/src/tool.rs", code)
                self.assertTrue(any("find_tool" in failure for failure in self.failures()))
        self.write("spaces/irc/src/tool.rs",
                   'let tool = subspace_framework::core::path::find_tool("gh"); // not split_paths(\n'
                   'let home = std::env::var_os("HOME");')
        self.write("spaces/board-lab/src/environments/extensions.rs", 'std::env::split_paths(&p)')
        self.write("terminal/runtime/src/program.rs", 'std::env::split_paths(v)')
        self.assertFalse(any("find_tool" in failure for failure in self.failures()))

    def test_switches_are_built_only_by_the_shared_toggle(self) -> None:
        for source in ("spaces/irc/src/view.rs", "products/subspace/src/main.rs",
                       "terminal/panel/src/settings.rs"):
            with self.subTest(source=source):
                self.write(source, 'let on = Switch :: new("option").checked(true);')
                self.assertTrue(any("Switch::new" in failure for failure in self.failures()))
                (self.root / source).unlink()
        self.write("spaces/irc/src/view.rs",
                   'toggle::switch("option", cx); // not Switch::new\nlet text = "Switch::new";')
        self.assertFalse(any("Switch::new" in failure for failure in self.failures()))

    def test_dedicated_toolbars_reject_shared_switch_calls(self) -> None:
        calls = ('toggle::switch("follow", cx)',
                 'subspace_framework::dashboard :: toggle /* gap */ :: switch\n("follow", cx)',
                 'dashboard::toggle::switch("follow", cx)')
        for source, call in product(("spaces/actions/src/toolbar.rs", "spaces/web/src/toolbar.rs"),
                                    calls):
            with self.subTest(source=source, call=call):
                failures = switch_violations(self.root, self.root / source, "// toolbar\n" + call)
                self.assertEqual(len(failures), 1, failures)
                self.assertIn(f"{source}:2:", failures[0])
                self.assertIn("toolbar switch", failures[0])

    def test_toolbar_switch_guard_runs_in_the_architecture_check(self) -> None:
        self.write("spaces/actions/src/toolbar.rs", 'fn render() { toggle::switch("follow", cx); }')
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("toolbar switch", failures[0])

    def test_toolbar_guard_preserves_settings_native_controls_and_quoted_examples(self) -> None:
        for source in ("spaces/actions/src/settings.rs", "spaces/web/src/manager/view.rs"):
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
        source = self.root / "spaces/actions/src/toolbar.rs"
        self.assertEqual(switch_violations(self.root, source, text), [])

    def test_upward_includes_in_normal_test_example_and_build_sources(self) -> None:
        for source, syntax in (("src/lib.rs", "include_str"),
                               ("tests/theme.rs", "include_bytes"),
                               ("examples/theme.rs", "path"),
                               ("build.rs", "include_str")):
            with self.subTest(source=source):
                self.include("spaces/irc/" + source, "spaces/actions/themes/theme.json", syntax)
        failures = self.failures()
        self.assertEqual(len(failures), 4, failures)
        self.assertTrue(all("spaces -> spaces forbidden" in f for f in failures))

    def test_source_includes_cannot_leave_the_folder(self) -> None:
        for source in ("spaces/actions/examples/themes.rs", "products/subspace/src/main.rs",
                       "terminal/panel/src/theme.rs"):
            with self.subTest(source=source):
                self.include(source, "../subspace-framework/board-kit/dashboard/examples/x.json")
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("unowned/outside source include", failures[0])
                (self.root / source).unlink()

    def test_board_owned_crate_has_no_sibling_board_privileges(self) -> None:
        for kind in KINDS:
            self.depend("spaces/irc/crates/support", name="subspace-spaces-actions-crates-support",
                        kind=kind, rename="neutral",
                        path=str(self.root / "spaces/actions/crates/support"))
            self.assertTrue(any("spaces -> spaces forbidden" in f for f in self.failures()))
        self.depend("spaces/irc/crates/support", name="subspace-clients-nett",
                    path=str(self.root / "clients/nett"))
        self.assertEqual(self.failures(), [])

    def test_own_board_support_is_allowed_but_sibling_board_is_not(self) -> None:
        self.include("spaces/actions/src/lib.rs", "spaces/actions/crates/support/data.txt")
        self.include("spaces/actions/crates/support/src/lib.rs", "spaces/actions/data.txt")
        self.assertEqual(self.failures(), [])
        self.include("spaces/irc/tests/assets.rs", "spaces/actions/crates/support/data.txt")
        self.assertEqual(len(self.failures()), 1)

    def test_product_and_terminal_direction_rules_apply_to_source(self) -> None:
        self.include("products/subspace/src/main.rs", "products/subspace/assets/product.svg")
        self.include("products/subspace/tests/panel.rs", "terminal/panel/data.txt")
        self.include("terminal/panel/src/lib.rs", "terminal/runtime/data.txt")
        self.assertEqual(self.failures(), [])
        self.include("terminal/panel/tests/test.rs", "spaces/actions/data.txt")
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("terminal -> spaces forbidden", failures[0])

    def test_comments_strings_and_character_literals_are_not_code(self) -> None:
        self.write("spaces/irc/src/lib.rs", '''
// include_str!("../../actions/theme.json")
/* outer /* include_bytes!(unknown!()) */ #[path = "up.rs"] */
const EXAMPLE: &str = r###"include_str!(unknown!())"###;
const QUOTED: &str = "include_bytes!(\\\"bad\\\")";
const QUOTE: char = '\"';
''')
        self.assertEqual(self.failures(), [])

    def test_multiline_raw_and_manifest_concat_are_resolved(self) -> None:
        self.write("spaces/irc/tests/test.rs", '''
include_str! (
    concat!(env!("CARGO_MANIFEST_DIR"), r#"/../actions/themes/theme.json"#),
);
include_bytes! { r##"../../actions/themes/theme.json"## }
''')
        self.assertEqual(len(self.failures()), 2)

    def test_raw_path_attribute_and_escaped_literal_paths(self) -> None:
        self.write("spaces/irc/tests/theme.rs", r'''
# [ path = r##"../../actions/src/theme.rs"## ] mod foreign;
include_str!("../../actions/themes/\u{73}hell.json");
''')
        failures = self.failures()
        self.assertEqual(len(failures), 2, failures)
        self.assertTrue(all("spaces -> spaces forbidden" in f for f in failures))

    def test_inline_module_path_uses_rust_module_directory(self) -> None:
        self.write("spaces/irc/src/outer.rs", '''
mod nested {
    #[path = "../../../../actions/source.rs"] mod foreign;
    #[path = "local.rs"] mod local;
}
''')
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("spaces -> spaces forbidden", failures[0])

    def test_unknown_expressions_and_unowned_source_fail_closed(self) -> None:
        self.write("spaces/irc/src/lib.rs", '''
include_str!(concat!(env!("OTHER_ROOT"), "/theme.json"));
include_bytes!(unknown!());
#[path = unknown!()] mod foreign;
include_str!("../../../shared.rs");
''')
        self.assertEqual(len(self.failures()), 4)

    def test_shared_docs_are_data_not_a_source_layer_escape(self) -> None:
        self.include("products/subspace/src/main.rs", "docs/mcp.md")
        self.assertEqual(self.failures(), [])
        self.include("spaces/irc/src/lib.rs", "docs/shared.rs", "path")
        self.assertEqual(len(self.failures()), 1)
        self.include("spaces/irc/src/lib.rs", "docs/shared.md", "path")
        self.assertEqual(len(self.failures()), 1, "a module path is source, not shared data")

    def test_package_roots_skip_foreign_and_generated_trees_not_tests(self) -> None:
        for directory in ("target", "vendor", "foreign"):
            self.write(f"spaces/actions/{directory}/bad.rs", "include_str!(unknown!());")
        self.write("spaces/actions/foreign/Cargo.toml", "[package]\nname='foreign'\n")
        self.assertEqual(self.failures(), [])
        self.write("spaces/actions/examples/bad.rs", "include_str!(unknown!());")
        self.assertEqual(len(self.failures()), 1)

    def test_board_owned_crate_is_not_a_shared_client(self) -> None:
        directory = self.root / "spaces/irc/crates/support"
        dependency = {"name": "subspace-spaces-irc-crates-support", "kind": None,
                      "features": [], "path": str(directory)}
        self.packages[self.root / "spaces/irc"]["dependencies"] = [dependency]
        self.assertEqual(self.failures(), [])
        for source, kind in product(("spaces/actions", "clients/nett", "terminal/panel"), KINDS):
            with self.subTest(source=source, kind=kind):
                package = self.packages[self.root / source]
                package["dependencies"] = [{**dependency, "kind": kind}]
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("-> spaces forbidden", failures[0])
                package["dependencies"] = []

    def test_products_compose_boards_but_cannot_include_their_files(self) -> None:
        for kind in KINDS:
            self.depend("products/subspace", name="subspace-spaces-actions", kind=kind,
                        path=str(self.root / "spaces/actions"))
            self.assertEqual(self.failures(), [])
        for source, syntax in product(("src/main.rs", "tests/artwork.rs", "examples/art.rs", "build.rs"),
                                      ("include_str", "include_bytes", "include", "path")):
            file = "products/subspace/" + source
            with self.subTest(source=source, syntax=syntax):
                self.include(file, "spaces/actions/assets/workflow.svg", syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("products -> spaces forbidden (source include)", failures[0])
                (self.root / file).unlink()

    def test_include_macro_is_source_not_a_shared_docs_escape(self) -> None:
        self.include("products/subspace/src/main.rs", "docs/shared.md", "include")
        failures = self.failures()
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("unowned/outside source include", failures[0])

    def test_generated_icons_remain_supported_but_not_shared_build_helpers(self) -> None:
        self.include("products/subspace/build.rs", "products/build.rs", "include")
        self.assertEqual(len(self.failures()), 1)
        (self.root / "products/subspace/build.rs").unlink()
        self.write("spaces/actions/src/lib.rs", 'include!(concat!(env!("OUT_DIR"), "/icons.rs"));')
        self.assertEqual(self.failures(), [])
        self.write("spaces/actions/src/lib.rs",
                   'include!(concat!(env!("OUT_DIR"), "/../../spaces/irc/src/lib.rs"));')
        self.assertTrue(self.failures())

    def test_generated_output_filenames_keep_ascii_identifiers(self) -> None:
        for name, supported in (("_Icons_09az.rs", True), ("icons.rs", True),
                                ("icôns.rs", False), ("icons١.rs", False)):
            with self.subTest(name=name):
                self.write("spaces/actions/src/lib.rs",
                           f'include!(concat!(env!("OUT_DIR"), "/{name}"));')
                self.assertEqual(self.failures() == [], supported)

    def test_most_specific_owner_is_used_for_resources_and_source(self) -> None:
        self.packages[self.root] = {"name": "broad-fixture", "dependencies": []}
        original_group = CHECK.group
        with patch.object(CHECK, "group", side_effect=lambda path:
                          ("products", None) if path == self.root else original_group(path)):
            self.include("terminal/panel/src/lib.rs", "spaces/actions/crates/support/asset.svg")
            failures = self.failures()
            self.assertEqual(len(failures), 1, failures)
            self.assertIn("terminal -> spaces forbidden", failures[0])

    def test_inline_path_override_and_cfg_attr_path_fail_explicitly(self) -> None:
        self.write("clients/nett/src/lib.rs", '''
#[path = "custom"] pub(crate) mod inline { #[path = "local.rs"] mod local; }
#[cfg_attr(test, path = "elsewhere.rs")] mod conditional;
''')
        failures = self.failures()
        self.assertEqual(len(failures), 2, failures)
        self.assertTrue(all("unsupported" in failure for failure in failures))

    def test_retired_harness_packages_and_cargo_edges_fail_closed(self) -> None:
        retired = self.root / "prototypes/board-lab/managed-runtimes"
        for kind in KINDS:
            with self.subTest(kind=kind):
                self.depend("products/subspace", name="retired-harness", kind=kind,
                            path=str(retired))
                with self.assertRaisesRegex(ValueError, "Unclassified internal package"):
                    self.failures()

    def test_product_source_cannot_read_retired_harness_files(self) -> None:
        for syntax in ("include_str", "include_bytes", "path"):
            with self.subTest(syntax=syntax):
                self.include("products/subspace/src/main.rs",
                             "prototypes/board-lab/managed-runtimes/src/main.rs", syntax)
                failures = self.failures()
                self.assertEqual(len(failures), 1, failures)
                self.assertIn("unowned/outside source include", failures[0])


if __name__ == "__main__":
    unittest.main()
