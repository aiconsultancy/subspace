#!/bin/sh
# Ergo auth-script hook: ask the Subspace API whether a login is valid.
#
# Ergo writes one JSON line on stdin and reads one JSON line from stdout.
# Only API-issued credentials (a passphrase with the configured prefix) and,
# when enabled, certificate logins reach the API. Logins to local-only
# accounts and every other login are refused at once, so a local account's
# password never leaves this server. The request is posted unchanged; the
# answer to Ergo is always a fresh object built from validated values, never
# the API's own body. Never log the request, the token or the response: they
# carry credentials.
set -u
umask 077

# The "error" text is a fixed message chosen here, which Ergo logs. It never
# carries request or response data. Ergo treats an error as a refusal.
refuse() {
    if [ "$#" -gt 0 ]; then
        printf '{"success":false,"error":"subspace-auth: %s"}\n' "$1"
    else
        printf '{"success":false}\n'
    fi
    exit 0
}

# Ergo keeps stdin open until the script exits, so read exactly one line
# rather than waiting for the end of input.
IFS= read -r request || refuse 'no request on stdin'

prefix=${IRC_AUTH_PASSPHRASE_PREFIX:-sirc1.}
cert_logins=${IRC_AUTH_CERT_LOGINS:-false}
# A JSON array of account names that only sign in locally: the seeded
# accounts, channel founders, the operator and IRC_AUTH_LOCAL_ACCOUNTS. The
# entrypoint always lists the operator, so a missing, empty or malformed list
# is an error: nothing is forwarded without it. Read it once and use that
# snapshot for both checks.
names='[]'
names_valid=false
if [ -n "${IRC_AUTH_LOCAL_LIST:-}" ] && list=$(cat "$IRC_AUTH_LOCAL_LIST" 2>/dev/null) &&
    printf '%s' "$list" | jq -se '
        length == 1 and (.[0] | type == "array" and length > 0
            and all(.[]; type == "string" and test("\\A[!-?A-~]{1,64}\\z")))' >/dev/null 2>&1; then
    names=$(printf '%s' "$list" | jq -c 'map(ascii_downcase)')
    names_valid=true
fi

# Ergo's ASCII case mapping folds A-Z only, as ascii_downcase does. A device
# suffix (name@device) never changes the account.
decision=$(printf '%s\n' "$request" | jq -r --argjson names "$names" --arg prefix "$prefix" --arg certs "$cert_logins" '
    def text($key): .[$key] | type == "string" and . != "";
    if type != "object" then "local"
    elif text("accountName")
        and ((.accountName | sub("@.*"; "") | ascii_downcase) as $name | any($names[]; . == $name))
    then "local"
    elif text("passphrase") then
        if .passphrase | startswith($prefix) then "api" else "local" end
    elif text("certfp") and $certs == "true" then "api"
    else "local" end' 2>/dev/null) || refuse 'Ergo sent an invalid request'
[ "$decision" = api ] || refuse
[ "$names_valid" = true ] || refuse 'the local-only account list is missing or invalid'

# Admit at most IRC_AUTH_API_CONCURRENCY API-bound runs at once, without
# waiting: Ergo takes one of its shared script slots for every login, local
# ones included, and has no acquire timeout, so a slow API must never hold
# them all. Each run holds one lock file for its lifetime; when none is free
# the login is refused at once and Ergo checks its local accounts.
slots=${IRC_AUTH_SLOTS_DIR:-}
limit=${IRC_AUTH_API_CONCURRENCY:-0}
case $limit in '' | *[!0-9]*) limit=0 ;; esac
[ -n "$slots" ] && [ -d "$slots" ] && [ -w "$slots" ] && [ "$limit" -gt 0 ] || refuse 'the API concurrency slots are not set up'
slot=1
while :; do
    exec 8>"$slots/$slot" || refuse 'cannot open an API concurrency slot'
    flock -n 8 && break
    exec 8>&-
    slot=$((slot + 1))
    [ "$slot" -le "$limit" ] || refuse 'API concurrency limit reached'
done

url=${IRC_AUTH_API_URL:-}
[ -n "$url" ] || refuse 'IRC_AUTH_API_URL is not set'
if [ -n "${IRC_AUTH_API_TOKEN_FILE:-}" ]; then
    token=$(cat "$IRC_AUTH_API_TOKEN_FILE" 2>/dev/null) || refuse 'cannot read the hook token file'
else
    token=${IRC_AUTH_API_TOKEN:-}
fi
case $token in
    '' | *[!A-Za-z0-9._~+/=-]*) refuse 'the hook token is missing or invalid' ;;
esac

protocols='=https'
if [ "${IRC_AUTH_API_ALLOW_HTTP:-false}" = true ]; then
    protocols='=http,https'
fi
set --
if [ -n "${IRC_AUTH_API_CA_FILE:-}" ]; then
    set -- --cacert "$IRC_AUTH_API_CA_FILE"
fi

# The token reaches curl through a here-document on descriptor 3 and the
# request through stdin, keeping both out of the process list. No retries,
# and short timeouts: every login holds one of Ergo's script slots, local
# ones included, while this runs.
response=$(printf '%s\n' "$request" |
    curl -q --silent --noproxy '*' --proto "$protocols" \
        --connect-timeout 2 --max-time 4 --max-filesize 65536 \
        "$@" \
        -H @/dev/fd/3 -H 'Content-Type: application/json' -H 'Accept: application/json' \
        --data-binary @- --write-out '\n%{http_code}' \
        "$url/api/v1/irc/authenticate" 2>/dev/null 3<<EOF
Authorization: Bearer $token
EOF
) || refuse "Subspace API request failed (curl exit $?)"
unset token

newline='
'
status=${response##*"$newline"}
body=${response%"$newline"*}
[ "$status" = 200 ] || refuse "Subspace API answered HTTP $status"

# Exactly one JSON object with a boolean success and no repeated keys, whether
# identical or differing only in case, at any depth: jq keeps the last of two
# identical keys and Ergo's decoder matches keys without regard to case, so
# Python's parser checks every key pair. A success must name a well-formed
# account that is not reserved or local-only.
verdict=$(printf '%s' "$body" | NAMES=$names python3 -I -c '
import json, os, re, sys

def unique_keys(pairs):
    seen = set()
    for key, _ in pairs:
        if key.casefold() in seen:
            raise ValueError("repeated key")
        seen.add(key.casefold())
    return dict(pairs)

try:
    answer = json.loads(sys.stdin.read(), object_pairs_hook=unique_keys)
except ValueError:
    sys.exit("invalid")
if not isinstance(answer, dict) or not isinstance(answer.get("success"), bool):
    sys.exit("invalid")
if not answer["success"]:
    print("refuse")
    sys.exit()
name = answer.get("accountName")
if (not isinstance(name, str)
        or not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]{0,31}", name, re.ASCII)
        or re.fullmatch(r"(?:(?:chan|nick|hist|host|memo|bot|oper)serv|global|guest-.*)",
                        name.lower(), re.ASCII)):
    sys.exit("invalid")
print("local" if name.lower() in json.loads(os.environ["NAMES"]) else "accept " + name)
' 2>/dev/null) || refuse 'Subspace API answered with an invalid body'

case $verdict in
    refuse) refuse ;;
    local) refuse 'Subspace API named a local-only account' ;;
    'accept '*) printf '{"success":true,"accountName":"%s"}\n' "${verdict#accept }" ;;
    *) refuse 'Subspace API answered with an invalid body' ;;
esac
