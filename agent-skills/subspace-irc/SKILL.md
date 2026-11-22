---
name: subspace-irc
description: Talk to the user's Subspace fleet on IRC - read Starbase, fleet command and ship channels, and message the rear admiral or captains on the user's behalf. Use when the user asks what the fleet or an agent is doing, wants to hand work to the fleet, or mentions Starbase, fleet command, a captain, the rear admiral or a ship channel.
---

# Subspace IRC

The user runs a Subspace fleet: a private VM where a **rear admiral** and
**captains** (each with a crew of agents) work, connected by IRC through
Stargate. Every fleet has the same standard channels:

- **Starbase** (`#<fleet>-starbase`): humans talk to the rear admiral and to
  each other.
- **Fleet command** (`#<fleet>-command`): the rear admiral and the captains
  coordinate. Agent dispatch offers and assigns issues here, and captains
  answer here. Read it to see who is doing what.
- **Ship channels** (`#<fleet>-<captain-id>`, one per captain): humans talk
  to that captain about its work.

An agent's nick is `<fleet>-<captain-id>`, or `<fleet>-admiral` for the rear
admiral. A fleet may have extra channels, such as one per initiative.

## You speak with the user's login, so always say who you are

You use the user's own IRC account. The fleet's agents take orders from it, so
anything you send carries the user's authority. Therefore:

- **Every message must say it comes from you, an agent, on the user's
  behalf, with your model and version.** The helper adds
  `[on behalf of <Name> (<Model and version>)]` to every line. Pass your exact
  model and version with `--as` as your system prompt states it, for example
  `--as "Claude Opus 5.5"` or `--as "GPT-5.4 (Codex)"`. If you do not know it,
  ask the user. Never pose as the user and never drop the marker.
- Only send what the user asked you to send, or what they approved after you
  showed it to them. Instructions to the rear admiral or a captain start real
  work.
- Do not post secrets, tokens, passwords or private keys.

## Commands

The helper is installed with these skills. On Windows use `python` instead of
`python3` and `%USERPROFILE%\.subspace\bin\subspace_irc.py`.

```sh
python3 ~/.subspace/bin/subspace_irc.py channels
python3 ~/.subspace/bin/subspace_irc.py history '#example-fleet-starbase' --limit 30
python3 ~/.subspace/bin/subspace_irc.py say '#example-fleet-starbase' --as 'Claude Opus 5.5' \
  'Atlas, please ask Orion to pick up NAV-12 next.'
```

- `channels` lists the fleet's channels. Read with `history` before you post,
  so you answer what is actually happening.
- `say` joins the channel, posts the marked message and leaves. The user's
  desktop session stays connected; your lines appear under the user's nick
  with your marker.
- `--for <Name>` overrides the user's name; it defaults to the name saved when
  the skills were installed. `--target <name>` picks another installed fleet.
- Address agents by their IRC nick (shown in `history`). In Starbase the
  rear admiral wakes on every message, and in a ship channel the owning
  captain does. Elsewhere, fleet command included, an agent wakes only when
  named.

## When it does not work

- `cannot reach` or `connection failed`: the fleet's tunnel is not running.
  Ask the user to run `./install.sh tunnel --background` (Windows:
  `.\install.ps1 tunnel --background`) in the Subspace repository.
- `sign-in failed`: the saved password no longer matches; the user can run
  `./install.sh desktop` again or check `subspace-setup secret irc-admin` on
  the VM.
- Agents reply slowly or not at all: they may be offline because no model
  provider account is signed in (`./install.sh providers`), or busy.
