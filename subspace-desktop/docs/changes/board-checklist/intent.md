# Board integration checklist

The owner asked for a checklist to run through before creating or merging a
pull request, and for a checklist of the features a board implements. The
Framework supplies hooks a board can use: registering content for full-text
search, agent and MCP tools, notifications and more. The owner wants every
hook and every piece of data and tooling the Framework gives a board listed
and explained, so that each new board implements or at least considers them,
and so the owner can understand them.

The IRC board was the example: it has no search integration, although people
should be able to search the messages fetched from the server, and its
history should be cached and loaded rather than fetched again on each launch.

After approving the pages, the owner asked that board-to-board communication
not be left out: "There should be a clearly defined method for boards to
communicate. … a board should be able to declare the events it publishes and
expose hooks and with that two boards via the subspace framework using
effectively an observer pattern should be able to receive notifications and
route them into the necessary boards that have subscribed to them and call
those hooks." The pages should say what exists and, if no declared mechanism
exists, propose one.
