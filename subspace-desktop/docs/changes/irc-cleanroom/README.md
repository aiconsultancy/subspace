# IRC protocol clean-room implementation

Reimplement the desktop IRC client's protocol layer from the written
functional specification and public standards. Preserve the existing
client and Channels Space interfaces. Record the implementation sources
in the [clean-room log](../../../clients/irc/CLEANROOM.md).

The implementation covers byte framing, protocol parsing and serialisation,
TCP and TLS, the per-network connection loop, and the session handlers for
registration, capability negotiation, SASL, ISUPPORT, batches and messages.

- [x] Restore the 15 protocol modules with no new dependencies.
- [x] Integrate the 67 supplied acceptance tests and fresh protocol tests.
- [x] Run the client tests with live Ergo on loopback.
- [x] Run the Channels Space tests with live Ergo, including the agent tools.
- [x] Verify TLS with a self-signed server, an extra trusted root and a
  client certificate for SASL EXTERNAL.
- [x] Exercise the idle PING and unanswered-PING reconnect with a loopback peer.
- [x] Confirm the desktop connects, joins, displays history, exchanges
  messages and typing, and reconnects after an Ergo restart.
- [x] Pass `make check` with native macOS process containment available.
- [x] Run `mise run quality` against the desktop baseline.

Run the client and Channels Space suites with `IRC_TEST_SERVER` set to the
loopback address from `scripts/irc-test-server.sh`. Run `make check` from the
desktop workspace for the full checks.
