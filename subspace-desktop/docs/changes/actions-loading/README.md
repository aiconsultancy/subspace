# Actions run loading and empty states

The Runs list shows which repositories are still loading and which have no
runs matching the current filters. When all repositories have loaded and
there are no matches, the space names the filters and offers **Clear filters**.
This applies to the latest runs fetched by Subspace.

Run loading tests exercise repositories settling in sequence and a failed
repository settling without holding the remaining list open. Shared dashboard
and gallery tests cover the loading and empty rows.

Captured on macOS with the loopback GitHub fixture and a five-second runs
delay. The same fixture delay was added to the baseline capture script.

| Before | Loading | Partly loaded | No matches |
| --- | --- | --- | --- |
| [Baseline](evidence/before-loading.png) | [Loading](evidence/loading.png) | [Partly loaded](evidence/partial.png) | [Empty](evidence/empty.png) |
