"""Exercise the web Space packaging script on the Weather example and isolated fixtures."""

import contextlib
import importlib.util
import io
import json
import os
import shutil
import tempfile
import unittest
import zipfile
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "package_web_space", Path(__file__).with_name("package-web-space.py")
)
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)
TOOL = Path(__file__).resolve().parents[1]
WEATHER = TOOL / "web-spaces/examples/weather"
TEMPLATE = TOOL / ".agents/skills/web-space/assets/template"


def write(root: Path, name: str, text: str = "x") -> Path:
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    return path


def manifest(**fields) -> str:
    return json.dumps({"name": "Notes", "version": "1.0.0", **fields})


def space(root: Path, text: str | None = None) -> Path:
    folder = root / "notes"
    write(folder, "index.html", "<!doctype html>")
    write(folder, "subspace-board.json", manifest() if text is None else text)
    return folder


class PackageTests(unittest.TestCase):
    def run_script(self, *argv: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = PACKAGE.main(list(argv))
        return status, out.getvalue(), err.getvalue()

    def fails(self, folder: Path, expected: str, *extra: str) -> None:
        status, _, err = self.run_script(str(folder), "--check", *extra)
        self.assertEqual(status, 1, err)
        self.assertIn(expected, err)

    def test_weather_packages_reproducibly_without_dev_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            first, second = Path(temp, "a.zip"), Path(temp, "b.zip")
            status, out, err = self.run_script(str(WEATHER), str(first))
            self.assertEqual(status, 0, err)
            self.assertIn("web Space: Weather", out)
            self.assertIn(f"wrote {first.resolve()}", out)
            copy = Path(temp, "weather")
            shutil.copytree(WEATHER, copy)
            os.utime(copy / "index.html", (0, 0))
            self.assertEqual(self.run_script(str(copy), str(second))[0], 0)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                names = archive.namelist()
                self.assertEqual(names, sorted(names))
                self.assertIn("index.html", names)
                self.assertIn("subspace-board.json", names)
                self.assertFalse([name for name in names if name.startswith("test/")])
                self.assertNotIn("mise.toml", names)
                for info in archive.infolist():
                    self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
                    self.assertEqual(info.external_attr >> 16, 0o100644)

    def test_skill_template_is_a_valid_web_space(self) -> None:
        status, out, err = self.run_script(str(TEMPLATE), "--check")
        self.assertEqual(status, 0, err)
        self.assertIn("files: 4,", out)

    def test_include_keeps_a_default_exclusion(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            write(folder, "test/a.test.js")
            write(folder, ".env", "SECRET=1")
            out = Path(temp, "out.zip")
            self.assertEqual(self.run_script(str(folder), str(out), "--include", "test/*")[0], 0)
            with zipfile.ZipFile(out) as archive:
                self.assertIn("test/a.test.js", archive.namelist())
                self.assertNotIn(".env", archive.namelist())

    def test_exclude_leaves_out_another_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            write(folder, "drafts/a.html")
            out = Path(temp, "out.zip")
            self.assertEqual(self.run_script(str(folder), str(out), "--exclude", "drafts")[0], 0)
            with zipfile.ZipFile(out) as archive:
                self.assertEqual(archive.namelist(), ["index.html", "subspace-board.json"])

    def test_required_root_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            (folder / "index.html").unlink()
            self.fails(folder, "needs a regular, readable index.html")
            write(folder, "index.html")
            (folder / "subspace-board.json").unlink()
            self.fails(folder, "needs a regular, readable subspace-board.json")

    def test_manifest_rules_match_the_app(self) -> None:
        cases = [
            ("[]", "expected a JSON object."),
            ("{", "invalid JSON"),
            (json.dumps({"version": "1"}), "expected name to be text of 1 to 60 characters."),
            (json.dumps({"name": "N"}), "expected version to be text of 1 to 40 characters."),
            (manifest(permissions="all"), "expected permissions to be a list."),
            (manifest(permissions=[1]), "expected permissions[0] to be text."),
            (
                manifest(settings=[{"key": "1x", "label": "L", "type": "text"}]),
                "settings[0]: expected key to be 1 to 40 letters",
            ),
            (
                manifest(settings=[{"key": "a", "label": "L", "type": "colour"}]),
                "expected type to be text, url, number, boolean, select or secret.",
            ),
            (
                manifest(settings=[{"key": "a", "label": "L", "type": "secret", "default": "x"}]),
                "a secret cannot have a default.",
            ),
            (
                manifest(settings=[{"key": "a", "label": "L", "type": "select", "options": []}]),
                "expected 1 to 50 options of 1 to 60 characters.",
            ),
            (
                manifest(
                    settings=[
                        {"key": "a", "label": "L", "type": "select", "options": ["x"], "default": "y"}
                    ]
                ),
                "expected default to be one of the options.",
            ),
            (
                manifest(settings=[{"key": "a", "label": "L", "type": "number", "default": True}]),
                "expected default to be a number.",
            ),
            (
                manifest(
                    settings=[
                        {"key": "a", "label": "L", "type": "text"},
                        {"key": "a", "label": "M", "type": "text"},
                    ]
                ),
                "settings[1]: the key a is used twice.",
            ),
            (
                manifest(settings=[{"key": f"k{n}", "label": "L", "type": "text"} for n in range(33)]),
                "expected at most 32 settings.",
            ),
        ]
        for text, expected in cases:
            with self.subTest(expected=expected), tempfile.TemporaryDirectory() as temp:
                folder = space(Path(temp), text)
                self.fails(folder, expected)
                self.fails(folder, "error: subspace-board.json: ")

    def test_manifest_size_and_unknown_permission(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp), manifest(permissions=["badge", "camera"]))
            status, out, _ = self.run_script(str(folder), "--check")
            self.assertEqual(status, 0)
            self.assertIn("permissions: badge", out)
            self.assertIn('warning: Unknown permission "camera" ignored.', out)
            write(folder, "subspace-board.json", manifest(pad=" " * 256 * 1024))
            self.fails(folder, "subspace-board.json is larger than 256 KiB.")

    def test_unsafe_names_links_and_collisions(self) -> None:
        for name in ["caf\u00e9.js", "CON.txt", "trailing.", "a:b.js"]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temp:
                folder = space(Path(temp))
                write(folder, name)
                self.fails(folder, "unsafe or unsupported filename")
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            (folder / "link.html").symlink_to(folder / "index.html")
            self.fails(folder, "Package links and special files are not supported.")
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            write(folder, "Index.HTML")
            if len(os.listdir(folder)) == 3:
                self.fails(folder, "duplicate or platform-colliding paths")

    def test_depth_and_size_limits(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            write(folder, "/".join(["d"] * 32) + "/f.js")
            self.fails(folder, "The package exceeds 32 directory levels.")
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            with open(folder / "big.bin", "wb") as big:
                big.truncate(32 * 1024 * 1024 + 1)
            self.fails(folder, "A package file exceeds 32 MiB.")

    def test_output_must_be_outside_the_folder(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            folder = space(Path(temp))
            status, _, err = self.run_script(str(folder), str(folder / "out.zip"))
            self.assertEqual(status, 1)
            self.assertIn("Write the ZIP outside the web Space folder.", err)


if __name__ == "__main__":
    unittest.main()
