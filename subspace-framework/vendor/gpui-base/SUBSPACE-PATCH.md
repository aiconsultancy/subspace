# GPUI Kit 0.11.0 text patch

This directory contains the published `gpui-base` 0.11.0 crate with its
[Apache 2.0 licence](LICENSE-APACHE). The crate was published from
[GPUI Kit commit e97fe438](https://github.com/longbridge/gpui-kit/tree/e97fe438129c5e4ccab504f2e6fafd9a4e396bba/crates/base).
The SHA-256 of the original crate archive is
`604d277dd339c63d13a3016d22a84622cc7511fdfd8e88573c0709088ede3500`.
[The source patch](subspace.patch) applies to the unmodified archive.

- Formatted links and linked images report secondary clicks through
  `TextView::on_link_click` while a text selection is active, so the text
  menu can offer **Copy link**.
- Adjust a test fixture to read the README from the packaged crate.

GPUI Kit 0.11.0 ships the other changes that the 0.9.1 patch backported:
selectable text highlights and reveal, the shared linked-image handler and
the press latch reset.

The Framework, desktop and both terminal manifests use `[patch.crates-io]`
to select this source. The desktop product enables the Framework's optional
`gpui-kit-text-patch` feature. Cargo applies patches from the root of the
build. The vendored directory stays in source checkouts.

## Modified files

The Subspace contributors changed these files from the published archive.
The complete source changes are in [subspace.patch](subspace.patch).

- `src/text/inline.rs` and `src/text/inline_object.rs` let secondary clicks
  reach the existing callback with a selection.
- `src/text/link_click_tests.rs` is a local test file for formatted links
  and both linked-image layouts.
- `src/text/text_view.rs` uses the packaged README fixture and registers the
  link click tests.

The vendored `Cargo.lock` comes from the published archive. The standalone
Base tests use it. The Framework, desktop and terminal builds use their
own lockfiles.

## Upstream status

| Change | Status |
| --- | --- |
| [#3929 Selectable text highlights](https://github.com/longbridge/gpui-kit/pull/3929) | Released in 0.11.0 |
| [#4052 Reveal the visual line of a selectable text range](https://github.com/longbridge/gpui-kit/pull/4052) | Released in 0.11.0 |
| [#4053 Share linked-image click handling](https://github.com/longbridge/gpui-kit/pull/4053) | Released in 0.11.0 |
| [#4054 Clear the selection press latch after mouse-down dispatch](https://github.com/longbridge/gpui-kit/pull/4054) | Released in 0.11.0 |
| Auxiliary link clicks with an active selection | Local only |

To retire the vendor, bump the GPUI Kit pin in the Framework, desktop and
two terminal manifests to a release that delivers auxiliary link clicks
with an active selection, remove their `gpui-base` overrides and refresh
the lockfiles. Run the dashboard regressions against it first.

## Verify

From the Framework workspace:

```sh
mise exec -- cargo test -p subspace-dashboard --features gpui-kit-text-patch --locked
mise exec -- cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support --lib --locked -- link_click
```
