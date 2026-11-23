#!/bin/sh
# Ergo auth-script hook: ask the Subspace API whether a login is valid.
#
# Ergo writes one JSON line on stdin and reads one JSON line from stdout. The
# request is posted unchanged to the API; its answer is printed only when it
# is an HTTP 200 with a well-formed body. Anything else is a refusal, after
# which Ergo checks its local accounts. Never log the request, the token or
# the response: they carry credentials.
set -u
umask 077

# The "error" text is a fixed message that Ergo logs; it never carries
# request or response data. Ergo treats an error as a refusal.
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

# The break-glass operator always signs in locally: its password is never
# sent to the API, and the API cannot sign anyone in as that account.
oper=$(printf '%s' "${IRC_OPER_NAME:-}" | tr '[:upper:]' '[:lower:]')
if [ -n "$oper" ]; then
    name=$(printf '%s\n' "$request" | jq -r '
        if type == "object" and (.accountName | type) == "string"
        then .accountName | ascii_downcase else "" end' 2>/dev/null) ||
        refuse 'Ergo sent an invalid request'
    [ "$name" != "$oper" ] || refuse
fi

set --
if [ -n "${IRC_AUTH_API_CA_FILE:-}" ]; then
    set -- --cacert "$IRC_AUTH_API_CA_FILE"
fi

# The token reaches curl through a here-document on descriptor 3 and the
# request through stdin, keeping both out of the process list. No retries:
# Ergo waits for this script while the client waits for Ergo.
response=$(printf '%s\n' "$request" |
    curl -q --silent --noproxy '*' --proto '=http,https' --max-time 5 \
        --max-filesize 65536 "$@" \
        -H @/dev/fd/3 -H 'Content-Type: application/json' -H 'Accept: application/json' \
        --data-binary @- --write-out '\n%{http_code}' \
        "$url/api/v1/irc/authenticate" 2>/dev/null 3<<EOF
Authorization: Bearer $token
EOF
) || refuse 'Subspace API unreachable or timed out'
unset token

newline='
'
status=${response##*"$newline"}
body=${response%"$newline"*}
[ "$status" = 200 ] || refuse "Subspace API answered HTTP $status"

# Exactly one JSON object with a boolean success; a success must name a
# non-empty account other than the local operator.
answer=$(printf '%s' "$body" | jq -sc --arg oper "$oper" '
    if length == 1 and (.[0] | type) == "object" and (.[0].success | type) == "boolean"
    then .[0] else error("invalid") end
    | if .success then
        if (.accountName | type) == "string" and .accountName != ""
            and (.accountName | ascii_downcase) != $oper
        then . else error("invalid") end
      else . end' 2>/dev/null) || refuse 'Subspace API answered with an invalid body'
printf '%s\n' "$answer"
