# WorkOS sign-in quality gate

## Intent

Restore the Framework quality gate after the addition of WorkOS device
authorisation.

## Spec

Preserve the browser and device flows, including cancellation, poll deadlines,
and refresh-token rotation. Keep the provider interface and stored credentials
compatible with existing callers and sessions.

## Delivery

The host presents sign-in prompts through a dedicated state update. The WorkOS
provider groups pending authorisations in a sign-in module, uses the shared
HTTP status helper, and keeps browser and device flow tests in separate files.

## Tasks

- [x] Resolve the nine quality findings in the host and WorkOS provider.
- [x] Preserve all browser and device session tests.
