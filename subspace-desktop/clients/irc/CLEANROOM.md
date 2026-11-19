# IRC client clean-room log

The earlier `subspace-client-irc` protocol layer included 15 files adapted
from Halloy (GPL-3.0-or-later). A clean-room implementation replaced them on
19 November 2026. The crate remains Apache-2.0.

- **Specification.** An AI agent that had read the earlier files wrote the
  functional specification, recorded the interfaces required by callers,
  listed the excluded material and wrote 67 acceptance tests with fresh
  inputs. The specification provides signatures and describes behaviour.
  The reviewer found no Halloy-derived code in its Rust blocks or in the
  acceptance tests.
  [Functional specification](../../docs/changes/irc-cleanroom/evidence/spec.md) ·
  [Caller inventory](../../docs/changes/irc-cleanroom/evidence/interface-callers.md) ·
  [Excluded material](../../docs/changes/irc-cleanroom/evidence/excluded.md)
- **Implementation.** A separate coding agent, OpenAI Codex, started from a
  single baseline commit in an isolated repository. The baseline omitted
  all 15 files. Codex used the supplied brief, the rest of the repository
  and the public references below, and reported encountering no excluded
  material. The integrated acceptance tests retain all 67 cases and their
  inputs and assertions.
  [Initial brief](../../docs/changes/irc-cleanroom/evidence/implementer-brief.md) ·
  [Follow-up brief](../../docs/changes/irc-cleanroom/evidence/implementer-follow-up.md)
- **Review.** An independent reviewer had access to the earlier files,
  Halloy and the replacement. The reviewer found no sign of Halloy-derived
  expression in the reviewed code. The fixed interfaces and specified
  behaviour account for much of the overlap with the earlier files. The
  reviewer verified that the isolated repository contained none of the
  historical versions of those files.
  [Similarity review](../../docs/changes/irc-cleanroom/evidence/similarity-review.md)

## References consulted

- IRC messages and registration. [RFC 1459](https://www.rfc-editor.org/rfc/rfc1459),
  [RFC 2812](https://www.rfc-editor.org/rfc/rfc2812) and the
  [Modern IRC Client Protocol](https://modern.ircdocs.horse/).
- [CTCP](https://modern.ircdocs.horse/ctcp) and
  [ZNC playback](https://wiki.znc.in/Playback).
- IRCv3 [capability negotiation](https://ircv3.net/specs/extensions/capability-negotiation),
  [SASL 3.1](https://ircv3.net/specs/extensions/sasl-3.1),
  [SASL 3.2](https://ircv3.net/specs/extensions/sasl-3.2),
  [message tags](https://ircv3.net/specs/extensions/message-tags),
  [batches](https://ircv3.net/specs/extensions/batch),
  [chathistory](https://ircv3.net/specs/extensions/chathistory),
  [multiline](https://ircv3.net/specs/extensions/multiline),
  [read marking](https://ircv3.net/specs/extensions/read-marking),
  [metadata](https://ircv3.net/specs/extensions/metadata),
  [standard replies](https://ircv3.net/specs/extensions/standard-replies),
  [typing](https://ircv3.net/specs/client-tags/typing),
  [reply tags](https://ircv3.net/specs/client-tags/reply) and the
  [extension index](https://ircv3.net/specs/extensions/).
- SASL PLAIN, EXTERNAL and Base64.
  [RFC 4616](https://www.rfc-editor.org/rfc/rfc4616),
  [RFC 4422 Appendix A](https://www.rfc-editor.org/rfc/rfc4422#appendix-A)
  and [RFC 4648 §4](https://www.rfc-editor.org/rfc/rfc4648#section-4).
- TLS SNI and dependency APIs.
  [RFC 6066 §3](https://www.rfc-editor.org/rfc/rfc6066#section-3),
  [rustls configuration](https://docs.rs/rustls/latest/rustls/struct.ConfigBuilder.html),
  [certificate verifier](https://docs.rs/rustls/latest/rustls/client/danger/trait.ServerCertVerifier.html),
  [platform verifier](https://docs.rs/rustls-platform-verifier/latest/rustls_platform_verifier/struct.Verifier.html)
  and [PEM parsing](https://docs.rs/rustls-pki-types/latest/rustls_pki_types/pem/trait.PemObject.html).

## Specification interpretation

The parser skips malformed tag entries with an empty key, following IRCv3's
nonempty tag-key grammar. This also keeps serialisation unambiguous.
The supplied acceptance cases retain their inputs and assertions. They are
grouped by subsystem, with shared fixture steps factored out of network tests.

## Round 2

On 19 November 2026 the implementer revised the clean-room code using
`TASK2.md`, `spec.md` and the protocol references below.

- A 30-second deadline covers each transport write and flush. Expiry drops
  the link and reports uncertain sends through the existing write failure path.
- Each session tracks at most 64 open batches and buffers at most 10,000
  messages per batch, including messages merged from children. Opening another
  batch releases the oldest one. A batch that exceeds its message limit releases
  what fits and ignores the remaining components until close. History released
  early is incomplete. [IRCv3 Batch](https://ircv3.net/specs/extensions/batch).
- Registration tries the configured alternate nicks and up to three appended
  underscores. Exhaustion or a candidate beyond the advertised `NICKLEN` byte
  limit ends the attempt with a failed status. This limits the unbounded
  fallback described in section 15.2 and is a deliberate deviation under
  section 18.2. [NICKLEN](https://modern.ircdocs.horse/#nicklen-parameter).
- A SASL exchange still in progress after 30 seconds is aborted. Capability
  negotiation then follows the usual progress rules, and late challenges to
  the timed-out exchange are ignored. [IRCv3 SASL 3.1](https://ircv3.net/specs/extensions/sasl-3.1#the-authenticate-command).
- Automatic CTCP replies are limited to one every three seconds per folded
  sender and ten per rolling minute per connection to limit NOTICE floods.
  [CTCP](https://modern.ircdocs.horse/ctcp).
- Section 7.2 clarification. The parser drops entries with an empty key and
  still accepts empty values on named tags. [IRCv3 Message Tags](https://ircv3.net/specs/extensions/message-tags#format).
- The IRC crate inherits the desktop workspace’s declared Rust 1.99.0 floor,
  matching the mise pin.
