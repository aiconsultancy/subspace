"""Bounded Rust source-include policy, not a Rust parser.

Scan include!, include_str!, include_bytes! and #[path] in package-owned .rs files,
including tests/examples/build scripts, without
evaluating cfg. Recognize literals/raw strings and concat! of literals with
env!("CARGO_MANIFEST_DIR"). Other expressions fail closed. Comments and string
examples are ignored; macro-generated code and generated/vendor trees are not
expanded. #[path] uses the file directory plus Rust's inline-module directory.
Inline modules with a path override are rejected rather than guessed.
Only unowned docs/*.md data is shared. Direct OUT_DIR generated filenames are
not source files. Other unowned Rust, shared paths and paths outside the
Framework fail closed. Package ownership always takes precedence. The facade's
Cargo composition does not permit Space includes.
"""

import hashlib
import os
import re
from pathlib import Path
from typing import Callable, NamedTuple


TOKEN = re.compile(
    r'(?P<raw>(?:br|cr|r)(?P<hashes>\#*)".*?"(?P=hashes))'
    r'|(?P<string>(?:b|c)?"(?:\\.|[^"\\])*")'
    r"|(?P<char>(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])')"
    r'|(?P<word>[A-Za-z_][A-Za-z_0-9]*)|(?P<punct>[^\s])',
    re.DOTALL,
)


class Token(NamedTuple):
    kind: str
    text: str
    offset: int


def tokens(text: str) -> list[Token]:
    result = []
    position = 0
    while position < len(text):
        if text.startswith("//", position):
            end = text.find("\n", position)
            position = len(text) if end < 0 else end
        elif text.startswith("/*", position):
            depth = 1
            position += 2
            while position < len(text) and depth:
                pair = text[position:position + 2]
                depth += (pair == "/*") - (pair == "*/")
                position += 2 if pair in {"/*", "*/"} else 1
        elif text[position].isspace():
            position += 1
        else:
            match = TOKEN.match(text, position)
            result.append(Token(match.lastgroup, match.group(), position))
            position = match.end()
    return result


def literal(token: Token) -> str:
    if token.kind == "raw" and token.text.startswith("r"):
        return token.text[token.text.index('"') + 1:token.text.rindex('"')]
    if token.kind != "string" or not token.text.startswith('"'):
        raise ValueError("expected Rust string literal")
    escapes = {'n': '\n', 'r': '\r', 't': '\t', '0': '\0', '\\': '\\', '"': '"', "'": "'"}

    def decode(match: re.Match) -> str:
        value = match.group()[1:]
        if value in escapes:
            return escapes[value]
        if value.startswith(("x", "u{")):
            digits = value[1:] if value.startswith("x") else value[2:-1]
            return chr(int(digits.replace("_", ""), 16))
        if value[0] in "\r\n":
            return ""
        raise ValueError("unsupported Rust string escape")

    return re.sub(r'\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|\r?\n\s*|.)',
                  decode, token.text[1:-1])


def expression(items: list[Token], directory: Path) -> str:
    """Resolve only the small constant-expression vocabulary used by this repo."""
    if items and items[-1].text == ",":
        items = items[:-1]
    if len(items) == 1:
        return literal(items[0])
    words = [item.text for item in items]
    if words[:3] == ["env", "!", "("] and words[-1:] == [")"]:
        if len(items) == 5 and literal(items[3]) == "CARGO_MANIFEST_DIR":
            return str(directory)
    if words[:3] == ["concat", "!", "("] and words[-1:] == [")"]:
        parts, start, depth = [], 3, 0
        for index in range(3, len(items) - 1):
            word = items[index].text
            if word == "," and depth == 0:
                parts.append(expression(items[start:index], directory))
                start = index + 1
            depth += (word in {"(", "[", "{"}) - (word in {")", "]", "}"})
        if start < len(items) - 1:
            parts.append(expression(items[start:-1], directory))
        if parts:
            return "".join(parts)
    raise ValueError("unresolved include expression; use literals or concat! with CARGO_MANIFEST_DIR")


def closing(items: list[Token], start: int) -> int:
    pairs = {"(": ")", "[": "]", "{": "}"}
    stack = []
    for index in range(start, len(items)):
        word = items[index].text
        if word in pairs:
            stack.append(pairs[word])
        elif word in pairs.values():
            if not stack or stack.pop() != word:
                break
            if not stack:
                return index
    raise ValueError("unbalanced include delimiters")


def generated_output(items: list[Token]) -> bool:
    """Recognize a direct generated filename, not a path back into source."""
    if items and items[-1].text == ",":
        items = items[:-1]
    if [item.text for item in items[:6]] != ["concat", "!", "(", "env", "!", "("]:
        return False
    if len(items) != 11 or [item.text for item in items[7:9]] != [")", ","]:
        return False
    return (literal(items[6]) == "OUT_DIR" and items[10].text == ")"
            and re.fullmatch(r"/[A-Za-z_]\w*\.rs", literal(items[9]), re.ASCII) is not None)


def inline_module(items: list[Token], start: int) -> bool:
    """Check whether the following declaration opens an inline module body."""
    declaration = []
    for token in items[start:]:
        declaration.append(token.text)
        if token.text in {";", "{"}:
            break
    return "mod" in declaration and declaration[-1:] == ["{"]


def path_attribute_start(items: list[Token], index: int, end: int) -> int | None:
    """Locate a direct path expression, rejecting unsupported module attributes."""
    attribute = items[index + 2:end]
    if [t.text for t in attribute[:2]] == ["path", "="]:
        if inline_module(items, end + 1):
            raise ValueError("inline module #[path] override is unsupported")
        return index + 4
    if any([t.text for t in attribute[i:i + 2]] == ["path", "="]
           for i in range(len(attribute) - 1)):
        raise ValueError("conditional/nested path attribute is unsupported")
    return None


def reference_at(items: list[Token], index: int, file_parent: Path,
                 module_base: Path, directory: Path) -> tuple[int, Path | str | None, bool] | None:
    """Resolve one reference and its recovery position, skipping generated output."""
    word = items[index].text
    end = None
    try:
        if word in {"include", "include_str", "include_bytes"} and index + 2 < len(items) and items[index + 1].text == "!":
            end = closing(items, index + 2)
            start, base = index + 3, file_parent
        elif word == "#" and index + 1 < len(items) and items[index + 1].text == "[":
            end = closing(items, index + 1)
            start, base = path_attribute_start(items, index, end), module_base
        else:
            return None
        if start is None:
            return None
        if word == "include" and generated_output(items[start:end]):
            return end + 1, None, False
        target = (base / expression(items[start:end], directory)).resolve()
        return end + 1, target, word in {"#", "include"}
    except ValueError as error:
        return end + 1 if end is not None else index + 1, str(error), word == "#"


def update_modules(items: list[Token], index: int, braces: list[str | None],
                   modules: list[str]) -> None:
    """Track inline module names and their enclosing braces at the current token."""
    word = items[index].text
    if word == "{":
        module = items[index - 1].text if index >= 2 and items[index - 2].text == "mod" else None
        braces.append(module)
        if module:
            modules.append(module)
    elif word == "}" and braces:
        if braces.pop():
            modules.pop()


def references(text: str, file: Path, directory: Path) -> list[tuple[int, Path | str, bool]]:
    """Collect source references or resolution errors with inline-module context."""
    items = tokens(text)
    result, braces, modules = [], [], []
    file_base = file.parent if file.stem in {"lib", "main", "mod", "build"} else file.with_suffix("")
    index = 0
    while index < len(items):
        module_base = file_base.joinpath(*modules) if modules else file.parent
        reference = reference_at(items, index, file.parent, module_base, directory)
        if reference is not None:
            next_index, target, module = reference
            if target is not None:
                result.append((items[index].offset, target, module))
            index = next_index
            continue
        update_modules(items, index, braces, modules)
        index += 1
    return result


def source_files(directory: Path) -> list[Path]:
    result = []
    for parent, directories, files in os.walk(directory):
        directories[:] = sorted(name for name in directories
                                if name not in {"target", "vendor", ".git", "generated"}
                                and not (Path(parent) / name / "Cargo.toml").is_file()
                                and not (Path(parent) / name).is_symlink())
        result.extend(Path(parent) / name for name in sorted(files) if name.endswith(".rs"))
    return result


def implementation_imports(text: str, names: set[str]) -> list[Token]:
    """Find selected implementation paths, ignoring comments and quoted examples."""
    items = tokens(text)
    return [item for index, item in enumerate(items)
            if item.kind == "word" and item.text in names
            and ([t.text for t in items[index + 1:index + 3]] == [":", ":"]
                 or [t.text for t in items[max(0, index - 2):index]] == ["extern", "crate"])]


def implementation_violations(root: Path, file: Path, text: str, layer: str) -> list[str]:
    """Reject shared-layer implementations that belong to their feature owners."""
    from icon_button_source import violations as icon_button_violations

    failures = settings_violations(root, file, text, layer)
    failures.extend(path_lookup_violations(root, file, text))
    failures.extend(switch_violations(root, file, text))
    failures.extend(icon_button_violations(root, file, text))
    if layer != "board-kit":
        return failures
    failures.extend(f"{file.relative_to(root)}:{text.count(chr(10), 0, item.offset) + 1}: "
                    f"board-kit implementation import {item.text} is forbidden"
                    for item in implementation_imports(text, {"subspace_agent", "subspace_chat"}))
    return failures + search_feature_violations(root, file, text)


def settings_violations(root: Path, file: Path, text: str, layer: str) -> list[str]:
    """Keep connection dispatch registration-driven, not keyed to known board IDs."""
    location = file.relative_to(root)
    if layer != "host" or ("settings" not in location.parts and file.stem != "settings"):
        return []
    # Test fixtures name boards to check the real editor; they do not supply UI dispatch.
    if any(part == "tests" or part.endswith("_tests") for part in location.with_suffix("").parts):
        return []
    if file.name == "legacy.rs":
        return [f"{location}: legacy settings dispatcher is forbidden"]
    items = tokens(text)
    offsets = settings_dispatch_offsets(items)
    return [f"{location}:{text.count(chr(10), 0, offset) + 1}: "
            "board-ID settings dispatch is forbidden; declare fields or capabilities in registration"
            for offset in sorted(offsets)]


# Files that read PATH for a reason other than finding a tool.
PATH_LOOKUP_EXEMPTIONS = {
    # Puts a local agent's own folder first in the PATH of its child process.
    "runtimes/agent/src/runtime/child.rs",
}


def path_lookup_violations(root: Path, file: Path, text: str) -> list[str]:
    """Find external tools with subspace_core::path::find_tool, not ad hoc PATH searches."""
    location = file.relative_to(root)
    if (location.parts[:4] == ("services", "core", "src", "path")
            or location.as_posix() in PATH_LOOKUP_EXEMPTIONS):
        return []
    items = tokens(text)
    failures = []
    for index, item in enumerate(items):
        following = [t.text for t in items[index + 1:index + 4]]
        split = item.text == "split_paths" and following[:1] == ["("]
        read = (item.text == "var_os" and following[:1] == ["("]
                and len(following) > 1 and following[1] == '"PATH"')
        if item.kind == "word" and (split or read):
            failures.append(f"{location}:{text.count(chr(10), 0, item.offset) + 1}: "
                            "PATH lookup outside subspace_core::path is forbidden; use find_tool")
    return failures


def switch_violations(root: Path, file: Path, text: str) -> list[str]:
    """Build switches with dashboard::toggle::switch, which applies the on-track contrast rule."""
    location = file.relative_to(root)
    toggle = ("board-kit", "dashboard", "src", "toggle")
    if location.with_suffix("").parts[:4] == toggle:
        return []
    items = tokens(text)
    direct = [f"{location}:{text.count(chr(10), 0, item.offset) + 1}: "
              "direct Switch::new is forbidden; use subspace_dashboard::toggle::switch"
              for index, item in enumerate(items)
              if item.kind == "word" and item.text == "Switch"
              and [t.text for t in items[index + 1:index + 4]] == [":", ":", "new"]]
    return direct + toolbar_switch_violations(location, text, items)


# Explicit implementation paths, not a filename glob or a ban on board forms.
# Review this list when moving toolbar construction into a new source file.
TOOLBAR_SWITCH_FILES = {
    "host/app/src/app/toolbar.rs",
    "host/app/src/app/toolbar/chrome.rs",
    "host/app/src/app/toolbar/globals.rs",
    "host/chat/src/agent_chat/toolbar.rs",
    "board-kit/dashboard/src/panel/toolbar.rs",
    "board-kit/dashboard/src/log_view/view/toolbar.rs",
    "board-kit/dashboard/src/native_grid/view/controls/toolbar.rs",
}
TOOLBAR_SWITCH_FUNCTIONS = {
    "host/search-ui/src/search_view/view.rs": "scope_bar",
    "board-kit/dashboard/src/log_view/view.rs": "follow_control",
}


def toolbar_switch_violations(location: Path, text: str, items: list[Token]) -> list[str]:
    """Reject direct toggle::switch calls only in the inventoried toolbar regions.

    Comments, strings and unrelated methods are ignored. Aliases, indirect helper
    calls, macro expansion and newly moved files are not resolved. Mixed views
    use the existing bounded function scanner, not a whole-file restriction.
    """
    from icon_button_source import function_ranges

    path = location.as_posix()
    if path in TOOLBAR_SWITCH_FILES:
        ranges = [(0, len(items))]
    elif path in TOOLBAR_SWITCH_FUNCTIONS:
        ranges = [(start, end) for start, end, name in function_ranges(items)
                  if name == TOOLBAR_SWITCH_FUNCTIONS[path]]
    else:
        return []
    return [f"{location}:{text.count(chr(10), 0, item.offset) + 1}: "
            "toolbar switch is forbidden; use IconButton::latched(bool)"
            for index, item in enumerate(items)
            if item.kind == "word" and item.text == "toggle"
            and [t.text for t in items[index + 1:index + 5]] == [":", ":", "switch", "("]
            and any(start <= index < end for start, end in ranges)]


def settings_dispatch_offsets(items: list[Token]) -> set[int]:
    """Recognize comparisons, match arms and matches! with migrated board literals."""
    direct = {item.offset for index, item in enumerate(items)
              if settings_board_literal(item) and settings_dispatch_operator(items, index)}
    return direct | settings_matches_offsets(items)


def settings_board_literal(item: Token) -> bool:
    """Keep credential-section arguments valid while recognizing board literals."""
    board_ids = {"actions", "github", "github-copilot-agents"}
    return (item.kind in {"string", "raw"} and item.text.startswith(('"', 'r'))
            and literal(item) in board_ids)


def settings_dispatch_operator(items: list[Token], index: int) -> bool:
    """Inspect the operators directly beside a known board literal."""
    before = [token.text for token in items[max(0, index - 2):index]]
    after = [token.text for token in items[index + 1:index + 3]]
    return (before in (["=", "="], ["!", "="])
            or after in (["=", "="], ["!", "="], ["=", ">"])
            or "|" in before[-1:] + after[:1])


def settings_matches_offsets(items: list[Token]) -> set[int]:
    """Recognize board-ID patterns inside matches! without parsing Rust."""
    result = set()
    for index, item in enumerate(items):
        if item.text == "matches" and [t.text for t in items[index + 1:index + 3]] == ["!", "("]:
            end = closing(items, index + 2)
            result.update(token.offset for token in items[index + 3:end]
                          if settings_board_literal(token))
    return result


def search_feature_violations(root: Path, file: Path, text: str) -> list[str]:
    """Keep the retained SearchView feature out of board-kit, including re-exports."""
    location = file.relative_to(root)
    if file.stem == "search_view" or "search_view" in location.parts:
        return [f"{location}: board-kit search UI implementation source is forbidden"]
    forbidden = {"SearchView", "SearchViewEvent", "search_view", "subspace_search_ui"}
    return [f"{location}:{text.count(chr(10), 0, item.offset) + 1}: "
            f"board-kit search UI implementation {item.text} is forbidden"
            for item in tokens(text) if item.kind == "word" and item.text in forbidden]


def package_violations(root: Path, directory: Path, owners: list[Path],
                       group: Callable, allowed: Callable) -> list[str]:
    """Report forbidden or unowned source includes within one package."""
    failures = []
    for file in source_files(directory):
        text = file.read_text(encoding="utf-8")
        failures.extend(implementation_violations(root, file, text, group(directory)[0]))
        for offset, target, module in references(text, file, directory):
            location = f"{file.relative_to(root)}:{text.count(chr(10), 0, offset) + 1}"
            if isinstance(target, str):
                failures.append(f"{location}: {target}")
                continue
            owner = next((p for p in owners if target.is_relative_to(p)), None)
            if owner is not None and not source_allowed(directory, owner, group, allowed):
                failures.append(f"{location} -> {target.relative_to(root)}: "
                                f"{group(directory)[0]} -> {group(owner)[0]} forbidden (source include)")
            elif owner is None and not shared_source(root, directory, file, target, module):
                failures.append(f"{location} -> {target}: unowned/outside source include")
    return failures


def source_allowed(directory: Path, owner: Path, group: Callable, allowed: Callable) -> bool:
    """The facade re-exports Space APIs through Cargo, never includes Space files."""
    if group(directory)[0] == "facade" and group(owner)[0] == "spaces":
        return False
    return allowed(directory, owner)


def shared_source(root: Path, directory: Path, file: Path, target: Path, module: bool) -> bool:
    """Allow the Framework's shared Markdown data, such as the component
    catalogue, and the facade's own README (its doctests), which it packages."""
    if directory == root / "src" and target == root / "README.md":
        return not module
    return not module and target.is_relative_to(root / "docs") and target.suffix == ".md"


def artwork_files(directory: Path) -> list[Path]:
    """Discover owned image resources without enumerating board identities."""
    extensions = {".svg", ".png", ".webp", ".jpg", ".jpeg", ".ico", ".icns"}
    return [file for folder in (directory / "assets", directory / "help-assets")
            if folder.is_dir() for file in folder.rglob("*")
            if file.is_file() and file.suffix.lower() in extensions]


def artwork_violations(root: Path, packages: dict[Path, dict], group: Callable) -> list[str]:
    """Reject copied/renamed board artwork in host, plus the migrated GitHub logo."""
    board_art = [file for directory in packages if group(directory)[0] == "spaces"
                 for file in artwork_files(directory)]
    names = {file.name for file in board_art}
    digests = {hashlib.sha256(file.read_bytes()).digest() for file in board_art}
    legacy = {root / "host/app/assets/logos/github.svg"}
    return [f"{file.relative_to(root)}: board-owned artwork is forbidden in host defaults"
            for directory in packages if group(directory)[0] == "host"
            for file in artwork_files(directory)
            if file in legacy or file.name in names
            or hashlib.sha256(file.read_bytes()).digest() in digests]


def violations(root: Path, packages: dict[Path, dict], group: Callable,
               allowed: Callable) -> list[str]:
    """Collect source-include violations using the most specific package owner."""
    owners = sorted(packages, key=lambda path: len(path.parts), reverse=True)
    return artwork_violations(root, packages, group) + [failure for directory in owners
            for failure in package_violations(root, directory, owners, group, allowed)]
