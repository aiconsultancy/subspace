# Tracker parent picker in the Component gallery

## Intent

Make the Issues Parent picker’s search and paging available to try in the
Component gallery.

## Spec and delivery

In **Menus**, the `menu::MenuAnchors` card includes a tracker parent picker
alongside its basic filterable menu. Find fictional trackers by key or title.
**Load more trackers** keeps the menu open and appends another page. Changing
the query starts from the first page. Picking a tracker closes the menu and
updates the trigger. **No parent** clears the choice. The sample uses the same
shared menu and trigger as Issues with a local fixture list.

## Tasks and evidence

- Check the search, pagination, selection and clear path in a headless GPUI
  window.
- Compare the native card in Twilight Two before and after adding the sample.
  [Before](evidence/before.png) shows the basic sample.
  [After](evidence/after.png) shows the parent picker after searching.
  [More results](evidence/paged.png) shows the next page in the same menu.
  [Selected](evidence/selected.png) shows the trigger after picking a tracker.
