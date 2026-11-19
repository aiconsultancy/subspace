# Spec: IRC settings

## Settings > IRC

- The IRC board provides `Dashboard::settings_editor`. Its controls sit below
  **Enabled** on the board's page and share Settings' Save/Cancel draft. Save
  writes `networks` and `highlights` under `boardSettings.irc.settings`; other
  keys there, and keys the form does not show inside a network or its `sasl`
  object, are kept.
- **Networks** lists each network: its name, `host:port · TLS|Plaintext ·
  nick` and **Connects on request** when it does not connect on startup, a
  problem line when it is incomplete, **Edit…** and **Remove…**. **Remove…**
  asks inline: `Remove “<name>”?`, a note that Save disconnects it and removes
  its passwords, **Cancel** and a destructive **Remove**. **Add network**
  appends a network and opens its form.
- The form has the groups Server (Name, Host, Port, Use TLS; with TLS:
  Accept invalid certificates with a muted warning, CA certificate, Client
  certificate, Client key), Identity (Nickname, Alternative nicknames,
  Username, Real name), Sign-in (Server password, Account sign-in (SASL):
  None, Account and password (PLAIN), Client certificate (EXTERNAL); PLAIN:
  Account, Read the password from a file, then Account password or Password
  file; EXTERNAL: Account) and Channels (Channels to join, Connect on startup).
  Path fields have **Choose…**, which opens the native file picker. **Done**
  returns to the list; with a missing value it stays and shows the problems.
  A new network left blank is dropped on Done.
- Below the list: **Highlight words** (comma separated) and **Set up with a
  coding agent** with **Copy agent prompt** and the platform's reveal button.
- Validation: host and nickname required; port 1 to 65535; nicks start with a
  letter or `[]\`_^{|}` and continue with letters, digits, those characters or
  `-`; channels start with `#` or `&`, are longer than one character and have
  no space, comma or BEL; paths are absolute; PLAIN needs an account and a
  password or a password file; EXTERNAL needs TLS and a client certificate.
  Format problems show at once; missing values show after Done. Save refuses
  any problem and names the network and field.
- Ids: saved networks keep their id. A new network's id is the slug of its
  name (else its host), made unique with `-2`, `-3`… against every other id.
- Secrets: the server password and the SASL password use the shared secret
  row with password wording. A saved password is never copied into an input,
  is kept when untouched, is removed after **Clear saved password** with an
  empty input, and is replaced by typed text. Reading the password from a file
  removes a stored SASL password on Save.
- Applying: after Save, Settings mirrors the saved board settings into the
  `Services` config. The IRC hub observes `Services`, and when the parsed
  settings differ it sends `Command::Configure` (only when the networks
  differ) and reconfigures its state: kept networks keep their status and
  buffers, new ones appear and removed ones leave with their buffers. Board
  highlight words are matched by the board, not sent to the runtime, so
  changing them reconnects nothing.

## Show in the file manager

- Shared `reveal::button(id, path)`: neutral action, FolderOpen glyph, label
  **Show in Finder** (macOS), **Show in File Explorer** (Windows) or **Show in
  Folder** (Linux). It selects the file, or opens the nearest existing folder
  when the file is missing.
- Shown beside the configuration file path on Settings > General, on
  Settings > IRC and on the IRC empty state.

## Agent prompt

- **Copy agent prompt** (shared `copy_details::copy_text`, Copy glyph,
  **Agent prompt copied** in-app confirmation) copies text built on
  activation: what Subspace is, the absolute `config.json` path, the editing
  rules (valid JSON, change only `boardSettings.irc.settings`, keep other
  keys, keep `0600`, ask for passwords, never echo or commit them, close
  Settings first), the full schema with types, defaults and meanings, an
  example and how to verify.
- The schema comes from `spaces/irc/src/schema.rs`. A test keeps its keys
  equal to the serde keys of `NetworkConfig` and `Sasl`; another checks that
  the prompt names every key and the path.

## Empty board

- Title **No IRC networks**, one sentence, and **Add network…** (dispatches
  `ConfigureBoard`, opening Settings at the IRC page), **Copy agent prompt**
  and the reveal button, all neutral. A malformed saved network is still
  named in red. The JSON example is gone; the help guide and `docs/irc.md`
  carry the format.
