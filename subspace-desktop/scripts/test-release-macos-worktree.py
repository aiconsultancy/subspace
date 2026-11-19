"""Check the macOS release fixture with a linked worktree's hook environment."""

import os
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    script = Path(__file__).with_name("test-release-macos.sh").resolve()
    environment = {
        name: value for name, value in os.environ.items() if not name.startswith("GIT_")
    }
    with tempfile.TemporaryDirectory(prefix="subspace-release-hook-") as temp:
        root = Path(temp)
        parent = root / "parent"
        linked = root / "linked"
        config = root / "gitconfig"
        config.write_text("[commit]\n\tgpgsign = true\n[gpg]\n\tprogram = fixture-gpg-must-not-run\n")
        environment |= {"GIT_CONFIG_GLOBAL": str(config), "GIT_CONFIG_NOSYSTEM": "1"}

        def git(cwd: Path, *args: str) -> str:
            return subprocess.check_output(
                ["git", "-C", str(cwd), *args], env=environment, text=True
            ).strip()

        git(root, "init", "-q", str(parent))
        git(parent, "-c", "commit.gpgsign=false", "-c", "user.name=Fixture", "-c",
            "user.email=fixture@example.test", "commit", "--allow-empty", "-qm", "baseline")
        git(parent, "worktree", "add", "--detach", "-q", str(linked))
        before = git(linked, "rev-parse", "HEAD")
        git_dir = Path(git(linked, "rev-parse", "--absolute-git-dir"))
        common = Path(git(linked, "rev-parse", "--path-format=absolute", "--git-common-dir"))
        hook_environment = environment | {
            "GIT_DIR": str(git_dir),
            "GIT_WORK_TREE": str(linked),
            "GIT_COMMON_DIR": str(common),
            "GIT_INDEX_FILE": str(git_dir / "index"),
            "GIT_OBJECT_DIRECTORY": str(common / "objects"),
        }
        subprocess.run(["bash", str(script)], cwd=linked, env=hook_environment, check=True)
        if git(linked, "rev-parse", "HEAD") != before or git(linked, "status", "--porcelain"):
            raise RuntimeError("The release fixture changed the calling worktree")


if __name__ == "__main__":
    main()
