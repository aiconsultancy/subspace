# Decisions search and paging

## Intent

Find a decision by the reference shown on its row, search the whole log,
and keep the list steady while the API answers.

## Behaviour

- Full decision references match a single record. A short reference matches
  its number across projects. A shorter prefix matches the start of the ID.
  Ordinary text still searches the title and decision text.
- Previous and Next read up to 100 decisions at a time. The next page is
  available until the API's cursor runs out. Page changes keep the existing
  rows visible while reading.
- The toolbar marks a pending search with a spinner. The newest response
  replaces the visible rows. Previous results stay available after a
  temporary failure and are labelled as such. An account change clears them.

## Delivery

The desktop uses the same decision list endpoint and cursors. The short-ID
search behaviour requires the updated Subspace API. The desktop does not
write any search or paging data to preferences.

The desktop and API are verified with fictional records. The API integration
fixture traverses 5,004 decisions on an isolated PostgreSQL database. The
native evidence shows a large list, the next page, search in progress and a
single ID match.

- [Before paging](evidence/before.png)
- [First page](evidence/first.png) and [second page](evidence/second.png)
- [Search in progress](evidence/searching.png) and [short-ID match](evidence/match.png)
