# Independent IRC clean-room similarity review

An independent reviewer examined the initial replacement on 19 November
2026. The reviewer had access to the earlier modules, the replacement and
Halloy. This summary records the results for that initial implementation.
[Follow-up implementer brief](implementer-follow-up.md)

## Verdict

The reviewer found no sign of Halloy-derived expression in the 15 new
files, the 67 supplied acceptance tests or the Rust blocks in the functional
specification. The matching runs were explained by Rust boilerplate, public
protocol requirements and the fixed interfaces the specification supplied.

## Method

- Tokenise Rust, dropping whitespace and comments while keeping literals.
  Compare exact tokens and repeat with non-keyword identifiers normalised to
  a common token. Measure the share of each left-hand file's distinct
  eight-token sequences found in each comparison corpus. Also measure the
  longest contiguous exact match.
- Compare the new and earlier files with the corresponding Halloy files and
  Halloy's broader IRC and data source trees (133 files, snapshot at
  `ce6da64a`, 17 November 2026). Use the earlier adaptation as a calibration
  and the independently written `irc` and `irc-proto` crates, both at 1.1.0,
  as a same-protocol baseline.
- Compare the earlier and new versions in both directions at sequence
  lengths of 8, 12, 16 and 24 tokens. Exclude sequences from materials the
  implementer could see (the supplied Rust blocks and acceptance tests, and
  the 275 other Rust files from the client and Channels Space baseline) and
  from the independent crates when measuring matches unique to Halloy.
- Check shared string literals of 10 or more characters, including the
  earlier test inputs copied from Halloy. Compare the other 58 files of
  the IRC client with Halloy separately.

## Per-file results

Percentages are exact / identifier-normalised eight-token shares. Run
lengths are exact token counts. The Halloy run column compares each version
with its corresponding Halloy file. The Halloy-only column shows the
longest run containing at least 12 tokens shared with Halloy after excluding
matches in the other version, the visible material and the independent crates.

| File | New tokens | New vs earlier (%) | New vs Halloy (%) | New vs independent crates (%) | Earlier vs Halloy (%) | Run: new vs earlier | Halloy run: new / earlier | Halloy-only run: new / earlier | Reviewer observation |
| --- | ---: | --- | --- | --- | --- | ---: | ---: | ---: | --- |
| `codec.rs` | 426 | 19.7 / 28.0 | 1.4 / 9.2 | 0.5 / 31.3 | 0.7 / 7.4 | 27 | 15 / 18 | 12 / 0 | Shared constants and signatures come from the spec. Halloy matches are derive and error boilerplate. |
| `connection/mod.rs` | 451 | 39.6 / 50.9 | 5.1 / 18.4 | 4.4 / 41.2 | 6.4 / 18.2 | 61 | 23 / 19 | 23 / 18 | The spec fixes the public types, function signature and errors. The longest Halloy run is a common `thiserror` conversion. |
| `connection/tls.rs` | 868 | 13.6 / 33.4 | 4.0 / 20.8 | 6.8 / 48.1 | 13.7 / 26.1 | 35 | 16 / 23 | 12 / 23 | The new verifier uses the provider's signature algorithms and reads PEM files asynchronously. The shared trait dictates the remaining overlap. |
| `network.rs` | 2450 | 34.4 / 51.5 | 0.7 / 19.3 | 0.5 / 28.8 | 0.4 / 19.4 | 100 | 17 / 14 | 15 / 12 | The longest shared run comes from interfaces in spec §4.1. Other matches follow the required connection-loop behaviour. |
| `proto/ctcp.rs` | 126 | 86.6 / 88.0 | 5.0 / 32.5 | 0.0 / 35.9 | 5.6 / 32.3 | 57 | 15 / 16 | 0 / 0 | The four-line body matches the earlier local file, following the spec's four steps. Halloy uses a different parsing sequence and parameter representation. |
| `proto/mod.rs` | 392 | 68.0 / 73.5 | 8.1 / 15.6 | 9.5 / 57.8 | 15.7 / 23.7 | 127 | 23 / 47 | 15 / 26 | The spec fixes the accessors. The new user source has named fields. |
| `proto/parse.rs` | 789 | 22.0 / 45.8 | 0.3 / 10.2 | 1.7 / 29.4 | 0.9 / 11.0 | 51 | 13 / 24 | 0 / 18 | The parameter loop follows spec §7.2. Halloy uses `nom`. |
| `proto/serialise.rs` | 490 | 8.2 / 19.3 | 0.0 / 6.7 | 2.1 / 30.0 | 0.2 / 11.9 | 22 | 11 / 16 | 0 / 13 | IRCv3 fixes the message-tag escape table. |
| `proto/tests.rs` | 164 | 13.4 / 38.4 | 2.0 / 22.4 | 5.4 / 32.8 | 1.8 / 10.3 | 15 | 10 / 12 | 0 / 12 | The new test inputs were independently written. |
| `session/batch.rs` | 722 | 11.0 / 28.8 | 1.4 / 38.0 | 0.0 / 29.1 | 2.3 / 42.1 | 28 | 11 / 14 | 0 / 12 | No Halloy-only run reaches 12 tokens in the replacement. |
| `session/caps.rs` | 1522 | 20.8 / 39.1 | 0.5 / 12.0 | 0.1 / 24.6 | 0.5 / 12.5 | 46 | 14 / 16 | 14 / 16 | The spec fixes the handler signatures and the order of the capability list. |
| `session/isupport.rs` | 736 | 63.7 / 76.0 | 0.3 / 17.0 | 0.7 / 31.2 | 0.7 / 18.3 | 165 | 17 / 17 | 13 / 0 | The spec supplies the struct, defaults and recognised keys. RFC 1459 fixes the case-mapping table. |
| `session/messages.rs` | 835 | 35.8 / 40.0 | 0.4 / 33.1 | 0.0 / 34.8 | 0.2 / 36.2 | 49 | 10 / 9 | 0 / 0 | The spec fixes the signatures and the status message format. |
| `session/mod.rs` | 1779 | 63.3 / 68.0 | 1.6 / 49.8 | 0.5 / 38.1 | 1.8 / 49.6 | 153 | 17 / 17 | 0 / 13 | The spec supplies state fields and helper signatures. Generic Rust structs account for much of the normalised overlap. |
| `session/registration.rs` | 972 | 30.9 / 40.8 | 0.4 / 37.3 | 0.1 / 29.9 | 0.5 / 40.4 | 50 | 9 / 11 | 0 / 0 | No Halloy-only run reaches 12 tokens in either version. |
| **All 15** | **12722** | **35.6 / 58.4** | **3.0 / 49.0** | **1.4 / 33.1** | **4.0 / 48.5** | | | | |

The short sequence shares include generic Rust patterns, especially after
identifier normalisation. At 24 exact tokens the broader comparison found
zero Halloy-only sequences in the new files, versus 36 in the earlier files.
At 12 tokens it found 33 in the new files and 125 in the earlier files.
There were 20 exact 12-token sequences common to the new files, earlier files
and Halloy after excluding visible sources and independent crates. They
formed seven runs, each at most 19 tokens, all generic Rust.

Of the 4,143 distinct eight-token sequences shared by the new and earlier
files, 1,458 also appeared in materials available to the implementer. The
specification supplies the full caller-facing types and signatures and spells
out behaviour such as parameter parsing, CTCP and welcome handling. These
requirements explain the longer matches between the two local versions.
[Functional specification](spec.md) · [Caller inventory](interface-callers.md)

The earlier files shared recognisable test inputs with Halloy. The new files
shared 21 literals of the measured length with Halloy, all IRC protocol
identifiers. The new and supplied acceptance tests shared no distinctive
Halloy test vectors. The longest exact runs against Halloy in the
specification's Rust blocks and supplied acceptance tests were 19 and 18
tokens respectively, all boilerplate. Among the other 58 files in the
crate, the longest run was a generic 24-token test assertion. The reviewer
also checked `session/dispatch.rs` and `session/numerics.rs` and found no
evidence of derivation.

## Process checks

- The isolated repository had one baseline commit omitting all 15 files
  and two implementation commits. Its object store contained none of the
  49 historical blobs checked for the replaced files, including their
  earlier locations.
- The reviewer checked the final implementation run's log (104 commands
  and seven web fetches). Paths accessed were within the isolated
  repository or brief folder. The web fetches were to IRCv3, RFC Editor
  and docs.rs. The log covers that run only.
- The supplied specification and acceptance files matched the
  specification author's copies byte for byte before publication edits.
  Integration kept all 67 acceptance cases, their inputs and all 260
  assertion macros. Test grouping changed.
