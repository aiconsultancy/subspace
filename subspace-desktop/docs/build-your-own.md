# Make your own build

The published Subspace release signs in to an API on this computer
(`http://127.0.0.1:3000`), and its `-NoAPI` download has no API at all. A
[connection file](subspace/connection-file.md) points the standard download
at a hosted API and sign-in without rebuilding. Build the app yourself when
you need what that file cannot change: the backend, the name, title prefix
and logo, the GitHub OAuth App, the Report issue repository or where the app
updates from.

Keep private build configurations out of this public repository: put your
build config file elsewhere and name it with `SUBSPACE_BUILD_CONFIG`. Never
commit a real tenant, client ID, private API address, signing identity or
secret.

## The build configuration file

`products/subspace/build-config.json`, or the JSON file
`SUBSPACE_BUILD_CONFIG` names (an absolute path, or one relative to
`products/subspace`), sets what the app is built with.
`products/subspace/build/branding.rs` checks it when the app is built and
refuses an invalid one. Every field is optional; the checked-in file is the
local development stack.

| Field | What it does |
| --- | --- |
| `api_backend` | `subspace` (empty is the same), `nett`, or `none` for an app with no API: no account, no Issues or Decisions, Channels configured by hand |
| `api_url` | The Subspace API: HTTPS, or HTTP to this computer only, optionally ending in `/api`. Not with `nett` or `none` |
| `nett_api_url` | A NETT build's API, required with `api_backend` `nett` and refused otherwise |
| `identity_provider` | `dev`, `workos`, `entra`, or empty for no sign-in. `dev` signs in by itself to an API on this computer; `workos` and `entra` ask the person to sign in when the app opens. A NETT build needs `workos`; a `none` build takes none |
| `workos_client_id` | The WorkOS AuthKit client ID, `client_` and letters and digits; required for `workos` |
| `workos_api_url` | The WorkOS API; the default is `https://api.workos.com` |
| `workos_redirect_port` | A fixed loopback port for the WorkOS redirect, registered in WorkOS as `http://127.0.0.1:<port>/callback`. Not with `nett` |
| `entra_tenant_id`, `entra_client_id` | The Entra tenant and its public-client registration, both non-nil UUIDs; required for `entra` |
| `entra_authority` | The Entra authority; the default is `https://login.microsoftonline.com` |
| `entra_api_scope` | The API's delegated scope |
| `dev_email`, `dev_name`, `dev_organisation` | Who `dev` signs in as; `dev` also needs `api_url` |
| `organization_name` | The organisation the account is labelled with, such as in the sign-in dialog |
| `title_prefix` | A prefix for the window title, menus and About |
| `about_logo` | An SVG logo for About, relative to the config file |

The same-named `SUBSPACE_*` environment variables, and the
[connection file](subspace/connection-file.md), override the API and sign-in
fields when the app starts. They cannot change `api_backend`, the branding
or the update source. See
[Identity providers and the API URL](../README.md#identity-providers-and-the-api-url).

### A WorkOS build

```json
{
  "identity_provider": "workos",
  "api_url": "https://api.example.com",
  "workos_client_id": "client_00000000000000000000000000",
  "organization_name": "Example Works",
  "title_prefix": "Example"
}
```

Register `http://127.0.0.1:*/callback` (or the fixed port's URL) as a
redirect URI for the client in WorkOS.

### An Entra build

```json
{
  "identity_provider": "entra",
  "api_url": "https://api.example.com",
  "entra_tenant_id": "00000000-0000-0000-0000-000000000001",
  "entra_client_id": "00000000-0000-0000-0000-000000000002",
  "entra_api_scope": "api://00000000-0000-0000-0000-000000000003/API.Access",
  "organization_name": "Example Works"
}
```

Register a public client on the Mobile and desktop platform with the redirect
`http://localhost/auth/callback` and no secret, and grant it the API scope.

Build and run either with:

```bash
SUBSPACE_BUILD_CONFIG=/path/outside/the/repository/example.json make run
```

## Build environment variables

| Variable | Default | What it does |
| --- | --- | --- |
| `SUBSPACE_BUILD_CONFIG` | `build-config.json` | The build configuration file above |
| `SUBSPACE_GITHUB_OAUTH_CLIENT_ID` | Subspace's OAuth App | The GitHub OAuth App for **Sign in with GitHub**; empty builds without it |
| `SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET` | none | That OAuth App's client secret, compiled in. The release scripts read it from the login keychain (service `subspace-github-oauth`, account `client-secret`); without it the app signs in to GitHub with a code |
| `SUBSPACE_REPORT_ISSUE_REPOSITORY` | the release repository | The `owner/name` **Report issue** files into; empty builds without Report issue |
| `SUBSPACE_RELEASE_CHANNEL` | `stable` | `nightly`, `alpha` or `beta` builds a pre-release with its own name, bundle id, data folders and update channel |
| `SUBSPACE_DEFAULT_THEME` | none | A theme every install of this build opens in |
| `SUBSPACE_BUILD_FEATURES` / `FEATURES=` | none | Extra Cargo features, such as `terminal`. Release packaging refuses the unreleased Spaces |

[Build configuration](installation.md#build-configuration) has more on the
OAuth App and Report issue.

## What a fork changes in the code

| Constant | File | What it sets |
| --- | --- | --- |
| `RELEASE_REPOSITORY` | `products/descriptor/src/lib.rs` | The repository the app updates from and Report issue files into by default |
| `REPOSITORY` | `products/subspace/src/main.rs` | The source link in About |
| `GITHUB_OAUTH_CLIENT_ID` | `products/subspace/build/config.rs` | The default GitHub OAuth App |

Keep the bundle id (`com.aiconsultancy.subspace` and its channel variants):
it decides where the app keeps its settings, state and keychain entries, and
changing it strands existing installs.

## Signing and publishing

| Variable | What it does |
| --- | --- |
| `SIGNING_IDENTITY` | Your Developer ID Application identity, for `make package-macos` and `make release-macos` |
| `KEYCHAIN_PROFILE` | The `notarytool` keychain profile for notarization |
| `RELEASE_REPO` | The `owner/name` `make release-macos` publishes to |
| `DRAFT=1` | Attach the files but leave the release a draft |
| `RELEASE_NOTES_FILE` | Notes instead of the changelog's |

`make release-macos` publishes both variants, the standard app from
`build-config.json` and the one with no API backend from
`build-config.no-api.json`, ignoring any `SUBSPACE_BUILD_CONFIG` in the
shell. For a fork, edit those two files on your release branch, or package
with `make package-macos` and your own `SUBSPACE_BUILD_CONFIG`. See
[Publish a macOS release](installation.md#publish-a-macos-release) and
[Configure release signing in GitHub](installation.md#configure-release-signing-in-github).
