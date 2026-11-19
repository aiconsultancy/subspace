"""Read private operator files and install cloud credentials as the agent."""

import base64
from contextlib import contextmanager
import os
from pathlib import Path
import re
import stat
import tempfile


DIRECTORY = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
NAME = re.compile(r"[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}")
MAX_BYTES = 1024 * 1024


def private_file(path, optional=False, dir_fd=None):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=dir_fd)
    except FileNotFoundError:
        if optional:
            return None
        raise ValueError(f"Missing private file: {path}.") from None
    except OSError:
        raise ValueError(f"Cannot open private file: {path}.") from None
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077:
            raise ValueError(f"Expected a regular, owner-only file: {path}.")
        contents = stream.read(MAX_BYTES + 1)
        if not 0 < len(contents) <= MAX_BYTES:
            raise ValueError(f"Private file is empty or too large: {path}.")
        return contents


@contextmanager
def private_directory(path, parent=None):
    try:
        fd = os.open(path, DIRECTORY, dir_fd=parent)
    except FileNotFoundError:
        yield None
        return
    except OSError:
        raise ValueError(f"Expected a real secrets directory: {path}.") from None
    try:
        if os.fstat(fd).st_mode & 0o022:
            raise ValueError(f"Secrets directory must exclude group and other writes: {path}.")
        yield fd
    finally:
        os.close(fd)


def collect_cloud(root, agent_ids):
    result = {}
    for agent in agent_ids:
        with private_directory(agent, root) as agent_dir:
            if agent_dir is None:
                continue
            files = {}
            for name in ("kubeconfig", "database-dsn"):
                if (contents := private_file(name, optional=True, dir_fd=agent_dir)) is not None:
                    files[name] = contents
            with private_directory("cloud", agent_dir) as cloud:
                if cloud is not None:
                    names = os.listdir(cloud)
                    if len(names) > 32 or any(not NAME.fullmatch(name) or name in files for name in names):
                        raise ValueError(f"Invalid or duplicate cloud credential filenames for {agent}.")
                    files.update((name, private_file(name, dir_fd=cloud)) for name in names)
            if files:
                result[agent] = files
    return result


def write_private(destination, contents):
    """Install a private file atomically, preserving a correct existing file."""
    try:
        info = destination.lstat()
        if (stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid()
                and stat.S_IMODE(info.st_mode) == 0o600 and private_file(destination) == contents):
            return
    except FileNotFoundError:
        pass
    with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as stream:
        temp = Path(stream.name)
        try:
            stream.write(contents)
            stream.close()
            temp.replace(destination)
        finally:
            temp.unlink(missing_ok=True)


def agent_directory(*parts):
    directory = Path.home()
    for part in parts:
        directory = directory / part
        directory.mkdir(mode=0o700, exist_ok=True)
        info = directory.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid():
            raise ValueError("Installer files need a real, agent-owned directory.")
    return directory


def provision_cloud(files, prune=False):
    """Run with the destination user's identity, including on upgrade."""
    os.umask(0o077)
    directory = agent_directory(".config", "subspace", "cloud")
    directory.chmod(0o700)

    manifest = directory / ".installer-files"
    previous = private_file(manifest, optional=True)
    old_names = previous.decode().splitlines() if previous else []
    if any(not NAME.fullmatch(name) for name in [*old_names, *files]):
        raise ValueError("Invalid cloud credential filename.")
    for name, value in files.items():
        write_private(directory / name, base64.b64decode(value, validate=True))
    names = set(files)
    if prune:
        for name in set(old_names) - names:
            (directory / name).unlink(missing_ok=True)
    else:
        names.update(old_names)
    if names:
        write_private(manifest, ("\n".join(sorted(names)) + "\n").encode())
    else:
        manifest.unlink(missing_ok=True)
