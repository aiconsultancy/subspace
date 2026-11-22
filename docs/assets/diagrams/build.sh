#!/usr/bin/env bash
# Build overview-dark.svg and overview-light.svg from overview.src.svg.
#
# The source keeps its text as text so it stays editable. A README image
# cannot load fonts, so the built files have every glyph outlined: Chrome
# prints the source to PDF with the app's own Inter and JetBrains Mono, and
# pdftocairo turns that PDF into an SVG of paths.
#
# Needs Google Chrome or Chromium, pdftocairo (Poppler) and Python 3.
# Set CHROME to the browser binary if it is not found.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
fonts="$here/../../../subspace-desktop/products/subspace/assets/fonts"
src="$here/overview.src.svg"

find_chrome() {
  local candidate
  for candidate in "${CHROME:-}" \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "/Applications/Chromium.app/Contents/MacOS/Chromium" \
    google-chrome chromium chromium-browser; do
    [ -n "$candidate" ] || continue
    if command -v "$candidate" >/dev/null 2>&1; then
      command -v "$candidate"
      return 0
    fi
  done
  return 1
}

chrome=$(find_chrome) || { echo "build.sh: Chrome or Chromium not found; set CHROME" >&2; exit 1; }
command -v pdftocairo >/dev/null 2>&1 || { echo "build.sh: pdftocairo (Poppler) not found" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The light theme. The dark one is the source's own.
cat > "$work/light.css" <<'CSS'
:root:root {
  --bg: #f7f5f0; --card: #ffffff; --inset: #fbfaf7; --tile: #f3f0e9;
  --stack-1: #f1eee7; --stack-2: #ebe8e0;
  --border: #dedad0; --border-strong: #cdc8bc; --tree: #b9b4a8;
  --fg: #2f2d28; --bright: #14130f; --muted: #6b675e;
  --accent: #b5872b; --accent-text: #8a6516; --accent-bg: #fbf2dc; --chip-border: #dcc793;
  --human: #a9689f; --human-text: #8c4d83; --human-bg: #f8ecf6;
  --captain: #5578ab; --captain-text: #3f6296; --captain-bg: #eaf0f9;
  --crew: #74923a; --crew-text: #58751f; --crew-bg: #eff5e2;
  --warn: #c98a1e; --wire: #6b675e; --star: transparent;
  --logo: #111111; --gate-1: #14130f; --gate-2: #55524b; --gate-3: #8a877f;
}
CSS
: > "$work/dark.css"

for theme in dark light; do
  python3 - "$src" "$fonts" "$work/$theme.css" "$work/$theme.html" <<'PY'
import base64
import pathlib
import re
import sys

src, fonts, theme_css, out = map(pathlib.Path, sys.argv[1:5])
faces = [
    ("Inter", 400, "Inter-Regular.ttf"),
    ("Inter", 600, "Inter-SemiBold.ttf"),
    ("Inter", 700, "Inter-Bold.ttf"),
    ("JetBrains Mono", 400, "JetBrainsMono-Regular.ttf"),
]
css = "".join(
    f'@font-face{{font-family:"{family}";font-weight:{weight};'
    f'src:url(data:font/ttf;base64,{base64.b64encode((fonts / name).read_bytes()).decode()})}}'
    for family, weight, name in faces
)
svg = src.read_text(encoding="utf-8")
# The PDF page is the size of the source's canvas.
size = re.search(r'<svg\b[^>]*?\bwidth="(\d+)"[^>]*?\bheight="(\d+)"', svg)
if not size:
    sys.exit("build.sh: the source has no width and height")
width, height = size.groups()
out.write_text(
    "<!doctype html><meta charset='utf-8'><style>"
    + css
    + f"@page{{size:{width}px {height}px;margin:0}}html,body{{margin:0}}svg{{display:block}}"
    + "*{-webkit-print-color-adjust:exact;print-color-adjust:exact}"
    + "</style>" + svg + "<style>" + theme_css.read_text(encoding="utf-8") + "</style>",
    encoding="utf-8",
)
PY
  "$chrome" --headless=new --disable-gpu --no-pdf-header-footer \
    --virtual-time-budget=5000 --print-to-pdf="$work/$theme.pdf" \
    "file://$work/$theme.html" >/dev/null 2>&1
  pdftocairo -svg -f 1 -l 1 "$work/$theme.pdf" "$work/$theme.svg"
  # Restore the accessible name the PDF round trip drops, and end with one
  # newline.
  python3 - "$work/$theme.svg" "$here/overview-$theme.svg" <<'PY'
import pathlib
import re
import sys

built, out = map(pathlib.Path, sys.argv[1:3])
svg = built.read_text(encoding="utf-8")
if "<image" in svg:
    sys.exit("build.sh: the build rasterised part of the diagram")
label = (
    '<title id="ov-title">How Subspace fits together</title>'
    '<desc id="ov-desc">Your team works in the Subspace desktop app, sharing IRC '
    "channels, an issue tracker, decisions and the Fleet Console with fleets of "
    "coding agents. Each fleet is one VM run by Stargate, with a rear admiral, "
    "captains and their crews. Stakeholders reach the same agents from Slack or "
    "Microsoft Teams. Subspace and the fleets sit inside a private network, and "
    "the agents' work reaches GitHub as pull requests.</desc>"
)
svg, count = re.subn(
    r"(<svg\b[^>]*)>",
    lambda m: m.group(1) + ' role="img" aria-labelledby="ov-title ov-desc">\n' + label,
    svg,
    count=1,
)
if count != 1:
    sys.exit("build.sh: no <svg> element in the built file")
# pdftocairo writes six decimals; three keep the outlines exact at any size
# a README shows and make the file much smaller.
svg = re.sub(
    r'(?<= d=")[^"]+',
    lambda m: re.sub(
        r"-?\d+\.\d+",
        lambda n: f"{float(n.group()):.3f}".rstrip("0").rstrip(".") or "0",
        m.group(),
    ),
    svg,
)
out.write_text(svg.rstrip() + "\n", encoding="utf-8")
PY
  echo "built overview-$theme.svg"
done
