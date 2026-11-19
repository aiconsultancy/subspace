#!/usr/bin/env bash
# Test a fresh, isolated local stack started with DEMO=0.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z "${COMPOSE_PROJECT_NAME:-}" || "$COMPOSE_PROJECT_NAME" == subspace ]]; then
    echo "Set an isolated COMPOSE_PROJECT_NAME and start that stack with DEMO=0 first." >&2
    exit 2
fi

verify() {
    python3 scripts/local-stack/check-demo.py "${API_PORT:-3000}" "$1"
}

irc_counts() {
    docker compose exec -T irc python3 - <<'PY'
import json
import os
import runpy
from collections import Counter
from contextlib import ExitStack
from pathlib import Path

client = runpy.run_path("/run/subspace-stack/seed-irc.py")["Client"]
accounts = {
    row["name"]: row["password"]
    for row in json.loads(Path(os.environ["IRC_ACCOUNTS_FILE"]).read_text())
}
with ExitStack() as scope:
    irc = client(scope, "developer", accounts["developer"])
    result = {}
    for channel in json.loads(Path(os.environ["IRC_CHANNELS_FILE"]).read_text()):
        name = channel["name"]
        irc.join(name)
        entries = [(sender, body) for sender, body in irc.history(name) if sender in accounts]
        assert all(count == 1 for count in Counter(entries).values()), name
        result[name] = len(entries)
    print(json.dumps(result, sort_keys=True))
PY
}

mode=${STACK_LIVE_MODE:-empty}
if [ "$mode" = empty ]; then
    verify empty
    test "$(irc_counts)" = '{"#aurora-command": 0, "#aurora-nav": 0, "#aurora-relay": 0, "#borealis-command": 0, "#borealis-warp": 0, "#subspace": 0}'
    make -s stack-demo
fi
initial=$(verify demo)
echo "$initial"
history=$(irc_counts)
echo "$history"
test "$history" = '{"#aurora-command": 3, "#aurora-nav": 5, "#aurora-relay": 4, "#borealis-command": 3, "#borealis-warp": 4, "#subspace": 3}'
make -s stack-demo | grep -Fx 'Demo IRC history ready: 0 new messages.'
test "$(verify demo)" = "$initial"
test "$(irc_counts)" = "$history"
echo "Live stack fixtures and idempotency passed"
