"""Check the Framework Spaces' guides, local links and embedded screenshot sources."""

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

TOOL = Path(__file__).resolve().parents[1]
LINK = re.compile(r"(!?)\[([^\]]*)\]\(([^\s)]+)(?:\s+\"[^\"]*\")?\)")
BACKTICKS = re.compile(r"`+")


def fence_opener(line: str) -> str:
    """Return the run of three or more backticks or tildes that opens a fence.

    As in CommonMark, a backtick fence's info string cannot contain a backtick, so a
    column-0 inline span such as ```example``` stays prose. Tilde fences have no such limit.
    """
    marker = line[:1]
    if marker not in ("`", "~"):
        return ""
    run = len(line) - len(line.lstrip(marker))
    if run < 3 or (marker == "`" and "`" in line[run:]):
        return ""
    return line[:run]


def closes_fence(line: str, opener: str) -> bool:
    """A closing fence repeats the opening character at least as often, then only spaces or tabs."""
    body = line.rstrip("\r")
    run = len(body) - len(body.lstrip(opener[0]))
    return run >= len(opener) and not body[run:].strip(" \t")


def prose(markdown: str) -> str:
    """Exclude fenced examples from the document contract in one pass over the lines.

    Lines split on "\\n" only, as the replaced regex did; str.splitlines also splits on
    form feeds and other separators. A removed fence leaves only its final "\\n".
    """
    kept, opener = [], ""
    lines = markdown.split("\n")
    for index, line in enumerate(lines):
        newline = "\n" if index < len(lines) - 1 else ""
        if opener:
            if closes_fence(line, opener):
                opener = ""
                kept.append(newline)
        else:
            opener = fence_opener(line)
            if not opener:
                kept.append(line + newline)
    return "".join(kept)


def strip_code_spans(text: str) -> str:
    """Remove code spans: a backtick run closes at the next run of the same length."""
    runs = [match.span() for match in BACKTICKS.finditer(text)]
    following, latest = [None] * len(runs), {}
    for index in range(len(runs) - 1, -1, -1):
        length = runs[index][1] - runs[index][0]
        following[index], latest[length] = latest.get(length), index
    pieces, cursor, index = [], 0, 0
    while index < len(runs):
        closer = following[index]
        if closer is None:
            index += 1
            continue
        pieces.append(text[cursor:runs[index][0]])
        cursor, index = runs[closer][1], closer + 1
    pieces.append(text[cursor:])
    return "".join(pieces)


def section_errors(markdown: str, required: list[str]) -> list[str]:
    """Require one title and the template's exact H2 sequence."""
    text = prose(markdown)
    errors = []
    if len(re.findall(r"^# \S.*$", text, re.M)) != 1:
        errors.append("expected one H1 title")
    if re.findall(r"^## (.+)$", text, re.M) != required:
        errors.append("H2 headings must match the board-guide template in order")
    return errors


def anchors(markdown: str) -> set[str]:
    """Return heading anchors for the plain-text headings used in guides."""
    headings = re.findall(r"^#{1,6} (.+)$", prose(markdown), re.M)
    return {re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-") for title in headings}


def markup_errors(markdown: str) -> list[str]:
    """Prevent placeholders from becoming raw HTML and hiding guide sections."""
    text = re.sub(r"<!--.*?-->", "", prose(markdown), flags=re.S)
    errors = []
    if re.search(r"^\s*<[A-Za-z]", text, re.M):
        errors.append("a line starts with raw HTML; keep code placeholders on one line")
    text = strip_code_spans(text)
    if re.search(r"<(?!https?://)[A-Za-z][^>]*>", text):
        errors.append("escape angle-bracket placeholders outside code spans")
    return errors


def inside_project(guide: Path, target: Path) -> bool:
    """Whether a link stays inside this project. Links to another project's
    pages, such as the Framework's, are not checked here."""
    root = TOOL.resolve()
    return not guide.resolve().is_relative_to(root) or target.resolve().is_relative_to(root)


def link_errors(guide: Path, markdown: str) -> list[str]:
    """Check local destinations and Markdown fragments without network access."""
    errors = []
    for _, _, destination in LINK.findall(prose(markdown)):
        url = urlsplit(destination.strip("<>"))
        if url.scheme or url.netloc:
            continue
        target = guide.parent / unquote(url.path) if url.path else guide
        if not inside_project(guide, target):
            continue
        if not target.is_file():
            errors.append(f"missing local link: {destination}")
        elif url.fragment and target.suffix == ".md":
            if unquote(url.fragment) not in anchors(target.read_text()):
                errors.append(f"missing heading: {destination}")
    return errors


def image_errors(board: Path, markdown: str) -> list[str]:
    """Require named, bounded WebP files embedded by their owning board."""
    sources = "\n".join(path.read_text() for path in (board / "src").rglob("*.rs"))
    errors = []
    for image, alt, destination in LINK.findall(prose(markdown)):
        if not image:
            continue
        target = board / destination
        if not alt.strip():
            errors.append(f"image needs alt text: {destination}")
        if not re.fullmatch(r"help-assets/[\w-]+\.webp", destination):
            errors.append(f"image must use help-assets/<id>.webp: {destination}")
        elif f'include_bytes!("../{destination}")' not in sources:
            errors.append(f"image is not embedded by this board: {destination}")
        if target.is_file() and target.stat().st_size > 512 * 1024:
            errors.append(f"image exceeds 512 KiB: {destination}")
    return errors


def board_errors(board: Path, required: list[str]) -> list[str]:
    """Validate a guide against its own source and the shared template."""
    guide = board / "help.md"
    if not guide.is_file():
        return ["missing help.md"]
    markdown = guide.read_text()
    sources = "\n".join(path.read_text() for path in (board / "src").rglob("*.rs"))
    errors = section_errors(markdown, required)
    if 'include_str!("../help.md")' not in sources:
        errors.append("help.md is not registered by its owning board")
    return errors + markup_errors(markdown) + link_errors(guide, markdown) + image_errors(board, markdown)


def check(tool: Path) -> list[str]:
    """Check every Framework Space's guide, using the template as authority."""
    template = (tool / "docs/board-guide-template.md").read_text()
    required = re.findall(r"^## (.+)$", template, re.M)
    if len(required) != 10:
        return ["template must declare the ten required H2 headings"]
    boards = sorted(path.parent for path in (tool / "spaces").glob("*/Cargo.toml"))
    if not boards:
        return ["no Framework Spaces found"]
    return [f"{board.name}: {error}" for board in boards
            for error in board_errors(board, required)]


if __name__ == "__main__":
    failures = check(TOOL)
    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        sys.exit(1)
    print("Board guide headings, local links and embedded image sources pass.")
