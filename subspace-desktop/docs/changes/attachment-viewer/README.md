# Text and Markdown attachments in Issues

Open a text attachment from the issue body or Attachments. The panel renders
Markdown and makes plain text selectable. Files over 1 MiB show the precise
byte limit. Empty files show their own state without requesting file bytes.

The viewer shares NETT's authenticated private grant flow with issue images.
The byte request carries no session credentials and requires the declared
content type, exact byte count and `nosniff`. The panel discards replies
from older issue revisions and from attachments that have been replaced
or closed.

Focused client and board tests exercise the response checks, reload during a
read, and a late result arriving after a different attachment opens. Captures
use the fictional NETT `text-attachments` scenario on macOS.

| Before | Markdown | Plain text | Empty file |
| --- | --- | --- | --- |
| File chips | Rendered document | Selectable notes | Empty viewer |
