"""The local upload tool only publishes a named, staged regular file."""
import base64
import hashlib
import importlib.util
import json
import os
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest.mock import patch

MODULE = Path(__file__).resolve().parents[1] / "lib/officer_upload.py"
spec = importlib.util.spec_from_file_location("officer_upload", MODULE)
u = importlib.util.module_from_spec(spec)
spec.loader.exec_module(u)


class OfficerFileTests(unittest.TestCase):
    def test_binary_upload_returns_verified_reference(self):
        with tempfile.NamedTemporaryFile(dir="/tmp", prefix="subspace-share-test-", suffix=".png") as f:
            name = Path(f.name).name.removeprefix("subspace-share-")
            data = bytes(range(256)) * 1024
            f.write(data)
            f.flush()
            upload_id = uuid.uuid4()
            ref = "UPL-" + base64.urlsafe_b64encode(upload_id.bytes).decode().rstrip("=")
            receipt = {"reference": ref, "sizeBytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}

            def capture(*args):
                body = args[4]
                self.assertEqual(base64.b64decode(body["base64"]), data)
                self.assertEqual(body["uploadId"],str(upload_id))
                return receipt

            with patch.object(u,"post",side_effect=capture):
                got = u.upload({"fileName":name,"uploadId":str(upload_id)}, "http","http://localhost:3001","officer")
            self.assertEqual(got,{"reference":ref,"sizeBytes":len(data),"fileName":name})

    def test_staging_accepts_the_chat_apis_text_extensions(self):
        with tempfile.TemporaryDirectory() as root:
            for extension, mime in (("md", "text/markdown"), ("markdown", "text/markdown"),
                                    ("txt", "text/plain"), ("text", "text/plain")):
                with self.subTest(extension=extension):
                    name = "notes." + extension
                    (Path(root) / ("subspace-share-" + name)).write_bytes(b"hello")
                    self.assertEqual(u.staged(name, 5, root), (mime, b"hello"))

    def test_only_the_officers_staged_regular_file_is_accepted(self):
        with tempfile.TemporaryDirectory() as root:
            file = Path(root)/"subspace-share-test.md"
            file.write_bytes(b"document")
            with self.assertRaises(ValueError):
                u.staged("../test.md", 20, root)
            with self.assertRaises(ValueError):
                u.staged("test.md", 2, root)
            os.link(file,Path(root)/"second")
            with self.assertRaises(ValueError):
                u.staged("test.md", 20, root)
            file.unlink()
            file.symlink_to(Path(root)/"second")
            with self.assertRaises(OSError):
                u.staged("test.md",20,root)


if __name__ == "__main__":
    unittest.main()
