# IRC issue links and Issues search

An issue mentioned in an IRC message should open in the Issues board. People should be able to copy that reference with the context menu and get directly to the record by typing its identifier in Issues.

## Behaviour

- A primary click on an IRC issue reference asks the current Issues board to open it, even while its preview loads. The click carries the active profile and organisation. Both boards check scope before opening the record.
- A secondary click opens the text menu. Copy uses the current selection or the displayed link text. Links to other sites use the shared browser link policy.
- An exact issue identifier takes precedence over text mentions within the current list scope. A query for an identifier continues through result pages to look for that issue. Ordinary text searches use the existing paging.
- Groups with matches open as their results arrive. Folds made while searching are temporary. Clearing the search restores the saved list folds.

## Board checklist

- **Layout and keyboard.** Reuse the selectable transcript, native menu and Issues list. The existing transcript reference keys and list keys continue to work.
- **Contracts and links.** IRC routes to the current scope responder through the existing contract. The Issues board checks the destination against the live scope. Other URLs use the shared link policy.
- **Data and search.** Searches use the selected tab, team and filters. Result pages remain in memory. The board stops automatically continuing a group if a page adds no rows.
- **Integration.** This fixes existing interactions and uses the current registrations, preferences, notifications, agent tools, settings and stored data formats.
- **Testing and evidence.** Headless tests exercise pointer activation, the clipboard, unresolved and withdrawn scope, exact search, continuation and group expansion. Captures use the macOS app with fictional IRC messages and NETT issues.

## macOS evidence

IRC issue reference before the context menu fix.

Right-click on an IRC issue reference opens the text menu.

The Issues board shows the record opened from IRC.

Exact ID before the fix shows related mentions and hides the exact issue in Done.

Exact ID after the fix shows a single visible result.

Text search before the fix leaves a matching Done group folded.

Text search after the fix opens both matching groups.
