# Desktop defect repairs: specification

## Title-bar mark

Resolve product artwork on every render as before. Render themed marks with
GPUI's `svg()` element at the existing 14 px slot size. Tint its alpha mask
with the exact baked artwork tones: dark `#F0EFED`, light `#111111`. Keep
`img()` for compatibility `product.svg` so products can retain multicolour
artwork. `product_logo` composes path resolution and `logo_element`; the latter
owns rendering selection.

The pinned GPUI source's `elements/img.rs` calls
`render_single_frame(&bytes, 1.0)` for an SVG. Its 512 px intrinsic raster is
then shrunk into the slot. `svg()` uses window-scale-aware `paint_svg` and
rasterises the alpha mask at the destination's device-pixel size. At 1x,
this avoids reducing a 512 px raster to 14 px (about 36:1), preventing thin
strokes from breaking up through undersampling. At 2x it draws at 28 px.
The existing geometry, theme selection, fallback and interaction stay intact.
This explanation of the 1x improvement is based on the rendering path;
capture builds render at 2x and cannot establish a visual 1x result.

The existing `shell::toolbar` gallery card and catalogue own this surface.
Brand artwork tones are an explicit asset-colour exception to theme tokens.
Use GPUI primitives; no custom drawing or new control is needed.

## Actions account

Read the board's saved `actions.account`, falling back to the shared
`subspace_core::credentials::GITHUB_ACCOUNT_KEY` (`github.activeAccount`).
Pass that preference to the existing `GhAccounts::pick`, retaining its fallback
when neither key is saved. A board choice continues to override Settings on
later starts. Credential-source priority and Settings' Save/restart boundary
stay as documented.

The only other `actions.account` access is the write in
`spaces/actions/src/lib.rs::switch_account`. No other Actions read bypasses
the shared fallback. The new context test uses in-memory preferences and
fictional logins; no service calls are needed.

## Lanes tests

Move `spaces/issues/src/tests/lanes.rs` byte-for-byte to
`spaces/issues/src/view/lanes/tests.rs` and use conventional `mod tests`.
Keep all five tests and their module-qualified names. No cargo-shear ignore
metadata or application behaviour changes.

## ImageMagick

Pin `conda:imagemagick` 7.1.2_31 in `mise.toml`. On 16 November 2026,
`mise registry imagemagick` selected that backend and `mise ls-remote
conda:imagemagick` listed this as the newest available version. It provides
ImageMagick 7's `magick` command and avoids a global installation or a
separate conda setup. See the [backend documentation](https://mise.jdx.dev/dev-tools/backends/conda.html).

Both icon make targets run through `mise exec` so non-interactive shells
resolve the project tool. Document `mise install`, verification and macOS
generation prerequisites in the README build reference and developer tools.

## Copilot Agents account

`Client::with_work_account` supplies the shared GitHub account to the clients
owned by the Copilot Agents and PRs boards. `pick_work_login` first matches
that preference against configured gh logins, ignoring ASCII case and
retaining gh's spelling. An absent or unknown choice falls back to the first
EMU login with a nonempty alphanumeric `_shortcode`; no match returns `None`.
The error now explains that either an EMU or chosen account is sufficient.

The credential's viewer must still exactly equal the selected login.
`work_login` integrates account discovery, selection, viewer lookup and
verification; the pure selection and verification helpers own their logic.
Credential-source priority, private token handling and the Settings restart
boundary remain unchanged.

Known gap, outside this request: agent-tool clients constructed from
`ToolEnvironment` have no preferences and continue to use EMU detection only.
Passing the chosen account into that environment is a separate follow-up;
no `spaces/*/src/agent_tools.rs` implementation changes are included here.

Back to [intent](intent.md); see [delivery](delivery.md) and [tasks](tasks.md).
