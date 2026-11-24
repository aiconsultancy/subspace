# Networks from the signed-in account

Back to the [change records](../README.md).

After a person signs in, Channels connects to each of their fleets' IRC
servers that Subspace API vouches for, each with a short-lived login issued
for that server alone, so nobody types IRC settings. The networks are derived from the account and its
fleets, run beside the saved networks, are never written to
`irc/config.json` and go on sign-out. A saved network on the same server is
kept, with an offer to switch it. **Settings > Channels** shows them
read-only.

- [Intent](intent.md): what was asked for and why.
- [Spec](spec.md): the behaviour, by crate.
- [Delivery](delivery.md): checks, screenshots and what is not verified.
- [Tasks](tasks.md).

The maintainers' description is
[Networks from your account](../../irc.md#networks-from-your-account).
