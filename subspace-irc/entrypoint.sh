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
history_database=${IRC_HISTORY_DATABASE:-sqlite}

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

# Persistent history: SQLite in the data volume (default), or a PostgreSQL
# database, for example beside the Subspace API's on the same host.
case $history_database in
    sqlite)
        history_block=$(printf '    sqlite:\n        enabled: true\n        database-path: /ircd/history.db\n        busy-timeout: 5s\n        max-conns: 1')
        ;;
    postgresql)
        pg_host=${IRC_POSTGRES_HOST:-}
        pg_port=${IRC_POSTGRES_PORT:-5432}
        pg_user=${IRC_POSTGRES_USER:-ergo}
        pg_database=${IRC_POSTGRES_DATABASE:-ergo_history}
        pg_ssl=${IRC_POSTGRES_SSL_MODE:-disable}
        case $pg_host in
            ''|*[!A-Za-z0-9.-]*) die 'IRC_POSTGRES_HOST must be a host name or IP address' ;;
        esac
        case $pg_port in
            ''|*[!0-9]*) die 'IRC_POSTGRES_PORT must be a number' ;;
        esac
        for name in "$pg_user" "$pg_database"; do
            case $name in
                ''|*[!A-Za-z0-9_]*) die 'IRC_POSTGRES_USER and IRC_POSTGRES_DATABASE use letters, digits and underscores' ;;
            esac
        done
        case $pg_ssl in
            disable|require|verify-ca|verify-full) ;;
            *) die 'IRC_POSTGRES_SSL_MODE must be disable, require, verify-ca or verify-full' ;;
        esac
        case "${IRC_POSTGRES_PASSWORD_FILE:+file}${IRC_POSTGRES_PASSWORD:+env}" in
            file) pg_password=$(cat "$IRC_POSTGRES_PASSWORD_FILE") || die 'Cannot read IRC_POSTGRES_PASSWORD_FILE' ;;
            env) pg_password=$IRC_POSTGRES_PASSWORD ;;
            fileenv) die 'Set only one of IRC_POSTGRES_PASSWORD and IRC_POSTGRES_PASSWORD_FILE' ;;
            '') die 'PostgreSQL history needs IRC_POSTGRES_PASSWORD_FILE or IRC_POSTGRES_PASSWORD' ;;
        esac
        quoted_pg_password=$(printf '%s' "$pg_password" | jq -Rsc .)
        unset pg_password IRC_POSTGRES_PASSWORD
        history_block=$(printf '    postgresql:\n        enabled: true\n        host: "%s"\n        port: %s\n        ssl-mode: "%s"\n        user: "%s"\n        password: %s\n        history-database: "%s"\n        timeout: 3s\n        max-conns: 4\n        connect-timeout: 10s\n        application-name: "subspace-irc"' \
            "$pg_host" "$pg_port" "$pg_ssl" "$pg_user" "$quoted_pg_password" "$pg_database")
        ;;
    *) die 'IRC_HISTORY_DATABASE must be sqlite or postgresql' ;;
esac

# Sign-in through the Subspace API (Ergo's auth-script). Off unless
# IRC_AUTH_API_URL is set. The hook token and the local-only account list are
# written to the private run directory, which only UID 10001 can read, and
# reach the script by path.
auth_url=${IRC_AUTH_API_URL:-}
auth_ca=${IRC_AUTH_API_CA_FILE:-}
auth_prefix=${IRC_AUTH_PASSPHRASE_PREFIX:-}
auth_http=${IRC_AUTH_API_ALLOW_HTTP:-false}
auth_certs=${IRC_AUTH_CERT_LOGINS:-false}
auth_limit=${IRC_AUTH_API_CONCURRENCY:-}
auth_slots=$run/auth-slots
auth_token=$run/auth-token
auth_local=$run/local-accounts.json
auth_block=
rm -f "$auth_token" "$auth_token.tmp" "$auth_local" "$auth_local.tmp"
rm -rf "$auth_slots"
for setting in "$auth_http" "$auth_certs"; do
    case $setting in
        true|false) ;;
        *) die 'IRC_AUTH_API_ALLOW_HTTP and IRC_AUTH_CERT_LOGINS must be true or false' ;;
    esac
done
case "${IRC_AUTH_LOCAL_ACCOUNTS_FILE:+file}${IRC_AUTH_LOCAL_ACCOUNTS:+env}" in
    file) local_names=$(cat "$IRC_AUTH_LOCAL_ACCOUNTS_FILE") || die 'Cannot read IRC_AUTH_LOCAL_ACCOUNTS_FILE' ;;
    env) local_names=$IRC_AUTH_LOCAL_ACCOUNTS ;;
    fileenv) die 'Set only one of IRC_AUTH_LOCAL_ACCOUNTS and IRC_AUTH_LOCAL_ACCOUNTS_FILE' ;;
    '') local_names= ;;
esac
if [ -n "$auth_url" ]; then
    case $auth_url in
        https://*) ;;
        http://*) [ "$auth_http" = true ] ||
            die 'IRC_AUTH_API_URL must use https unless IRC_AUTH_API_ALLOW_HTTP=true' ;;
    esac
    printf '%s' "$auth_url" |
        jq -Rse 'test("\\Ahttps?://[A-Za-z0-9.-]+(:[0-9]{1,5})?(/[A-Za-z0-9._~-]+)*/?\\z")' >/dev/null ||
        die 'IRC_AUTH_API_URL must be an http or https URL with a host name and optional port and path, and no credentials, query or fragment'
    auth_url=${auth_url%/}
    case "${IRC_AUTH_API_TOKEN_FILE:+file}${IRC_AUTH_API_TOKEN:+env}" in
        file) jq -Rsj 'sub("\\r?\\n\\z"; "")' < "$IRC_AUTH_API_TOKEN_FILE" > "$auth_token.tmp" ||
            die 'Cannot read IRC_AUTH_API_TOKEN_FILE' ;;
        env) printf '%s' "$IRC_AUTH_API_TOKEN" > "$auth_token.tmp" ;;
        fileenv) die 'Set only one of IRC_AUTH_API_TOKEN and IRC_AUTH_API_TOKEN_FILE' ;;
        '') die 'IRC_AUTH_API_URL needs the hook token in IRC_AUTH_API_TOKEN_FILE or IRC_AUTH_API_TOKEN' ;;
    esac
    jq -Rse 'test("\\A[A-Za-z0-9._~+/=-]{32,1024}\\z")' < "$auth_token.tmp" >/dev/null ||
        { rm -f "$auth_token.tmp"; die 'The hook token must be 32–1024 characters of letters, digits and . _ ~ + / = -'; }
    mv "$auth_token.tmp" "$auth_token"
    # Only passphrases with this prefix (API-issued credentials) reach the API.
    auth_prefix=${auth_prefix:-sirc1.}
    printf '%s' "$auth_prefix" | jq -Rse 'test("\\A[!-~]{1,64}\\z")' >/dev/null ||
        die 'IRC_AUTH_PASSPHRASE_PREFIX must be 1–64 printable ASCII characters without whitespace'
    if [ -n "$auth_ca" ]; then
        case "$auth_ca" in /*) ;; *) die 'IRC_AUTH_API_CA_FILE must be an absolute path' ;; esac
        [ -r "$auth_ca" ] || die 'IRC_AUTH_API_CA_FILE must be readable by UID 10001'
    fi
    # API-bound script runs are capped well below Ergo's 256 script slots, so
    # local and operator logins always find a free slot.
    auth_limit=${auth_limit:-128}
    case $auth_limit in
        [1-9] | [1-9][0-9] | 1[0-8][0-9] | 19[0-2]) ;;
        *) die 'IRC_AUTH_API_CONCURRENCY must be an integer from 1 to 192' ;;
    esac
    mkdir "$auth_slots"
    # Extra local-only accounts, separated by commas, spaces or new lines.
    case $local_names in
        *@*) die 'IRC_AUTH_LOCAL_ACCOUNTS names must not contain @: list the bare account name, without a device suffix' ;;
    esac
    local_extra=$(printf '%s' "$local_names" | jq -Rsc '
        [splits("[\\s,]+") | select(. != "")]
        | if all(.[]; test("\\A[!-~]{1,64}\\z")) then map(ascii_downcase) else error("invalid") end
    ' 2>/dev/null) ||
        die 'IRC_AUTH_LOCAL_ACCOUNTS names must be 1–64 printable ASCII characters, separated by commas or whitespace'
    # Every login holds one of Ergo's script slots while the script runs, even
    # a local one that the script refuses at once, so allow plenty of slots.
    auth_block=$(printf '    auth-script:\n        enabled: true\n        command: "/ircd-bin/subspace-auth"\n        args: []\n        autocreate: true\n        timeout: 9s\n        kill-timeout: 1s\n        max-concurrency: 256')
elif [ -n "${IRC_AUTH_API_TOKEN_FILE:-}${IRC_AUTH_API_TOKEN:-}${auth_ca}${auth_prefix}${local_names}${auth_limit}" ] ||
    [ "$auth_http" = true ] || [ "$auth_certs" = true ]; then
    die 'The IRC_AUTH_* settings need IRC_AUTH_API_URL'
fi
unset IRC_AUTH_API_TOKEN

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

if [ -n "$auth_url" ]; then
    # A seeded password with the credential prefix would be sent to the API.
    if jq -e --arg prefix "$auth_prefix" 'any(.[]; .password | startswith($prefix))' \
        "$temporary/seed" >/dev/null; then
        die 'Seeded passwords must not start with IRC_AUTH_PASSPHRASE_PREFIX'
    fi
    # Local-only accounts: never sent to the API, and never accepted from it.
    channel_seed=/dev/null
    [ ! -f "$temporary/channels" ] || channel_seed=$temporary/channels
    jq -nc --slurpfile seed "$temporary/seed" --slurpfile channels "$channel_seed" \
        --arg founder "${IRC_CHANNEL_FOUNDER:-$oper}" --argjson extra "$local_extra" '
        [$seed[0][].name] + [($channels[0] // [])[] | .founder // $founder] + [$founder] + $extra
        | map(select(type == "string" and . != "") | ascii_downcase) | unique
    ' > "$auth_local.tmp" || die 'Could not write the local-only account list'
    mv "$auth_local.tmp" "$auth_local"
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
    SERVER_NAME=$server_name LISTENERS=$1 AUTH_SCRIPT=$2 WS_ORIGINS=$origins OPERS=$opers \
        BCRYPT_COST=$bcrypt_cost HISTORY_DATABASE=$history_block awk '
        /@@LISTENERS@@/ { print ENVIRON["LISTENERS"]; next }
        /@@AUTH_SCRIPT@@/ { if (ENVIRON["AUTH_SCRIPT"] != "") print ENVIRON["AUTH_SCRIPT"]; next }
        /@@HISTORY_DATABASE@@/ { print ENVIRON["HISTORY_DATABASE"]; next }
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
    # The bootstrap server never runs the auth hook. Seeding creates and
    # verifies local accounts through check_auth, which would otherwise send
    # seed passwords to the API and could sign in to an API-chosen account.
    render_config '        "127.0.0.1:16670": {}' '' > "$temporary/bootstrap.yaml"
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

render_config "$listeners" "$auth_block" > "$run/ircd.yaml"
cleanup
trap - EXIT TERM INT
# Ergo passes its own environment to the auth script. It carries the token's
# path, never the token, and allow-environment-overrides stays off.
set -- PATH="$PATH"
if [ -n "$auth_url" ]; then
    set -- "$@" IRC_AUTH_API_URL="$auth_url" IRC_AUTH_API_TOKEN_FILE="$auth_token" \
        IRC_AUTH_PASSPHRASE_PREFIX="$auth_prefix" IRC_AUTH_LOCAL_LIST="$auth_local" \
        IRC_AUTH_CERT_LOGINS="$auth_certs" IRC_AUTH_API_ALLOW_HTTP="$auth_http" \
        IRC_AUTH_SLOTS_DIR="$auth_slots" IRC_AUTH_API_CONCURRENCY="$auth_limit"
    if [ -n "$auth_ca" ]; then
        set -- "$@" IRC_AUTH_API_CA_FILE="$auth_ca"
    fi
fi
exec env -i "$@" "$ergo" run --conf "$run/ircd.yaml"
