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
# A JSON array of lower-case account names that only sign in locally: the
# seeded accounts, channel founders, the operator and IRC_AUTH_LOCAL_ACCOUNTS.
local_list=${IRC_AUTH_LOCAL_LIST:-/dev/null}
[ -r "$local_list" ] || refuse 'cannot read the local-only account list'

# Ergo's ASCII case mapping folds A-Z only, as ascii_downcase does. A device
# suffix (name@device) never changes the account.
decision=$(printf '%s\n' "$request" | jq -r --slurpfile local "$local_list" --arg prefix "$prefix" --arg certs "$cert_logins" '
    def text($key): .[$key] | type == "string" and . != "";
    ($local[0] // []) as $names
    | if type != "object" then "local"
      elif text("accountName")
          and ((.accountName | sub("@.*"; "") | ascii_downcase) as $name | any($names[]; . == $name))
      then "local"
      elif text("passphrase") then
          if .passphrase | startswith($prefix) then "api" else "local" end
      elif text("certfp") and $certs == "true" then "api"
      else "local" end' 2>/dev/null) || refuse 'Ergo sent an invalid request'
[ "$decision" = api ] || refuse

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

# Exactly one JSON object with a boolean success and no keys that differ only
# in case (Ergo's decoder matches keys without regard to case). A success
# must name a well-formed account that is not reserved or local-only.
verdict=$(printf '%s' "$body" | jq -sr --slurpfile local "$local_list" '
    ($local[0] // []) as $names
    | if length == 1 and (.[0] | type) == "object" then .[0] else error("invalid") end
    | ([keys[] | ascii_downcase]) as $folded
    | if ($folded | length) != ($folded | unique | length) then error("ambiguous") else . end
    | if (.success | type) != "boolean" then error("invalid")
      elif .success | not then "refuse"
      elif (.accountName | type) != "string"
          or (.accountName | test("\\A[A-Za-z][A-Za-z0-9_-]{0,31}\\z") | not)
          or (.accountName | ascii_downcase
              | test("\\A(?:(?:chan|nick|hist|host|memo|bot|oper)serv|global|guest-.*)\\z"))
      then error("invalid")
      elif (.accountName | ascii_downcase) as $name | any($names[]; . == $name) then "local"
      else "accept " + .accountName end' 2>/dev/null) ||
    refuse 'Subspace API answered with an invalid body'

case $verdict in
    refuse) refuse ;;
    local) refuse 'Subspace API named a local-only account' ;;
    'accept '*) printf '{"success":true,"accountName":"%s"}\n' "${verdict#accept }" ;;
    *) refuse 'Subspace API answered with an invalid body' ;;
esac
