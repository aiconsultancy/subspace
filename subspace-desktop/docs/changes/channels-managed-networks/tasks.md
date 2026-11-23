# Tasks: networks from the signed-in account

Back to the [record](README.md).

- [x] API client: `Me::username`, `Client::irc_credential` with offline
      fixture tests for success, 404, 401, 5xx, an unreachable API, invalid
      answers, redaction and NETT.
- [x] IRC client: `CredentialSource` beside the password and password file,
      asked at every connect on a blocking thread, retried on failure, never
      serialised; tests for each connect, retries, precedence and PLAIN with
      a device.
- [x] Channels: account check with backoff, server and network derivation,
      the credential source, the device ID, the hub's saved and effective
      settings; unit tests for the pure parts and hub tests for sign-in,
      sign-out, initiative autojoin, saved-network clashes and saves.
- [x] Settings › Channels: the read-only section, the switch offer and its
      confirmation; headless render tests.
- [x] Fixture: `username` on `/me` and `--irc-credential`
      (`CAPTURE_IRC_CREDENTIAL=1`), with fixture and capture-script tests.
- [x] Docs: `docs/irc.md`, the Channels help guide, `docs/settings.md`,
      `docs/capture.md`.
- [x] Evidence from the capture build in Twilight Two, before and after.
- [x] Review round one (pull request 83): bind each credential to its
      server, plan only from the account's own memberships, reserve account
      network ids and scope their history, keep the issued password out of
      the configuration and require its SASL, keep logins until shortly
      before they expire with spacing and backoff, validate endpoints and
      check the peer before a plaintext login, retry refused checks and
      unread memberships, tighten the Settings switch, and test the timer
      path; screenshots recaptured.
- [x] Review round two: merge `main`; keep and hide rather than delete
      other people's history, restoring it in-session; compare hosts in one
      form and validate only for logins; reserve only `@account.`; check a
      staged switch again at Save; harden the credential path; record the
      known limits.
- [ ] End-to-end sign-in against a real API with the credential route and an
      Ergo server with the auth hook, both now on `main`.
