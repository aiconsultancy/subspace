# IRC history and full-text search: intent

Back to the [change records](../README.md).

The owner asked (verbatim): "I think that we should be able to free text
search across all the messages in the channels that have been pulled from the
server. There should also be caching so we don't want to be reloading loads of
data every time; if we have historical messages they should also just be
persisted and loaded. There's a data caching facility."

Decisions: build both search and history now; one local IRC search scope with
the network as a filter, without a search-service change for several scopes.

Then the direction changed (verbatim): "IRC should be using SQLite for some
sort of persistent storage. And this should be free text indexed. And I'm not
sure how that gets exposed to global search, but it should be. I think
actually by default boards should store their data in a SQLite that is free
text indexed. That should just be the pattern and the global search should be
able to hook into that through an API that every board exposes. There
probably should already be an API that exists so let's try not to reinvent
the wheel."

So IRC keeps its messages only in the Framework's existing SQLite FTS store
(the search service), reads them back through a new generic board storage API
in that service, and global search finds them there. No separate history
file. The pattern is written down for every board in
[What the Framework gives a board](../../board-integration.md#data-caching-and-refresh).

The [spec](spec.md) holds the design note and behaviour, [tasks](tasks.md) the
work and the board checklist, and [delivery](delivery.md) the checks and
screenshots.
