"""Bounded token check for application icon buttons, not Rust type analysis.

Recognizes direct builders and simple local split chains. Macros, indirect
helpers, arbitrary aliases, dynamic labels and cross-function flows are not
resolved. Nested argument expressions are not attributed to their parent.
"""

from pathlib import Path

from architecture_source import closing, literal, tokens


# Navigation tabs that are not independent toolbar actions, as
# (file, function). The shared IconButton itself lives in the Framework, so
# no desktop file is exempt today.
EXCEPTIONS: set[tuple[str, str]] = set()
GLYPHS = set('×✕✖✓✔☑☐⋯›»←→↑↓⌄⌃＋−⟳⚙')


def test_source(path: Path) -> bool:
    return any(part in {'tests', 'test', 'examples'} or part.endswith('_tests')
               for part in path.parts) or path.stem in {'tests', 'test'} or path.stem.endswith('_tests')


def is_test_attribute(items: list, index: int) -> bool:
    return ([t.text for t in items[index:index + 5]] == ['#', '[', 'cfg', '(', 'test']
            and [t.text for t in items[index + 5:index + 7]] == [')', ']'])


def production_tokens(text: str) -> list:
    items = tokens(text)
    result = []
    index = 0
    while index < len(items):
        cfg_test = is_test_attribute(items, index)
        test_fn = [t.text for t in items[index:index + 4]] == ['#', '[', 'test', ']']
        after = index + (7 if cfg_test else 4)
        if (cfg_test or test_fn) and after < len(items) and items[after].text == 'pub':
            after += 1
            if after < len(items) and items[after].text == '(':
                after = closing(items, after) + 1
        if ((cfg_test or test_fn) and after < len(items)
                and items[after].text in {'mod', 'fn'}):
            start = after
            while start < len(items) and items[start].text not in {'{', ';'}:
                start += 1
            if start < len(items):
                index = closing(items, start) + 1 if items[start].text == '{' else start + 1
                continue
        result.append(items[index])
        index += 1
    return result


def aliases(items: list) -> set[str]:
    names = {'Button', 'InputGroupButton'}
    for index, item in enumerate(items):
        if item.text in names and [t.text for t in items[index + 1:index + 2]] == ['as']:
            if index + 2 < len(items) and items[index + 2].kind == 'word':
                names.add(items[index + 2].text)
    return names


def methods(items: list, start: int) -> tuple[list[tuple[str, list]], int]:
    """Read only methods in this chain, excluding nested argument methods."""
    result = []
    index = start
    while index + 2 < len(items) and items[index].text == '.' and items[index + 2].text == '(':
        end = closing(items, index + 2)
        result.append((items[index + 1].text, items[index + 3:end]))
        index = end + 1
    return result, index


def glyph_argument(items: list) -> bool:
    for token in items:
        if token.kind in {'string', 'raw'} and token.text.startswith(('"', 'r')):
            try:
                value = literal(token)
                # A trailing ellipsis names a dialog action, not an icon.
                if value.strip() == '…' or any(char in GLYPHS for char in value):
                    return True
            except ValueError:
                continue
    names = {token.text for token in items if token.kind == 'word'}
    return bool(names & {'Glyph', 'Icon', 'IconName', 'Svg', 'svg'})


def button_methods(chain: list[tuple[str, list]]) -> bool:
    return any(name == 'icon' or name in {'child', 'label'} and glyph_argument(args)
               for name, args in chain)


def icon_div(chain: list[tuple[str, list]]) -> bool:
    children = [args for name, args in chain if name == 'child']
    return (any(name in {'on_click', 'on_mouse_down', 'on_mouse_up'} for name, _ in chain)
            and len(children) == 1 and glyph_argument(children[0]))


def function_ranges(items: list) -> list[tuple[int, int, str]]:
    result = []
    for index, token in enumerate(items):
        if token.text != 'fn' or index + 1 >= len(items):
            continue
        start = index + 2
        while start < len(items) and items[start].text not in {'{', ';'}:
            start += 1
        if start < len(items) and items[start].text == '{':
            result.append((start, closing(items, start), items[index + 1].text))
    return result


def function_offsets(items: list, names: set[str], start: int, end: int) -> set[int]:
    failures = set()
    locals_ = set()
    for index in range(start + 1, end):
        word = items[index].text
        constructor = (word in names and [t.text for t in items[index + 1:index + 4]] == [':', ':', 'new']
                       and index + 4 < end and items[index + 4].text == '(')
        div = word == 'div' and index + 1 < end and items[index + 1].text == '('
        if constructor or div:
            opening = index + (4 if constructor else 1)
            finish = closing(items, opening)
            chain, _ = methods(items, finish + 1)
            # A common wrapper, `styled(Button::new(id), intent).icon(...)`.
            if (constructor and index >= 2 and items[index - 1].text == '('
                    and items[index - 2].text == 'styled'):
                outer_end = closing(items, index - 1)
                outer_chain, _ = methods(items, outer_end + 1)
                chain += outer_chain
            if constructor and button_methods(chain) or div and icon_div(chain):
                failures.add(items[index].offset)
            if constructor:
                # Recognize `let [mut] name = Button::new(...)` only.
                previous = [t.text for t in items[max(start, index - 4):index]]
                if len(previous) >= 3 and previous[-1] == '=' and 'let' in previous:
                    locals_.add(previous[-2])
        elif word in locals_ and index + 1 < end and items[index + 1].text == '.':
            chain, _ = methods(items, index + 1)
            if button_methods(chain):
                failures.add(items[index].offset)
    return failures


def violations(root: Path, file: Path, text: str) -> list[str]:
    path = file.relative_to(root)
    if test_source(path):
        return []
    items = production_tokens(text)
    names = aliases(items)
    offsets = set()
    try:
        for start, end, function in function_ranges(items):
            if (path.as_posix(), function) not in EXCEPTIONS:
                offsets.update(function_offsets(items, names, start, end))
    except ValueError as error:
        raise ValueError(f'{path}: {error}') from error
    return [f'{path}:{text.count(chr(10), 0, offset) + 1}: '
            'hand-rolled icon control: use shared IconButton'
            for offset in sorted(offsets)]
