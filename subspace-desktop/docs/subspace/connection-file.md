# The connection file

The downloaded Subspace app signs in to an API on this computer,
`http://127.0.0.1:3000`, as the local development stack and the fleet
installer's tunnel provide. To point the same download at a hosted API and
its sign-in instead, without building the app yourself, put a connection file
beside the app's settings.

## Where it goes

`connection.json`, in the folder that holds the app's `config.json`:

| System | Path |
| --- | --- |
| macOS | `~/Library/Application Support/subspace/connection.json` |
| Linux | `$XDG_CONFIG_HOME/subspace/connection.json`, by default `~/.config/subspace/connection.json` |
| Windows | `%APPDATA%\subspace\connection.json` |

When an existing `config.json` is in `~/.config/subspace/` on macOS or
Windows, the app uses that folder, and the connection file goes there too. A
pre-release channel uses its own folder, such as `subspace-nightly`.

The app reads the file when it starts. Without it, the app behaves as before.

## What it holds

One JSON object. Every setting is optional; one the file leaves out keeps the
app's built-in value.

```json
{
  "api_url": "https://api.example.com",
  "identity_provider": "workos",
  "workos_client_id": "client_00000000000000000000000000"
}
```

| Setting | Meaning |
| --- | --- |
| `api_url` | The Subspace API: HTTPS, or HTTP to this computer only (`127.0.0.1`, `[::1]` or `localhost`), optionally ending in `/api` |
| `identity_provider` | `workos`, `entra`, `dev`, or empty for no sign-in |
| `workos_client_id` | The WorkOS AuthKit client ID, such as `client_00000000000000000000000000` |
| `workos_api_url` | The WorkOS API; the default is `https://api.workos.com` |
| `workos_redirect_port` | A fixed loopback port for WorkOS's redirect, as text or a number |
| `entra_tenant_id`, `entra_client_id` | The Microsoft Entra tenant and its public-client registration, such as `00000000-0000-0000-0000-000000000000` |
| `entra_authority` | The Entra authority; the default is `https://login.microsoftonline.com` |
| `entra_api_scope` | The API's delegated scope, such as `api://00000000-0000-0000-0000-000000000000/API.Access` |
| `dev_email`, `dev_name`, `dev_organisation` | Who development sign-in (`dev`) signs in as |
| `nett_api_url` | A NETT build's API; other builds refuse it |

The rules are those of the same-named `SUBSPACE_*` environment variables
([Identity providers and the API URL](../../README.md#identity-providers-and-the-api-url)),
and an environment variable wins over the file. Development sign-in only ever
reaches an API on this computer. With `workos` or `entra`, the app asks you
to sign in when it opens; with `dev`, it signs in by itself.

The file holds public addresses and client IDs only. Never put a secret,
token or password in it: none of the settings is one, and the app does not
need one.

It cannot change what the app was built with: whether it has an API at all,
its name, title, logo or where it updates from. For those,
[make your own build](../build-your-own.md).

## The build with no API backend

The `-NoAPI` download has no API and nothing to sign in to. It refuses a
connection file that would switch an API or sign-in on, the same way it
refuses the environment variables; a file with no such setting, or only
empty ones, is fine.

Both apps keep their settings in the same folder and read the same file, so
before moving from the standard app to the `-NoAPI` one, rename or remove a
connection file that sets an API or sign-in, and put it back when you move
again.

## When something is wrong

If the file cannot be read, is not JSON, names a setting it cannot hold or
breaks a rule, the app does not start. It shows a window naming the file, the
setting and the problem, with **Quit**. Correct or remove the file and open
the app again. Started from a terminal, it also prints the problem there.
