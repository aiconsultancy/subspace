# Connection guidance

## Intent

Help people recover when Channels, an API-backed Space or Fleet Console cannot
reach its service.

## Specification

- A loopback address suggests checking the local service and any local VM or
  tunnel.
- Other addresses suggest checking network connectivity and connecting to the
  organisation's private network if required. People using Cloudflare One
  (WARP) are prompted to refresh their Access session and complete any
  private-app sign-in.
- Fleet Console keeps its retry countdown and distinguishes HTTP errors from
  a missing response. IRC keeps certificate and authentication errors specific.

## Delivery

The changes use the existing API offline screen, IRC reconnect status and
Fleet Console banner and connection test. The configured address determines the
guidance. Tests exercise local and remote addresses and preserve separate
token and TLS errors.

## Evidence

Twilight Two fixture captures before and after for the loopback API and local
and remote IRC and Fleet Console endpoints are in [evidence](evidence/).
