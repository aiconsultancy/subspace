#!/usr/bin/env bash
# Exercise desktop-session.sh with a fake launchctl that runs the agent's
# program as launchd would: only the plist's environment, output to its log.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/subspace-desktop-session-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "${fixture:?}"' EXIT
mkdir -p "$fixture/bin" "$fixture/plists" "$fixture/work" "$fixture/tmp"
cat >"$fixture/bin/launchctl" <<'PY'
#!/usr/bin/env python3
import os, plistlib, shutil, signal, subprocess, sys
fixture = os.environ["FIXTURE"]
with open(f"{fixture}/launchctl.log", "a") as log:
    log.write(" ".join(sys.argv[1:]) + "\n")
command = sys.argv[1]
if command == "managername":
    print(os.environ.get("FAKE_LAUNCHD_MANAGER", "Background"))
elif command == "bootstrap":
    assert sys.argv[2] == f"gui/{os.getuid()}", sys.argv[2]
    shutil.copy(sys.argv[3], f"{fixture}/plists/")
    with open(sys.argv[3], "rb") as file:
        job = plistlib.load(file)
    assert job["RunAtLoad"] is True
    output = open(job["StandardOutPath"], "ab")
    process = subprocess.Popen(job["ProgramArguments"], env=job["EnvironmentVariables"], cwd="/",
                               stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                               start_new_session=True)
    with open(f"{fixture}/job-{job['Label']}", "w") as file:
        file.write(str(process.pid))
elif command == "bootout":
    label = sys.argv[2].rsplit("/", 1)[1]
    try:
        with open(f"{fixture}/job-{label}") as file:
            os.killpg(int(file.read()), signal.SIGTERM)
    except (FileNotFoundError, ProcessLookupError):
        pass
else:
    sys.exit(f"unexpected launchctl call: {sys.argv[1:]}")
PY
chmod 755 "$fixture/bin/launchctl"
export FIXTURE="$fixture" PATH="$fixture/bin:$PATH" TMPDIR="$fixture/tmp/" DESKTOP_SESSION_POLL=0.05
helper="$root/scripts/desktop-session.sh"

# Output streams back, the folder and the allowed variables cross, the
# command's status comes back, and no credential reaches the plist.
cd "$fixture/work"
status=0
SIGNING_IDENTITY='Developer ID Application: Fixture (TEAMID)' RELEASE_DRAFT=1 MISE_ENV=terminal \
	SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET=fixture-oauth-secret MISE_GITHUB_TOKEN=fixture-token \
	UNRELATED=fixture-unrelated \
	bash "$helper" bash -c 'pwd; echo "identity=$SIGNING_IDENTITY draft=$RELEASE_DRAFT env=$MISE_ENV"
		echo "secret=${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-unset} token=${MISE_GITHUB_TOKEN:-unset}"
		echo "unrelated=${UNRELATED:-unset} home=$HOME"; echo to-stderr >&2; exit 7' "argument with spaces" \
	>"$fixture/out" 2>"$fixture/err" || status=$?
test "$status" = 7
grep -Fxq "$fixture/work" "$fixture/out"
grep -Fxq 'identity=Developer ID Application: Fixture (TEAMID) draft=1 env=terminal' "$fixture/out"
grep -Fxq 'secret=unset token=unset' "$fixture/out"
grep -Fxq "unrelated=unset home=$HOME" "$fixture/out"
grep -Fxq 'to-stderr' "$fixture/out"
if grep -rFq -e fixture-oauth-secret -e fixture-token "$fixture/plists"; then
	echo 'a credential reached the plist' >&2
	exit 1
fi
grep -q '^bootout gui/[0-9]*/com.subspace.desktop-session\.' "$fixture/launchctl.log"
test -z "$(ls -A "$fixture/tmp")"

# Arguments cross unchanged, and every run has its own label.
# shellcheck disable=SC2016 # A literal $HOME must arrive unexpanded.
arguments=("argument with spaces" '$HOME' '<&>')
bash "$helper" printf '[%s]\n' "${arguments[@]}" >"$fixture/out"
test "$(cat "$fixture/out")" = "$(printf '[%s]\n' "${arguments[@]}")"
test "$(grep -c '^bootstrap ' "$fixture/launchctl.log")" = 2
test "$(find "$fixture/plists" -name '*.plist' | wc -l | tr -d ' ')" = 2

# An interrupt stops the command and removes the agent and its files.
bash "$helper" bash -c 'echo started; sleep 30; echo finished' >"$fixture/out" 2>&1 &
caller=$!
# Wait for the command to start, however long a busy machine takes.
for _ in $(seq 600); do grep -q started "$fixture/out" 2>/dev/null && break; sleep 0.05; done
grep -q started "$fixture/out"
kill -TERM "$caller"
status=0
wait "$caller" || status=$?
test "$status" = 143
if grep -q finished "$fixture/out"; then echo 'the interrupted command kept running' >&2; exit 1; fi
test "$(grep -c '^bootout ' "$fixture/launchctl.log")" = 3
test -z "$(ls -A "$fixture/tmp")"

# Only macOS has a desktop session.
mkdir "$fixture/linux"
printf '#!/usr/bin/env bash\necho Linux\n' >"$fixture/linux/uname"
chmod 755 "$fixture/linux/uname"
if PATH="$fixture/linux:$PATH" bash "$helper" true 2>"$fixture/err"; then
	echo 'the helper must refuse outside macOS' >&2
	exit 1
fi
grep -Fq 'runs only on macOS' "$fixture/err"
echo "desktop-session fixtures passed"
