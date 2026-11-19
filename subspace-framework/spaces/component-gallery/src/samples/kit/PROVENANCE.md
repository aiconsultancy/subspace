# GPUI Kit sample provenance

The GPUI Kit cards use each component's public API from gpui-component 0.9.1.
Where a sample follows a composition from GPUI Kit's own story gallery
(`crates/story/src/stories/` at tag `v0.9.1` of
[longbridge/gpui-kit](https://github.com/longbridge/gpui-kit), Apache-2.0),
it is cut down to fixture data and adapted to Subspace's tokens, IDs and
shared boundaries (`anchored`, `toggle::switch`). The file names the story it
follows.

- `basic/messages.rs`: `message_story.rs` and `marker_story.rs`, modified.
- `advanced/dock.rs`: `dock_story.rs`, modified.

Other samples were written from the component source and its documentation
at <https://gpui-kit.com/component>.
