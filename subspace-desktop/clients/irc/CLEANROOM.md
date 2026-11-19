# IRC client clean-room log

On 19 November 2026 the implementer wrote the 15 IRC client modules from
the 19 November 2026 version of `spec.md` and the protocol references
below. `interface-callers.md` and the visible code in this repository
were used to match interfaces. The brief supplied the acceptance tests.
The baseline omitted all 15 files. No excluded implementation, test,
repository history or derived artefact was consulted.

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
