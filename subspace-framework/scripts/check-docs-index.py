"""Check that the documentation index reaches every page and its links resolve."""

import importlib.util
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

TOOL = Path(__file__).resolve().parents[1]
INDEX = "docs/README.md"
# The Framework keeps no change records or include fragments.
UNLISTED: tuple[str, ...] = ()
GENERATED = "/obj/docs/"

SPEC = importlib.util.spec_from_file_location(
    "board_docs", Path(__file__).with_name("check-board-docs.py")
)
BOARD_DOCS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BOARD_DOCS)


def relative(tool: Path, path: Path) -> str:
    return path.resolve().relative_to(tool.resolve()).as_posix()


def required_pages(tool: Path) -> set[str]:
    """Every docs page and Framework Space guide."""
    pages = {relative(tool, path) for path in (tool / "docs").rglob("*.md")}
    pages = {page for page in pages if not page.startswith(UNLISTED)}
    return pages | {relative(tool, path) for path in (tool / "spaces").glob("*/help.md")}


def local_links(markdown: str) -> list[tuple[str, str, str]]:
    """Return (destination, path, fragment) for local links outside code."""
    text = BOARD_DOCS.strip_code_spans(BOARD_DOCS.prose(markdown))
    links = []
    for _, _, destination in BOARD_DOCS.LINK.findall(text):
        url = urlsplit(destination.strip("<>"))
        if not (url.scheme or url.netloc):
            links.append((destination, unquote(url.path), unquote(url.fragment)))
    return links


def link_errors(tool: Path, page: Path, markdown: str) -> tuple[list[str], list[Path]]:
    """Report broken local links and return the Markdown files they reach."""
    errors, targets = [], []
    for destination, path, fragment in local_links(markdown):
        if GENERATED in f"/{path}":
            continue
        target = (page.parent / path).resolve() if path else page.resolve()
        if not target.is_relative_to(tool.resolve()):
            # Another project's page, such as the Framework's: not checked here.
            continue
        if not target.is_file():
            errors.append(f"missing local link: {destination}")
            continue
        if target.suffix != ".md":
            continue
        if fragment and fragment not in BOARD_DOCS.anchors(target.read_text()):
            errors.append(f"missing heading: {destination}")
        targets.append(target)
    return errors, targets


def crawl(tool: Path) -> tuple[set[str], list[str]]:
    """Follow links from the index through docs pages, outside change records."""
    seen, errors, queue = set(), [], [tool / INDEX]
    while queue:
        page = queue.pop()
        name = relative(tool, page)
        if name in seen:
            continue
        seen.add(name)
        page_errors, targets = link_errors(tool, page, page.read_text())
        errors += [f"{name}: {error}" for error in page_errors]
        for target in targets:
            try:
                target_name = relative(tool, target)
            except ValueError:
                continue
            walk = target_name.startswith("docs/")
            if walk:
                queue.append(target)
            else:
                seen.add(target_name)
    return seen, errors


def check(tool: Path) -> list[str]:
    if not (tool / INDEX).is_file():
        return [f"missing {INDEX}"]
    reached, errors = crawl(tool)
    missing = sorted(required_pages(tool) - reached)
    return errors + [f"{page}: not linked from {INDEX} or a page it links" for page in missing]


if __name__ == "__main__":
    failures = check(TOOL)
    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        sys.exit(1)
    print("The documentation index reaches every page, and its local links resolve.")
