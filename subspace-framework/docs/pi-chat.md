# Pi chat engine

Pi is an experimental engine for local Subspace conversations. Enable
**Pi chat** in **Settings > Experiments**, then choose
**Settings > Chat > Chat engine > Pi (Experimental)**. Save and restart
Subspace. You can also set the choices in the product's `config.json`:

```json
{
  "chat": {
    "engine": "pi"
  },
  "features": {
    "app.pi-chat": true
  }
}
```

Choose `off` to turn Chat off. Each engine keeps its own saved conversations.

Pi is off by default. Turning its experiment off while Pi is selected changes
the draft engine to Off, with an explanation. Save and restart apply the
change. A hand-edited Pi engine choice without the opt-in cannot start Pi.
See [features and experiments](feature-flags.md).

## Setup

1. Install Pi using its [official setup instructions](https://pi.dev/).
2. Start `pi` yourself and use `/login` to configure the provider you want.
3. Enable the Pi chat experiment, select Pi in **Settings > Chat**, save and restart.
4. Choose an available model in the composer.

Subspace uses Pi's existing provider authentication. The GitHub account chosen for
Subspace's GitHub dashboard still controls its service operations; it does not
select the model provider's identity. No Pi API keys or OAuth tokens belong in
Subspace UI preferences, attached references or chat messages.

This adapter requires Pi `1.0.4`, the version used for the isolated protocol
check. Subspace rejects other versions rather than falling back to Copilot.

## Capabilities and limits

Pi uses the existing chat panel for streamed replies, exposed thinking, tool
activity, model selection and Stop. Thinking is shown only when Pi sends it.
Provider-reported usage is not a Copilot credit balance.

Each open chat runs its own Pi process. Subspace lists only sessions created by
this adapter, not your other Pi conversations. A renamed closed session receives
its saved name when reopened. The initial thinking selector does not offer
`xhigh` or `max`.

Pi receives the board tools of the product's enabled dashboards through a
bundled bridge. The existing Rust tools validate scope and request approval
before each service write batch. Denying a batch writes nothing. Stop cancels
pending work but cannot undo a request already sent to a service.

Pi does not provide the Copilot sandbox guarantees. Its native shell and file
tools remain disabled in Subspace. Automatic extensions, MCP servers, skills,
prompt templates and project instructions are not loaded. Only Subspace's
bundled bridge runs. Selecting Pi does not authorize host access or attach
Subspace's unconditional-approval MCP server.

The initial Pi integration does not offer Copilot custom-agent selection,
Copilot Plan/Autopilot modes, YOLO or cloud-agent replay. Select Copilot and
restart when you need those capabilities. Explicit attachments do not grant
access to their parent directories.

## Troubleshooting

- If Pi is missing, install it yourself and restart Subspace.
- If no models are available, configure Pi's provider authentication outside
  Subspace. Do not paste credentials into chat.
- If startup fails, check the Pi version and the supported RPC options.
- If a turn fails after it starts, inspect the error and tool results before
  retrying. A successful prompt acknowledgement is not a completed turn.

See [agent chat](agent-chat.md) for shared context and service-write behavior.
