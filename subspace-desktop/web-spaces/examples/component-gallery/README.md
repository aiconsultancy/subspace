# Component gallery web Space

Linux `mise run component-gallery-space-test` requires `dbus-run-session` and
runs only the browser-test command in a private D-Bus session. This prevents
Chromium from moving its test processes into uncapped desktop application
scopes. Other platforms run the same test command directly. Browser arguments,
sandboxing, GPU use and rendering are unchanged. The private session does not
provide the host desktop's portals or keyring services: these are web DOM tests,
not installed web-view IPC tests. Git, native checks and the outside resource
guardian keep the host session.

Keyboard focus stays in the current fixture: Clear returns to the associated
text or date input. Escape closes a popup menu and returns to the control that
opened it, including the context-menu target. Visiting another category does
not change an earlier fixture's popup, textarea or note styles.

The full gallery renders all 180 native catalogue cards in React 19 and
TypeScript, in One Dark and Cupertino Dark: 360 card/theme views. The cards
use real DOM controls, not screenshots or WASM. Exact pixel parity is not the
acceptance criterion; the original strict comparator remains unchanged.

| Query card | Native catalogue identifier |
| --- | --- |
| `action-button` | `action::action_button` |
| `input` | `fields::input` |
| `error-box` | `feedback::error_box` |

The static inventory has 81 Subspace, 15 shell and 84 GPUI Kit cards in 16
categories. Every canonical name has a stable query ID; the original three
aliases above remain unchanged.

Themes are `one-dark` and `cupertino-dark`. The original six native references
are unchanged committed prior-host PNGs, not fresh Bowser AMD captures. New
full-gallery visual-review references now cover all 360 views, captured by
native GPUI on Bowser AMD. The separate original native-only preview retains
its six fresh Bowser AMD images; those are not the historical comparison inputs. Native
images are labelled separately from the interactive React DOM. Fixture
actions have no external effects, as in the native gallery. Inputs retain
edits until the card changes. The mask control reveals a fake secret. The
copy button reports clipboard failures rather than claiming success.

## Build and check

```sh
npm ci
npx playwright install chromium --only-shell
npm run build
npm test
python3 ../../../scripts/package-web-space.py dist --check
```

The committed `dist/` is the installable package. It contains external JS/CSS,
local fonts and the permission-free `subspace-board.json`. No CDN, WASM,
server-side runtime or host CSP change is required. Rebuild after source or
reference changes. Font provenance and upstream licences are in
`public/fonts/`. The shadcn/ui attribution is in `public/licences/`.
`npm run build` generates [THIRD_PARTY_NOTICES](dist/THIRD_PARTY_NOTICES)
from the installed JavaScript and Tailwind CSS packages.

Install `dist/` through **Settings > Application > Web Spaces > Add board… >
Local folder**, then **Validate source** and **Install now**. Folder installs
are snapshots. Rebuild and use **Replace source…** for later changes.

## Browser comparison

```sh
npm run dev
```

Normal mode provides Theme, Component and Category selectors, multiword search,
and Previous/Next review controls. The sidebar shows every matching card.
Native/React panes scroll independently and stack on narrow windows. React and
native-image coverage are reported separately; missing images are explicit.
Changing cards resets fixture state. Fixture controls never start real sign-in,
service calls or native host operations.

## Full visual-review fixtures

The original capture/comparator commands below keep the fixed six-case lane.
Full-catalogue captures are separate, versioned visual-review fixtures:

```sh
node scripts/capture-all.mjs --start 0 --limit 20 --output artifacts/full-000
```

Repeat with starts 20, 40, …, 340 and fresh output directories for 360 views.
Batches contain at most 30 views. Captures use approved fonts, 800 logical
pixels width and DPR 2. There is no input resize, crop or mask. Full-page
height can grow for tall cards; metadata uses encoded PNG dimensions. Current
matched native/web chart cards are 1600 × 2176 in both themes. The shared list
card is 1600 × 1318 (One Dark) and 1600 × 1316 (Cupertino Dark); other views are
1600 × 1200. Use `--height` to set a larger full-frame web viewport when a
native card needs it; the capture script never transforms image inputs.
All 360 pairs have matching width, height and DPR. These are new visual-review
fixtures, not replacements for original fixed-height parity fixtures.

Tests render all 180 exact names in both themes and exercise search, category,
keyboard navigation and representative controls. No new dependency or font
substitution was added. The original six reference PNGs stay unchanged.

Platform limits: OS-native menus use a labelled DOM equivalent; speech remains
unsupported as in the native build; shell cards describe real composition but
cannot open the native host. Window dragging is not implemented. Rust editors
use editable plain text. Markdown preview covers fixture syntax, not a full
CommonMark engine. Dock retains two fixture groups; grid supports swaps and
resizing, not arbitrary native placement. Date/time/file pickers retain browser
presentation. These limits are not pixel-parity claims.

Capture mode shows only the selected DOM card on that viewport, without
navigation or a reference image:

```text
?theme=one-dark&card=action-button&capture=1
?theme=cupertino-dark&card=input&capture=1
?theme=one-dark&card=error-box&capture=1
```

Use a headless browser viewport of 800 × 600 with device scale factor 2. Wait
for `[data-testid="gallery-card"]` and `document.fonts.ready` before capture.
Unknown theme/card query values produce an explicit error, not a silently
substituted case. `npm run capture` and `npm run compare` invoke the separate
Linux screenshot/diff scripts in `scripts/`:

```sh
npx playwright install chromium --only-shell
npm run capture -- --output artifacts/web-run1
npm run compare -- --reference public/references --actual artifacts/web-run1 \
  --output artifacts/comparison-run1
```

Capture and comparison refuse existing output directories, to prevent stale evidence.
It also tests selectors, input edits, disabled controls, password visibility,
clipboard failure reporting and card-change reset in the built package.
`comparison-app.png` records the normal two-pane interface.
Comparison writes `report.json`, `index.html` and reference/React/diff/overlay
PNGs for every case. It exits 1 when any case fails or an input is missing.
Pixelmatch uses `threshold: 0`, `includeAA: false` and `maxDiffPixels: 0`.
The report also counts exact differences, including anti-aliased edges.

## Source snapshots and limitations

`src/themes/*.json` copies the selected canonical native theme variants from
`subspace-framework/host/app/themes/` at source baseline `42b07e3`. Fonts are
explicitly Inter and JetBrainsMono Nerd Font rather than Cupertino's system
font defaults. Build/runtime never reads the native folders.

Input fills use theme-specific opaque composites over the fixture's uniform
muted panel. They match the committed native reference interiors, not a
universal GPU blend rule. GPUI Component 0.9.1 mixes the input border with
transparent at 0.3 alpha; disabled inputs mix at 0.8 then apply 0.5 opacity.
Native GPU and Chromium rounding can differ by one RGB step. Re-measure these
tokens before changing the panel background or reusing them on another surface.
The browser regression checks normal and disabled interior pixels against the
unchanged native PNGs. Text, controls, borders and geometry remain real DOM.

Action plates use the same fixture-only composite rule. Outlined danger uses
the native 0.1 rest-state alpha, not 0.08, and neutral actions use the native
foreground token. GPUI ceilings shaped text widths before layout; the web
fixture uses real text spans with CSS `calc-size()`/`round()` for that sizing
and DPR2 frame snapping. Form controls explicitly request unhinted advances
because browser control styles otherwise override the root setting. The mask
glyph follows the native content inset while retaining the web hit target.
These measurements cover Chromium 159, not an installed host web-view claim.

Captures compare the complete 1600 × 1200 raster, without cropping, masking or
substituting reference images. Different browser/native text and SVG raster
rendering, control colours and border geometry may produce non-edge
differences. Those remain failures under zero non-edge tolerance. The gallery
does not hide them or claim macOS, installed web-view or full-catalogue pixel parity.
Hover, active and focus visuals and the mask toggle's keyboard behaviour
have not been compared with native. The web mask toggle is a keyboard stop;
the native fixture's toggle is not.
