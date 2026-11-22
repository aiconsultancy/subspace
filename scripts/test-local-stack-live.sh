#!/usr/bin/env bash
# Test a fresh, isolated local stack started with DEMO=0.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z "${COMPOSE_PROJECT_NAME:-}" || "$COMPOSE_PROJECT_NAME" == subspace ]]; then
    echo "Set an isolated COMPOSE_PROJECT_NAME and start that stack with DEMO=0 first." >&2
    exit 2
fi

api_binding="$(docker compose port api 3000)"
verify() {
    python3 scripts/local-stack/check-demo.py "${api_binding##*:}" "$1"
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
        # Who speaks where follows the standard channels: people and the
        # rear admiral in Starbase, the rear admiral and captains in command.
        # A ship channel has the captains, the rear admiral and people.
        fleet, _, suffix = name[1:].partition("-")
        agents = {account for account in accounts if account.startswith(f"{fleet}-")}
        if suffix == "starbase":
            allowed = {"developer", f"{fleet}-admiral"}
        elif suffix == "command":
            allowed = agents
        elif suffix:
            allowed = {"developer", *agents}
        else:
            allowed = set(accounts)
        assert {sender for sender, _ in entries} <= allowed, name
    print(json.dumps(result, sort_keys=True))
PY
}

mode=${STACK_LIVE_MODE:-empty}
if [ "$mode" = empty ]; then
    verify empty
    test "$(irc_counts)" = '{"#aurora-castor": 0, "#aurora-command": 0, "#aurora-lyra": 0, "#aurora-orion": 0, "#aurora-starbase": 0, "#aurora-vega": 0, "#borealis-command": 0, "#borealis-rigel": 0, "#borealis-starbase": 0, "#subspace": 0}'
    make -s stack-demo
fi
initial=$(verify demo)
echo "$initial"
history=$(irc_counts)
echo "$history"
test "$history" = '{"#aurora-castor": 0, "#aurora-command": 2, "#aurora-lyra": 5, "#aurora-orion": 4, "#aurora-starbase": 2, "#aurora-vega": 0, "#borealis-command": 2, "#borealis-rigel": 4, "#borealis-starbase": 2, "#subspace": 3}'
make -s stack-demo | grep -Fx 'Demo IRC history ready: 0 new messages.'
test "$(verify demo)" = "$initial"
test "$(irc_counts)" = "$history"
echo "Live stack fixtures and idempotency passed"
