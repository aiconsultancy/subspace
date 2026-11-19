# Full gallery visual review

19 November 2026. Owner expansion after slice 1: “Build them all and let me
visually check all.” The owner visually accepted the initial six views despite
strict 0/6. Exact pixel equality is no longer the done criterion. The original
strict comparator, fixed fixtures and failure reports remain unchanged.

## Scope and mapping

All 180 existing native catalogue cards have real accessible React DOM samples
in both approved themes. The inventory has 16 categories: 81 shared Subspace,
15 informational shell and 84 GPUI Kit cards. No native card, WASM, raster
substitution or generic placeholder was added. Original query aliases remain.

The shell cards are composition references in the native source, not full host
screens. Speech remains unsupported. NativeMenu has a labelled browser
equivalent, not an OS menu claim. See the [package guide](../../../web-spaces/examples/component-gallery/README.md)
for installation, interactions, capture commands and platform limits.

## Checks and evidence

- Production TypeScript/Vite build passes without warnings. All 45 browser/package/diff tests
  pass, including every exact card in both themes and original six fixtures.
- Search, category, theme and Previous/Next navigation report real DOM coverage
  separately from native image coverage. No dependency or font was replaced.
- Permission-free installable `dist/` validates: 409 files, 41,131,982 bytes,
  with local fonts, split sample modules and all 360 current native reference
  PNGs. No CDN is needed.
- Full React captures: 360 unique views in 20-view batches. PNG bytes are not
  modified. Most are 1600 × 1200. Matched list pairs are 1600 × 1318/1316 by
  theme. Matched chart pairs use a 1088-logical-pixel full-frame viewport:
  1600 × 2176 in both themes. All 360 pairs have matching width, height and DPR.

## Focused review fixes

The independent review found three interaction defects. Control-module popup,
textarea and note classes now have their own names, so visiting another lazy
module does not change their geometry. Clear returns focus to its text or date
input. PopupMenu returns focus to the actual opener, including the context
target, rather than always to Actions.

Four scoped regressions failed before the fixes and pass after them in both
themes. They cover cross-module navigation in both orders, text/date clear,
ContextMenu, Shift+F10, right-click and the dropdown opener. The warning-free
production build and package check pass. These changes preserve standalone
fixture styles and initial states; no native or React capture was replaced.
The original six strict metrics and full-gallery capture provenance remain
unchanged. Full-gallery visual acceptance still belongs to the owner.

## Native capture and provenance

Original committed references came from a prior host. The separate native-only
preview has fresh Bowser AMD captures; those are not comparison inputs. New
full-catalogue native support changes only the ignored Linux offscreen test and
its README: exact item/slug batches of at most 12 cards and bounded natural
heights, without production card changes. New visual fixtures are separate
from the original fixed six cases. Native/web pairs require matched dimensions.

The first guarded native compilation stopped on host-global memory PSI 1.26%
with at least 16.65 GiB available. No compiler failure or OOM was established.
Logs/cache remain. After diagnosis and approval, transient task-only user scopes
enforced a 10 GiB kernel memory cap with an outside guardian and a 12 GiB
admission floor. Pinned tools, one build job/test thread, 100 GiB disk floor and
80 GiB cache budget remained. No service-wide resource policy was changed.

The contained compile passed. Fifteen successful batches rendered all 360
actual native views; capture/GPU process and thread membership stayed under each
cap. One shell timeout interrupted a batch; its partial evidence is retained
but unpublished, and a fresh attempt completed it. Four initial viewport
mismatches were resolved by actual new native/web captures, not transforms.
A required test-only rebuild took 2.69 seconds. Five selection/height unit tests
pass. No cap, task OOM or host OOM event occurred in completed stages. Original
fixed six references and strict comparison metrics remain unchanged.

## Preview and delivery state

The existing tailnet route serves only explicit static files with byte digests
and safe path opens. Source, reports, private manifests, dotfiles and symlinks
are excluded. Original six native/comparison URLs remain unchanged. No Funnel,
ACL, public port, provider credential, Caddy or unrelated service was changed.

The separate feature branch starts at local `e5f0329`. Mario's checkout, build
and hooks are untouched. Source changes are not committed or pushed. Repository-wide
checks, normal delivery hooks, owner visual review and merge readiness remain
outstanding. This working-tree preview is not a release.
