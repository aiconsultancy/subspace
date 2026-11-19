# Isolated screenshot capture

Back to the [change records](../README.md).

## Intent

A helper's screenshot of the PRs board showed the owner's real GitHub
account, although its capture ran with an isolated `HOME`. The image was
deleted and never committed, but every agent was writing its own capture
script, so the gap could recur. The owner asked for captures that cannot
reach real accounts.

## Cause

`open` passes the caller's environment to the app it launches. The shell set
`XDG_CONFIG_HOME=/Users/<owner>/.config`, and `gh_config_dir`
(`framework/services/core/src/credentials.rs`) checks `GH_CONFIG_DIR`, then
`XDG_CONFIG_HOME`, before `$HOME/.config/gh`. The app read the real
`hosts.yml`, `gh auth token` returned the Keychain token, and with
`SUBSPACE_GITHUB_API_URL` unset the board called the live API. Debug builds
also share the stable Keychain name for the NETT session, so a capture of the
Issues board could have used the real NETT sign-in.

## Spec

- `scripts/capture.sh <out-dir> [steps]`, run as `make capture OUT=<dir>
  STEPS='...'`, is the only way to capture. It builds the debug capture build
  and starts it under `env -i`: macOS uses `open -g -n -W`; Linux launches the
  native executable in a private D-Bus session and a private Xvfb display.
  See [Desktop capture](../../capture.md). It passes:
  - a new `HOME` and `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` and
    `XDG_CACHE_HOME` inside it;
  - `GH_CONFIG_DIR` with the fictional `octo-dev` and `octo-dev_acme`
    accounts, a fixture `gh` that hands out `fixture-<login>` tokens, empty
    `GH_TOKEN` and `GITHUB_TOKEN`, a minimal `PATH` and no login-shell `PATH`;
  - `SUBSPACE_GITHUB_API_URL` naming `scripts/github_test_server.py`, and
    `SUBSPACE_API_URL` naming `scripts/subspace_api_test_server.py` with a
    private file holding the fixture Access assertion, both servers started on free
    loopback ports. The Subspace API fixture replaced the original NETT/WorkOS
    fixture and keeps all changes in memory.
- Options: `CAPTURE_THEME`, `CAPTURE_STATE` and `CAPTURE_CONFIG` (JSON merged
  into `state.json` and `config.json`), `CAPTURE_IRC_CONFIG` and
  `CAPTURE_KEEP_LOG`.
- `scripts/github_test_server.py` is the fictional GitHub API used for earlier
  evidence (the `acme` organisation and invented people), now in the
  repository with `--port`.
- AGENTS.md "UI change evidence", the developer checklist and its gotchas
  require `make capture` and forbid hand-launched captures.

## Delivery

- `scripts/test-capture.sh`, run by `make check` through the script tests on
  macOS and Linux, runs the script with stub builds and platform launchers while the caller
  sets a real-looking `XDG_CONFIG_HOME`, `GH_CONFIG_DIR` and `GH_TOKEN`. It
  checks that none of them reach `open` or the app, that every isolating
  variable is passed, that both fixture servers answer during the run, and
  that the fixture home and servers are gone afterwards. Removing `env -i` or
  the `XDG_CONFIG_HOME` line makes it fail.
- A real capture of the default PRs board shows `octo-dev` and the `acme`
  repositories.
- All 367 committed evidence images were checked with OCR for the owner's
  login and names, internal hosts and tokens. None shows a real account; the
  IRC images show the scripted fixture conversation on the local Ergo server.

## Tasks

- [x] Find the cause.
- [x] Check the committed evidence.
- [x] Add the capture script, the GitHub fixture server and their test.
- [x] Require them in AGENTS.md and the developer checklist.
