# Spec: NETT issue client core

## Ownership

`clients/nett` (`subspace-nett-client`) owns blocking HTTP, NETT JSON
projections, WorkOS device authentication, the per-profile session, non-secret
profiles, refresh-token storage and resolution caching. It depends only on
services and clients, with no GPUI dependency. Shared pure recognition lives in
`subspace_contracts::issues::recognition`; NETT Markdown preparation lives in
`spaces/issues`. These ownership moves complete the
[board-contracts placement decision](../board-contracts/spec.md#6-placement-of-frameworkservicesissues).
The existing `clients/test-support` fixture now captures
request headers and can return response headers; its original API remains.

## Reads

Append `/v1/` and the route below to the configured API base, such as
`https://nett.example.com/api`.

| GET path | Result |
| --- | --- |
| `auth/orgs` | Accessible organisation array |
| `orgs/{orgId}/issue-teams` | Paged teams and workflow states |
| `orgs/{orgId}/issues?team=…` | Paged issues, including sub-issue counts |
| `orgs/{orgId}/issues/{issueRef}` | Issue/tracker, progress, parent, children and relations |
| `orgs/{orgId}/issues/{issueRef}/comments` | Paged roots |
| `orgs/{orgId}/issues/{issueRef}/comments/{rootId}/replies` | Paged replies |
| `orgs/{orgId}/issues/{issueRef}/relations` | Paged relations |

Every call sends the in-memory bearer; organisation paths also send matching
`X-Org-Id`. Paging is finite and explicit. The list supports a team or parent,
optional order, search and archived inclusion. Continue detail children with
their cursor, parent ID, manual order and a page size of 20. Teams, issues and
relations accept limits 1–100; comments and replies accept 1–50. Search is
trimmed, omitted if blank, and limited to 200 UTF-16 units as NETT counts them.
Comment roots do not contain reply bodies; opening a thread
reads its replies. DTOs project fields the first panes need, preserve NETT's
JSON names and ignore extra fields without copying unused enum catalogues.

Contracts were checked against Indra's
`frontend/indra/src/lib/api/generated/schema.d.ts` at
`140c38abf950dbdc7d2d04b0ccbb25cb605c17ec`. These are ordinary REST routes,
not MCP adapters. JSON responses are bounded to 8 MiB; authentication/error
responses to 64 KiB. Tokens, provider bodies and raw transport errors never
enter error messages. Existing platform trust, proxy and timeout handling is
reused, with redirects disabled for every request, including same-host ones.

Errors distinguish API 401, Access admission, 403, 404, network failure,
unexpected statuses and invalid/oversized JSON. Access redirects are recognised
by the `*.cloudflareaccess.com` host. Access 401s use an Access challenge header
or bounded HTML sign-in markers. A completely unmarked empty Access 401 cannot
be distinguished from an API 401: the workspace specification and development access update must establish a reliable
deployed signal. `cf-ray` alone does not distinguish them.

## Profiles and sign-in

The `nett-dev` profile uses `workOsDevice` and a deployment-specific public
client ID. The placeholder is `client_00000000000000000000000000`. The
original profile is described in Indra's
`cli/Indra.Sdk/Auth/CliAuthProfile.cs`, `CliAuthProfiles.Development`.
`WorkOsDeviceClient.cs`, `WorkOsDeviceModels.cs` and their CLI tests supply the
flow contract. The WorkOS endpoints and an example verification origin are:

- Start: `https://api.workos.com/user_management/authorize/device`.
- Poll/exchange and refresh:
  `https://api.workos.com/user_management/authenticate`.
- Verification origin:
  `https://auth.nett.example.com`.

Provider requests use form encoding and never include a NETT bearer,
organisation header or client secret. Verification URLs must remain on the
checked-in HTTPS origin; a complete URL must carry exactly the returned user
code. Polling waits first, honours the supplied interval (five seconds when
omitted), adds five seconds on `slow_down`, and terminates on expiry, denial or
cancellation. The monotonic deadline includes time spent displaying the code.
Each poll retries network failures, HTTP 408/429 and 5xx up to three attempts,
following indractl's 200/400 ms backoff. A positive `Retry-After` delta or HTTP
date takes precedence. Every wait and request stays within the device deadline;
exhaustion returns a safe error for an explicit retry.

Access tokens stay in memory. JWT expiry is a scheduling hint; NETT owns bearer
validation. Where both claims exist, expiry is receipt time plus `exp - iat`,
independent of server/client clock skew. Without `iat`, use `exp` without
rejecting a locally past timestamp. Save rotation before parsing access claims;
even malformed access cannot discard a valid rotated refresh token. Refresh
begins within a minute of expiry. Terminal `invalid_grant` clears access and
deletes only a stored value still equal to the rejected token. Transient refresh
or store-read failure keeps unexpired access usable. A missing refresh entry
keeps state `RefreshRequired` while access lives, then becomes `SignedOut`.
Expired access is discarded. A rotated refresh-token save gets
three attempts. Exhaustion returns `KeychainSaveFailed`, best-effort deletes
the stale refresh entry and blocks automatic refresh in that session until a
new sign-in or sign-out. Existing access is retained until expiry but is not
returned while that failure is latched. Use one serialised background session
per profile to avoid concurrent refresh-token rotation. A cross-process profile
lock is required before introducing a second process such as headless tools.

Production uses `keyring` 4.2's native `v1` adapter for
[Keychain, Credential Manager and Secret Service](https://docs.rs/keyring/4.2.0/keyring/v1/index.html).
Its service namespace is `<product/channel directory>.api-refresh`, with one
entry named by profile ID. There is no file or memory fallback for a locked or
unavailable keychain. Tests explicitly inject `MemoryCredentialStore`.
Sign-out drops access and retries deletion of only that profile's refresh entry
up to three times. `SignedOut` never restores from the store, even if deletion
failed; later client calls retry pending deletion and require explicit sign-in.
The pane
must drop outstanding clients and reject stale results on sign-out/scope change.

Runtime profile metadata contains base URL, auth kind, public client ID,
selected organisation and fallback team keys. Saved `nettDev` contains only
selected organisation and team keys; current built-ins are supplied on load.
Legacy copies and unknown profile fields are ignored and dropped on rewrite.
Standalone profiles still save their separately configured base and auth kind.
`ProfileSettings` uses the existing
private, atomic, conflict-aware `SettingsFile` transaction for
`issues/config.json`. Token results cannot be serialised. Constructors validate
metadata before credentials are sent; the dev origin/client cannot be rebound.
Other services' environment/config/CLI priority stays unchanged.

The `subspace-api` placeholder holds its own HTTPS base and `cloudflareAccess`
kind. Its separate sign-in is deferred; the NETT session refuses this profile.

## References and subsequent slices

`ReferenceRecognizer` uses loaded team keys, falling back to configured keys
(default `NET`) only when no valid loaded keys remain. Both sources discard
keys outside NETT's `[A-Z][A-Z0-9]{1,4}` rule. It matches complete positive-number
identifiers at word boundaries, returning each UTF-8 byte range in input order,
including duplicates. Hyphenated prefixes and date continuations do not match;
`UTF-8` and `ISO-8601` remain plain even with matching team keys. URLs and closed
backtick code spans are excluded; unmatched runs are literal and scanning
continues to find later closed spans.
Recognition grants no access. Keys match canonical loaded/configured case exactly.

`IssueResolver` is an associated-type scalar seam implemented by the existing
issue detail GET. The Indra revision examined here predates the batch reference endpoint.
the 14 November batch-resolver decision and `docs/auth.md` describe later Build,
and neither backend source nor the generated REST schema has `references/resolve`.

Existing IRC settings accept arbitrary network hosts, including
`irc.nett.example.com`, without code changes. This delivery does not configure
or connect it.

The next Claude crew builds Slice 1's board, sign-in prompt, organisation/team
picker, list/detail/thread panes and clickable IRC references. Slice 2 adds
activity, images and guarded media reads/rendering, broader filters and the
standalone Access sign-in. Batch resolution depends on the reference endpoint. Writes, costs,
documents and website navigation remain outside scope.

The issue workspace specification must settle pane registration/scope ownership, default organisation and
team, stale-result disposal, refresh/retry presentation, sign-out with open
panes, the standalone endpoint/protocol and the Access-401 signal. Retired-key
discovery and case-insensitive recognition need explicit contracts if required.
