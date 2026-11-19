# GPUI Kit 0.9.1 text patch

This directory contains the published `gpui-base` 0.9.1 crate with its
[Apache 2.0 licence](LICENSE-APACHE). The crate was published from
[GPUI Kit commit 39ab849a](https://github.com/longbridge/gpui-kit/tree/39ab849a93b5f15507b969164d49891db8d5b470/crates/base).
The SHA-256 of the original crate archive is
`f3e9ae5237737cfd110d0ff513fcd9c20f97d98ebabd0a73164068177f0aca4f`.
[The source patch](subspace.patch) applies to the unmodified archive.

- Backport `SelectableText::highlights` and add `reveal_range` and
  `on_reveal`. Find in page can mark the matches in literal selectable text
  and scroll to the current match's visual line.
- Add `TextView::on_link_context_menu`. Applications receive the link URL
  on secondary press, including over selected text and linked images, and
  can use it in the enclosing text menu.
- Reset the selection press latch at the end of mouse-down dispatch.
  Selection can clear on the next press when a release was missed.
- Adjust a test fixture to read the README from the packaged crate.

The Framework, desktop and both terminal manifests use `[patch.crates-io]`
to select this source. The desktop product enables the Framework's optional `gpui-kit-text-patch`
feature. It lets dashboard code call the extra APIs. Cargo applies patches
from the root of the build. The vendored directory stays in source
checkouts, and the Framework's default feature set uses APIs from registry
GPUI Kit 0.9.1.

## Modified files

The Subspace contributors changed these files from the published archive.
The complete source changes are in [subspace.patch](subspace.patch).

- `src/selectable_text.rs`
- `src/text/inline.rs`
- `src/text/inline_flow.rs`
- `src/text/inline_object.rs`
- `src/text/node.rs`
- `src/text/state.rs`
- `src/text/text_view.rs`
- `src/text_selection.rs`

The patch also adds `src/text/link_context_tests.rs`. The vendored
`Cargo.lock` comes from the published archive. The standalone Base tests
below use that lockfile, while the Framework, desktop and terminal builds
use their own lockfiles.

## Pending upstream pull requests

GPUI Kit main already includes the highlight builder. Three pull requests
are pending for the remaining changes:

- Reveal the visual line of a selectable text range.
- Report secondary presses on text links.
- Clear the selection press latch after mouse-down dispatch.

Once all fixes ship together, upgrade the matching GPUI Kit crates, switch
the Framework code to the released APIs, remove the feature and path
patches, and refresh the affected lockfiles.

## Verify

From the Framework workspace:

```sh
mise exec -- cargo test -p subspace-dashboard --features gpui-kit-text-patch --locked
mise exec -- cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support --lib --locked context_event
mise exec -- cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support --lib --locked a_press_without_a_release
mise exec -- cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support --lib --locked reveal_reports_the_wrapped_line
```
