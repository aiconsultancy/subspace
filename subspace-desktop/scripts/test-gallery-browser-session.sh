#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir "$fixture/bin"
cat > "$fixture/bin/uname" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$FIXTURE_PLATFORM"
SH
cat > "$fixture/bin/dbus-run-session" <<'SH'
#!/usr/bin/env bash
printf 'private-session\n' >> "$FIXTURE_LOG"
test "$1" = --
shift
exec "$@"
SH
cat > "$fixture/bin/fixture-command" <<'SH'
#!/usr/bin/env bash
printf '<%s>\n' "$@" >> "$FIXTURE_LOG"
exit 17
SH
chmod +x "$fixture/bin/"*
export FIXTURE_LOG="$fixture/result"
for platform in Linux Darwin MINGW64_NT; do
    export FIXTURE_PLATFORM="$platform"
    : > "$FIXTURE_LOG"
    status=0
    PATH="$fixture/bin:$PATH" bash "$root/scripts/run-gallery-browser-tests.sh" \
        fixture-command 'one argument' '--test-name-pattern=exact test' || status=$?
    test "$status" = 17
    if [[ "$platform" = Linux ]]; then
        printf 'private-session\n<one argument>\n<--test-name-pattern=exact test>\n' > "$fixture/expected"
    else
        printf '<one argument>\n<--test-name-pattern=exact test>\n' > "$fixture/expected"
    fi
    cmp "$FIXTURE_LOG" "$fixture/expected"
done
echo 'Gallery browser runner: Linux session, other platforms, arguments and status pass.'
