# Delivery

Builds against the merged Activity data client and reuses the read-only
issue detail page from the issue workspace implementation slice 3. No deployment or NETT API change is
part of this client patch.

The macOS ARM64 capture build uses a temporary home, Cupertino Dark and
the repository's loopback NETT and WorkOS fixtures. All screenshot data is
fictional.

| Capture | What it shows |
| --- | --- |
| Before | The Issues list before switching to Activity. |
| Activity | Trackers above the recent feed with change, actor and age. |
| Tracker | A tracker opened in the shared issue view. |
| Issue | A recently updated issue opened in the same view. |
| Return | Back restores the active Activity tab and selected row. |
| [Loading](evidence/loading.png) | The two activity sections loading independently. |
| [Empty](evidence/empty.png) | The scoped feed has neither a tracker nor recent changes. |
| Retry | Only the failed recent section presents Retry. |
| Continuation | Load more on the recently updated section. |
| Narrow window | Long activity and state labels truncate at 650 points. |

Live verification remains dependent on the Activity read rollout on NETT dev.
