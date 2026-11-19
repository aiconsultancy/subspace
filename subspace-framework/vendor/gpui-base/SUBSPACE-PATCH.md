# GPUI Kit 0.9.1 text patch

This directory contains the published `gpui-base` 0.9.1 crate with its
[Apache 2.0 licence](LICENSE-APACHE). The crate was published from
[GPUI Kit commit 39ab849a](https://github.com/longbridge/gpui-kit/tree/39ab849a93b5f15507b969164d49891db8d5b470/crates/base).
The SHA-256 of the original crate archive is
`f3e9ae5237737cfd110d0ff513fcd9c20f97d98ebabd0a73164068177f0aca4f`.
[The source patch](subspace.patch) applies to the unmodified archive.

- Backport `SelectableText::highlights`, `reveal_range` and `on_reveal`.
  Find in page can mark matches in literal selectable text and scroll to
  the current match's visual line. Reveal runs once per requested range.
- Use the shared upstream handler for linked images. Formatted links and
  linked images report secondary clicks through `TextView::on_link_click`.
  The local patch lets those clicks reach the same handler while a text
  selection is active.
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

- `src/selectable_text.rs` backports the highlight and reveal builders and their tests.
- `src/text/inline_flow.rs` and `src/text/node.rs` call the merged linked-image helper from the 0.9.1 render paths.
- `src/text/text_view.rs` holds the merged helper, uses the packaged README fixture and registers the link click tests.
- `src/text/inline.rs` and `src/text/inline_object.rs` let secondary clicks reach the existing callback with a selection.
- `src/text/link_click_tests.rs` is a local test file for formatted links and both linked-image layouts.
- `src/text_selection.rs` backports the open press latch fix and its regression test.

The vendored `Cargo.lock` comes from the published archive. The standalone
Base tests use it. The Framework, desktop and terminal builds use their
own lockfiles.

## Upstream status

| Change | Status |
| --- | --- |
| [#3929 Selectable text highlights](https://github.com/longbridge/gpui-kit/pull/3929) | Merged as [`1ca6555d`](https://github.com/longbridge/gpui-kit/commit/1ca6555d) |
| [#4052 Reveal the visual line of a selectable text range](https://github.com/longbridge/gpui-kit/pull/4052) | Merged as [`f8243df9`](https://github.com/longbridge/gpui-kit/commit/f8243df95b9158d9b6d842348dcfcd0481a85c2c) |
| [#4053 Share linked-image click handling](https://github.com/longbridge/gpui-kit/pull/4053) | Merged as [`85156303`](https://github.com/longbridge/gpui-kit/commit/85156303ae309925bdefa63250ca9f38b17c270b) |
| [#4054 Clear the selection press latch after mouse-down dispatch](https://github.com/longbridge/gpui-kit/pull/4054) | Open at [`a3296507`](https://github.com/longbridge/gpui-kit/commit/a3296507b57a7aadf95b6e42a811695a05241b0d) |

The Framework already uses the released callback signature and inspects
the button on the `ClickEvent`. To retire the vendor, bump the GPUI Kit
pin in the Framework, desktop and two terminal manifests, remove their
`gpui-base` overrides and refresh the lockfiles. The replacement release
needs the press latch fix and must deliver auxiliary link clicks with
an active selection. Run the dashboard regressions against the replacement
before dropping the vendor directory.

## Verify

From the Framework workspace:

```sh
mise exec -- cargo test -p subspace-dashboard --features gpui-kit-text-patch --locked
mise exec -- cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support --lib --locked
mise exec -- cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support --lib --locked -- reports_the_auxiliary_click a_press_without_a_release reveal_reports_the_wrapped_line reveal_runs_once_per_range
```
