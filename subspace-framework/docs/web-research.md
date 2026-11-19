# Web research

Subspace exposes `web_read` to Pi, MCP and generated CLI surfaces. Copilot chat
continues to use the Copilot CLI's hosted `web_search` and `web_fetch` tools.
Subspace does not add a search provider, API key or egress configuration for
those hosted tools.

## Configuration

Web reads are disabled until an administrator adds exact hosts or one-label
wildcards to `web.allowedHosts`:

```json
{
  "web": {
    "allowedHosts": ["docs.example.com", "*.example.org"],
    "silentAllowedHosts": ["docs.example.com"]
  }
}
```

`*.example.org` matches `docs.example.org`. It does not match `example.org` or
`a.docs.example.org`. A host in `silentAllowedHosts` must also be allowed. Other
reads require one per-call approval.

`web_read` accepts only HTTPS URLs without user information. It resolves each
redirect target and rejects loopback, private, link-local, carrier-grade NAT,
unique-local, multicast and unspecified addresses. It follows at most three
redirects. Responses are limited to text, Markdown, HTML, JSON and XML, one
MiB, a five-second connect deadline and a ten-second total deadline. HTML and
XML active content is removed. Per-process and per-session rate limits apply.
Every result is marked as untrusted evidence and cannot authorize writes,
widen scope or trigger another fetch.

## Optional search provider

The framework defines a provider interface but ships no provider. `web_search`
is absent unless a host supplies a provider matching `web.search.provider`.
The optional configuration contains only a provider identifier and HTTPS
endpoint. Provider credentials come from Subspace credential sources; they are
never tool arguments or result fields.
