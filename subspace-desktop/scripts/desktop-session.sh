#!/usr/bin/env bash
# Run a command in the macOS desktop login session (the Aqua session), where
# codesign can use the Developer ID key, from any shell: SSH, an agent, tmux.
# Usage: desktop-session.sh COMMAND [ARGUMENT...]
#
# It bootstraps a one-off launchd agent into gui/<uid> that runs the command
# in the current folder, with no window, streams its output here while it
# runs, and exits with its status. The agent and its files are removed on
# exit, also on interrupt. The agent gets PATH, HOME and the variables named
# below, never one whose name looks like a credential: secrets stay in the
# login keychain, which the command reads inside the session.
# Environment for tests: DESKTOP_SESSION_POLL (seconds between polls).
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
	echo "desktop-session.sh runs only on macOS" >&2
	exit 2
fi
if [[ "$#" -eq 0 ]]; then
	echo "usage: desktop-session.sh COMMAND [ARGUMENT...]" >&2
	exit 2
fi

poll="${DESKTOP_SESSION_POLL:-1}"
uid="$(id -u)"
label="com.subspace.desktop-session.$$.$(date +%s).$RANDOM"
state="$(mktemp -d "${TMPDIR:-/tmp}/subspace-desktop-session.XXXXXX")"
chmod 700 "$state"
log="$state/output.log"
plist="$state/$label.plist"
: >"$log"

# shellcheck disable=SC2329 # Called by the EXIT trap.
cleanup() {
	launchctl bootout "gui/$uid/$label" >/dev/null 2>&1 || true
	rm -rf "${state:?}"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

forwarded() {
	local name
	for name in $(compgen -e); do
		case "$name" in
		*TOKEN* | *SECRET* | *PASSWORD* | *CREDENTIAL* | *_KEY | *_KEY_*) continue ;;
		PATH | HOME | USER | LOGNAME | SHELL | LANG | LC_* | TERM | TMPDIR) ;;
		SIGNING_IDENTITY | KEYCHAIN_PROFILE | RELEASE_NOTES_FILE | RELEASE_REPO | RELEASE_DRAFT | DRAFT) ;;
		COPILOT_SKIP_CLI_DOWNLOAD | GH_CONFIG_DIR | XDG_CONFIG_HOME | XDG_CACHE_HOME | XDG_DATA_HOME) ;;
		MISE_* | SUBSPACE_* | CARGO_* | RUSTUP_*) ;;
		*) continue ;;
		esac
		printf '%s\0' "$name"
	done
}

cat >"$state/run.sh" <<'SH'
#!/bin/bash
# Inside the session: record the process, run the command, record its status.
state="$1" directory="$2"
shift 2
echo "$$" >"$state/pid"
status=0
cd "$directory" && "$@" </dev/null || status=$?
echo "$status" >"$state/status.partial"
mv "$state/status.partial" "$state/status"
SH

# plistlib writes valid XML for any argument or value.
forwarded | python3 -c '
import os, plistlib, sys
label, plist, log, *arguments = sys.argv[1:]
names = [name for name in sys.stdin.read().split("\0") if name]
with open(plist, "wb") as file:
    plistlib.dump({
        "Label": label,
        "ProgramArguments": arguments,
        "EnvironmentVariables": {name: os.environ[name] for name in names},
        "RunAtLoad": True,
        "StandardOutPath": log,
        "StandardErrorPath": log,
        "ProcessType": "Interactive",
    }, file)
' "$label" "$plist" "$log" /bin/bash "$state/run.sh" "$state" "$PWD" "$@"

launchctl bootstrap "gui/$uid" "$plist"

offset=0
emit() {
	local size
	size="$(wc -c <"$log" | tr -d ' ')"
	if ((size > offset)); then
		tail -c "+$((offset + 1))" "$log" | head -c "$((size - offset))"
		offset="$size"
	fi
}

waited=0
while [[ ! -f "$state/status" ]]; do
	emit
	if [[ -f "$state/pid" ]]; then
		if ! kill -0 "$(cat "$state/pid")" 2>/dev/null && [[ ! -f "$state/status" ]]; then
			emit
			echo "the command stopped in the desktop session without a status" >&2
			exit 1
		fi
	elif ((waited >= 30)); then
		echo "launchd did not start the command in the desktop session (gui/$uid)" >&2
		exit 1
	fi
	sleep "$poll"
	waited=$((waited + 1))
done
emit
exit "$(cat "$state/status")"
