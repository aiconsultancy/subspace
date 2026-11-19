"""Fixtures for the package check, without Cargo."""

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "check_package", Path(__file__).with_name("check-package.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class PackageCheckTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.crate = Path(self.temp.name)

    def write(self, path: str, text: str) -> None:
        (self.crate / path).parent.mkdir(parents=True, exist_ok=True)
        (self.crate / path).write_text(text)

    def test_the_licence_must_ship_and_match(self) -> None:
        self.write("LICENSE", "Apache")
        self.assertEqual(CHECK.licence_errors(self.crate, ["LICENSE"], "Apache"), [])
        self.assertTrue(CHECK.licence_errors(self.crate, ["src/lib.rs"], "Apache"))
        self.assertTrue(CHECK.licence_errors(self.crate, ["LICENSE"], "MIT"))

    def test_each_third_party_file_needs_its_licence_and_a_notice_line(self) -> None:
        third = {"demo": {"vendor/x.js": "vendor/LICENSE"}}
        files = ["NOTICE", "vendor/x.js", "vendor/LICENSE"]
        self.write("NOTICE", "vendor/x.js: MIT")
        with patch.object(CHECK, "THIRD_PARTY", third):
            self.assertEqual(CHECK.notice_errors("demo", self.crate, files), [])
            self.assertTrue(CHECK.notice_errors("demo", self.crate, files[:2]))
            self.assertTrue(CHECK.notice_errors("demo", self.crate, files[1:]))
            self.write("NOTICE", "nothing named")
            self.assertTrue(CHECK.notice_errors("demo", self.crate, files))
            self.assertTrue(CHECK.notice_errors("other", self.crate, ["NOTICE"]))

    def test_files_that_credit_others_must_be_listed(self) -> None:
        self.write("src/a.rs", "//! Ported from someone's crate (MIT).")
        self.write("src/b.rs", "//! Re-exported from gpui-base.")
        self.write("LICENSE-X", "Copyright (c) Someone")
        with patch.object(CHECK, "THIRD_PARTY", {}):
            errors = CHECK.unlisted_third_party("demo", self.crate,
                                                ["src/a.rs", "src/b.rs", "LICENSE-X"])
        self.assertEqual(errors, ["src/a.rs credits third-party work but is not in THIRD_PARTY"])

    def test_litter_and_oversized_files_must_not_ship(self) -> None:
        self.write("big.bin", "x" * (CHECK.MAX_FILE_BYTES + 1))
        errors = CHECK.file_errors(self.crate, ["target/debug/x", ".DS_Store", "big.bin"])
        self.assertEqual(len(errors), 3, errors)

    def test_manifests_must_publish_with_a_description_and_the_licence(self) -> None:
        good = {"publish": None, "description": "x", "license": "Apache-2.0"}
        self.assertEqual(CHECK.manifest_errors(good), [])
        self.assertEqual(len(CHECK.manifest_errors({"publish": [], "license": "MIT"})), 3)


if __name__ == "__main__":
    unittest.main()
