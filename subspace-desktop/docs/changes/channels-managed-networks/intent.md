# Intent: networks from the signed-in account

Back to the [record](README.md).

A person who installs the desktop and signs in should be able to use the
tracker and IRC straight away: nobody runs a CLI, registers an IRC account
or sends a password. The API is the only identity authority. It allocates
each person a username, which is also their IRC account name, and issues a
short-lived IRC credential that the fleet's Ergo server checks with the API
through its auth-script hook.

Until now the desktop read each fleet's IRC host, port and TLS and switched
autojoin when the initiative changed, but would not create a connection,
because IRC needed that server's SASL account, and passwords typed in
Settings were kept in `irc/config.json`.

The owner asked the desktop to:

- keep one network for each distinct IRC server among the signed-in
  person's fleets, with the nick and SASL account from the API;
- fetch the password from `POST /api/v1/me/irc-credential` at every connect
  and reconnect, keep it in memory only, and send the SASL username as
  `account@device` with a stable, random per-install device ID;
- treat these networks as derived state: rebuilt as sign-in, fleets and
  initiatives change, never left behind in the saved list, gone on sign-out;
- add no second connection when a saved network already serves the server,
  and offer to switch it;
- degrade cleanly: without the API feature nothing changes, an unreachable
  API is a clear retrying state and never a password prompt, and a
  credential that expires mid-session does not disconnect;
- show the networks read-only in **Settings > Channels**, saying where they
  come from, for every identity provider the build can select.

The API side (`username` on `GET /me`, the credential route and Ergo's hook)
is built separately to a fixed contract; this change consumes that
contract and is tested against fixtures.
