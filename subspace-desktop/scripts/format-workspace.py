#!/usr/bin/env python3
"""Format owned workspace members without following vendored path dependencies."""

import json
import subprocess
import sys

for manifest in ["Cargo.toml", "terminal/runtime/Cargo.toml", "terminal/panel/Cargo.toml"]:
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--manifest-path", manifest, "--no-deps", "--format-version", "1"], text=True
    ))
    members = set(metadata["workspace_members"])
    packages = [package["name"] for package in metadata["packages"] if package["id"] in members]
    subprocess.run(["cargo", "fmt", "--manifest-path", manifest, "--package", *packages, *sys.argv[1:]], check=True)
