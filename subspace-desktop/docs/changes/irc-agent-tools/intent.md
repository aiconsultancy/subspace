# Intent: IRC agent tools

The owner asked: "can we add agent / mcp tools for IRC?"

Agents in Subspace chat and MCP clients can read and act on the GitHub and
Copilot Agents boards, but not on IRC. The goal is that an agent can read what
the IRC board shows (networks, channels, recent messages, members) and, with
one explicit approval per call, send messages, join and leave channels and
connect or disconnect networks, as the user.

IRC differs from the other boards: its tools need a live connection. The
tools must use the board's existing hub, with the same connections, buffers
and nick the user sees, and must never open a second connection. A second
connection would take another nick or collide with the user's session.

Writes act as the user in public channels, so the same safeguards apply as
for the other write tools. Every operation is validated and checked against
the live state before the approval. One approval covers the call, and the
approval shows every line that will be sent. Operations run once, in order,
and nothing is retried. Message text from IRC is written by other people, and
the tools treat it as untrusted data.
