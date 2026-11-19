#!/bin/sh
set -eu
umask 077

ergo=/ircd-bin/ergo
run=/run/subspace-irc
server_name=${IRC_SERVER_NAME:-irc.localhost}
plain=${IRC_ENABLE_PLAIN:-true}
websocket=${IRC_ENABLE_WS:-true}
ws_origin=${IRC_WS_ORIGIN:-}
bcrypt_cost=${IRC_BCRYPT_COST:-12}
cert=${IRC_TLS_CERT:-}
key=${IRC_TLS_KEY:-}
oper=${IRC_OPER_NAME:-}

die() { printf 'subspace-irc: %s\n' "$*" >&2; exit 1; }

for setting in "$plain" "$websocket"; do
    case $setting in
        true|false) ;;
        *) die 'IRC_ENABLE_PLAIN and IRC_ENABLE_WS must be true or false' ;;
    esac
done
case $server_name in
    *[!a-z0-9.-]*|[-.]*|*[-.]|*..*) die 'IRC_SERVER_NAME must be a lower-case DNS name' ;;
esac
case $bcrypt_cost in
    [4-9]|[12][0-9]|3[01]) ;;
    *) die 'IRC_BCRYPT_COST must be an integer from 4 to 31' ;;
esac

# Hold the same lock across provisioning and normal service. Ergo also locks
# its datastore when running; this lock covers the hand-off between processes.
exec 9>/ircd/instance.lock
flock -n 9 || die 'The IRC data volume is already in use'

temporary=$(mktemp -d "$run/start.XXXXXXXX")
bootstrap_pid=
cleanup() {
    if [ -n "$bootstrap_pid" ]; then
        kill -TERM "$bootstrap_pid" 2>/dev/null || :
        wait "$bootstrap_pid" 2>/dev/null || :
    fi
    rm -rf "$temporary"
    rm -f /ircd/.local-tls.pem.tmp
}
trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT

case "${IRC_ACCOUNTS_FILE:+file}${IRC_ACCOUNTS_JSON:+json}" in
    file) cp "$IRC_ACCOUNTS_FILE" "$temporary/accounts" ||
        die 'Cannot read IRC_ACCOUNTS_FILE' ;;
    json) printf '%s' "$IRC_ACCOUNTS_JSON" > "$temporary/accounts" ;;
    filejson) die 'Set only one of IRC_ACCOUNTS_JSON and IRC_ACCOUNTS_FILE' ;;
    '') printf '[]\n' > "$temporary/accounts" ;;
esac

case "${IRC_CHANNELS_FILE:+file}${IRC_CHANNELS_JSON:+json}" in
    file) cp "$IRC_CHANNELS_FILE" "$temporary/channels" ||
        die 'Cannot read IRC_CHANNELS_FILE' ;;
    json) printf '%s' "$IRC_CHANNELS_JSON" > "$temporary/channels" ;;
    filejson) die 'Set only one of IRC_CHANNELS_JSON and IRC_CHANNELS_FILE' ;;
esac

case "${IRC_OPER_PASSWORD_FILE:+file}${IRC_OPER_PASSWORD:+env}" in
    file) jq -Rsc --arg name "$oper" '[{name: $name, password: sub("\\r?\\n$"; "")}]' \
        < "$IRC_OPER_PASSWORD_FILE" > "$temporary/operator" ||
        die 'Cannot read IRC_OPER_PASSWORD_FILE' ;;
    env) printf '%s' "$IRC_OPER_PASSWORD" |
        jq -Rsc --arg name "$oper" '[{name: $name, password: .}]' > "$temporary/operator" ;;
    fileenv) die 'Set only one of IRC_OPER_PASSWORD and IRC_OPER_PASSWORD_FILE' ;;
    '') [ -z "$oper" ] || die 'IRC_OPER_NAME needs an operator password'
        printf '[]\n' > "$temporary/operator" ;;
esac
unset IRC_ACCOUNTS_JSON IRC_CHANNELS_JSON IRC_OPER_PASSWORD

# Check the complete seed before making any changes. Normalised names make
# duplicate detection agree with Ergo's ASCII case mapping. Passwords follow
# Ergo's passphrase rules (no whitespace, no leading colon, not "*"), so a bad
# entry cannot stop provisioning part-way through.
if ! jq -sec '
    if length != 2 or any(.[]; type != "array") then error("expected arrays") else add end
    | if all(.[];
        type == "object" and keys == ["name", "password"] and
        (.name | type == "string" and test("\\A[a-z][a-z0-9_-]{0,31}\\z")) and
        (.password | type == "string" and utf8bytelength >= 1 and utf8bytelength <= 72
            and (explode | all(. >= 32 and . != 127))
            and (test("\\A:|\\s") | not) and . != "*")
      ) and (map(.name) | length == (unique | length))
      then . else error("invalid account definition") end
' "$temporary/accounts" "$temporary/operator" > "$temporary/seed" 2>/dev/null; then
    die 'Expected distinct lower-case account names and passwords of 1–72 bytes without whitespace, control characters, a leading colon or the single value *'
fi

# Reserved nicks in Ergo v2.19.1 (irc/services.go and irc/nickname.go),
# plus the configured Guest-* pattern. The account syntax already excludes =scene=.
if jq -e 'any(.[]; .name | test("\\A(?:(?:chan|nick|hist|host|memo|bot|oper)serv|global|guest-.*)\\z"))' \
    "$temporary/seed" >/dev/null; then
    die 'Account name is reserved by Ergo'
fi

seed_channels() {
    python3 -I /ircd-bin/seed-channels.py "$1" "$temporary/channels" "$temporary/seed" \
        "$oper" "${IRC_CHANNEL_FOUNDER:-$oper}"
}
if [ -f "$temporary/channels" ]; then
    seed_channels validate
fi

if [ -z "$cert" ] && [ -z "$key" ]; then
    cert=/ircd/local-tls.pem
    key=$cert
    if [ ! -s "$cert" ] ||
        ! openssl x509 -in "$cert" -checkend 86400 -checkhost "$server_name" -noout >/dev/null 2>&1; then
        openssl req -x509 -newkey rsa:2048 -noenc -days 365 -sha256 \
            -subj "/CN=$server_name" \
            -addext "subjectAltName=DNS:$server_name,DNS:localhost,IP:127.0.0.1,IP:::1" \
            -keyout "$temporary/key" -out "$temporary/cert" 2>"$temporary/openssl-errors" ||
            die 'Could not generate the local TLS certificate'
        cat "$temporary/cert" "$temporary/key" > /ircd/.local-tls.pem.tmp
        mv /ircd/.local-tls.pem.tmp "$cert"
    fi
else
    case "$cert" in /*) ;; *) die 'IRC_TLS_CERT must be an absolute path' ;; esac
    case "$key" in /*) ;; *) die 'IRC_TLS_KEY must be an absolute path' ;; esac
    [ -r "$cert" ] && [ -r "$key" ] || die 'TLS certificate and key must be readable by UID 10001'
fi

opers='{}'
if [ -n "$oper" ]; then
    hash=$(jq -r '.[0].password' "$temporary/operator" | "$ergo" genpasswd --quiet)
    # JSON is valid YAML. The hash travels over stdin, keeping it out of argv.
    opers=$(printf '%s' "$hash" | jq -Rsc --arg name "$oper" \
        '{($name): {class: "server-admin", hidden: true, password: .}}')
    unset hash
fi

quoted_cert=$(printf '%s' "$cert" | jq -Rsc .)
quoted_key=$(printf '%s' "$key" | jq -Rsc .)
listeners=$(printf '        ":6697":\n            tls:\n                cert: %s\n                key: %s\n            min-tls-version: 1.2\n' \
    "$quoted_cert" "$quoted_key")
if [ "$plain" = true ]; then
    listeners=$(printf '%s\n        ":6667": {}' "$listeners")
fi
if [ "$websocket" = true ]; then
    listeners=$(printf '%s\n        ":8097": {websocket: true}' "$listeners")
fi
origins=$(jq -cn --arg origin "$ws_origin" '
    if $origin != "" then [$origin]
    else ["http", "https"] | map(. + "://") |
        [ .[] as $scheme | ("localhost", "127.0.0.1", "[::1]") as $host |
            ($scheme + $host), ($scheme + $host + ":*") ]
    end
')

render_config() {
    SERVER_NAME=$server_name LISTENERS=$1 WS_ORIGINS=$origins OPERS=$opers BCRYPT_COST=$bcrypt_cost awk '
        /@@LISTENERS@@/ { print ENVIRON["LISTENERS"]; next }
        /@@SERVER_NAME@@/ { sub(/@@SERVER_NAME@@/, ENVIRON["SERVER_NAME"]); print; next }
        /@@WS_ORIGINS@@/ { print "        allowed-origins: " ENVIRON["WS_ORIGINS"]; next }
        /@@BCRYPT_COST@@/ { sub(/@@BCRYPT_COST@@/, ENVIRON["BCRYPT_COST"]); print; next }
        /@@OPERS@@/ { print "opers: " ENVIRON["OPERS"]; next }
        { print }
    ' /ircd-bin/ircd.yaml.tmpl
}

bootstrap_request() {
    curl -q --noproxy '*' -fsS --max-time 8 -H "@$temporary/auth" \
        --json @- "http://127.0.0.1:8089/v1/$1"
}

check_credential() {
    printf '%s' "$1" | bootstrap_request check_auth |
        jq -e --arg name "$2" '.success and (.accountName | ascii_downcase) == $name' >/dev/null
}

provision() {
    render_config '        "127.0.0.1:16670": {}' > "$temporary/bootstrap.yaml"
    token=$("$ergo" gentoken)
    printf 'Authorization: Bearer %s\n' "$token" > "$temporary/auth"
    printf '\napi:\n    enabled: true\n    listener: "127.0.0.1:8089"\n    bearer-tokens: ["%s"]\n' \
        "$token" >> "$temporary/bootstrap.yaml"
    unset token

    env -i PATH="$PATH" "$ergo" run --conf "$temporary/bootstrap.yaml" &
    bootstrap_pid=$!
    attempts=0
    while :; do
        if status=$(printf '{}' | bootstrap_request status 2>/dev/null); then
            printf '%s' "$status" | jq -e '.success == true' >/dev/null ||
                die 'Ergo bootstrap API returned an invalid readiness response'
            break
        else
            [ "$?" -eq 7 ] || die 'Ergo bootstrap API readiness check failed'
        fi
        attempts=$((attempts + 1))
        if [ "$attempts" -ge 100 ] || ! kill -0 "$bootstrap_pid" 2>/dev/null; then
            die 'Ergo bootstrap API did not start'
        fi
        sleep 0.1
    done

    inventory=$(printf '{}' | bootstrap_request ns/list) || die 'Could not list Ergo accounts'
    printf '%s' "$inventory" | jq -e '.success and (.accounts | type == "array")' >/dev/null ||
        die 'Ergo returned an invalid account inventory'
    printf '%s' "$inventory" | jq -r '.accounts[].accountName | ascii_downcase' > "$temporary/existing"
    jq -c '.[] | {accountName: .name, passphrase: .password}' "$temporary/seed" > "$temporary/requests"

    # Verify pre-existing credentials before creating any missing accounts.
    while IFS= read -r request; do
        name=$(printf '%s' "$request" | jq -r .accountName)
        if grep -Fxq "$name" "$temporary/existing"; then
            check_credential "$request" "$name" ||
                die "Seed credential for $name does not match the existing account"
        fi
    done < "$temporary/requests"
    while IFS= read -r request; do
        name=$(printf '%s' "$request" | jq -r .accountName)
        if ! grep -Fxq "$name" "$temporary/existing"; then
            printf '%s' "$request" | bootstrap_request ns/saregister |
                jq -e '.success == true' >/dev/null || die "Could not provision account $name"
            check_credential "$request" "$name" || die "Could not verify provisioned account $name"
        fi
    done < "$temporary/requests"

    if [ -f "$temporary/channels" ]; then
        seed_channels apply
    fi

    kill -TERM "$bootstrap_pid"
    wait "$bootstrap_pid" || die 'Ergo bootstrap did not shut down cleanly'
    bootstrap_pid=
}

if jq -e 'length > 0' "$temporary/seed" >/dev/null; then
    provision
fi

render_config "$listeners" > "$run/ircd.yaml"
cleanup
trap - EXIT TERM INT
exec env -i PATH="$PATH" "$ergo" run --conf "$run/ircd.yaml"
