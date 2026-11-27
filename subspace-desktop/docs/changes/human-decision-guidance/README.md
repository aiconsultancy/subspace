# Human decision guidance

## Intent

Give fleet operators enough information to make a decision from a chat
question and a stable reference to open its full record.

## Behaviour

The shared Subspace tracker skill tells officers to record proposals in
Subspace and lead with a short summary, recommendation and action that
approval permits. Each chat question includes its decision reference.
Authenticated human fleet operators and admirals can approve the proposal
shown. Officers retain the original chat evidence and the approved version.

## Delivery

- [x] Update the shared tracker guidance for OpenCode, Claude Code and Codex.
- [x] Run the seven existing skill and MCP compatibility tests against the
  shared instructions.
- [x] Regenerate the installer fixture and pass its 103 policy and 47 session tests.
- [x] Check the documentation diff with `git diff --check`.
