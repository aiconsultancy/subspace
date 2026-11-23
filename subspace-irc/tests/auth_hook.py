"""Exercise sign-in through the auth hook against a stub API, using the real image."""

import base64
import hashlib
import json
import os
import secrets
import ssl
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, str(Path(__file__).resolve().parent))
from smoke import connect  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
OPERATOR = "hook-operator"
AGENT = "hook-agent"
API_URL = "https://auth-stub:8443"
TOKEN_PATH = "/run/secrets/irc-auth-token"
CA_PATH = "/run/secrets/irc-auth-ca.pem"
FAIL_CLOSED = {
    # passphrase: presented account name (each must stay uncreated)
    "sirc1.status500": "hook-status",
    "sirc1.redirect": "hook-redirect",
    "sirc1.text": "hook-text",
    "sirc1.twice": "hook-twice",
    "sirc1.string": "hook-string",
    "sirc1.array": "hook-array",
    "sirc1.nameless": "hook-nameless",
    "sirc1.operator": "hook-mapper",
    "sirc1.hang": "hook-hang",
    "sirc1.dupcase": "hook-dupcase-user",
    "sirc1.echo": "hook-echo",
    "sirc1.short": "hook-short-user",
    "sirc1.huge": "hook-huge-user",
    "sirc1.seeded": "hook-seed-mapper",
    "sirc1.handmade": "hook-hand-mapper",
    "sirc1.badname": "hook-badname",
    "sirc1.reserved": "hook-reserved",
    "sirc1.dupsame": "hook-dupsame-user",
    "sirc1.longs": "hook-longs-user",
}
# Names that refused answers carried; none may become an account.
CREATED_NEVER = ("hook-status", "hook-redirect", "hook-twice", "hook-string", "hook-array",
                 "hook-dupcase", "hook-short", "hook-huge", "hook-dupsame", "hook-longs")
FLOOD = 300


def sasl(port, account, password, context=None):
    """Sign in with SASL PLAIN, or EXTERNAL when password is None over TLS.

    Return the irc session and the 903/904 line.
    """
    nick = account.split("@")[0]
    irc = connect(port, context)
    irc.connection.settimeout(15)
    irc.send("CAP LS 302", f"NICK {nick}", f"USER {nick} 0 * :Test account")
    irc.until(lambda line: " CAP * LS " in line and " CAP * LS * " not in line, "end of CAP LS")
    irc.send("CAP REQ :sasl")
    irc.expect(" ACK ")
    irc.send("AUTHENTICATE " + ("EXTERNAL" if password is None else "PLAIN"))
    irc.expect("AUTHENTICATE +")
    if password is None:
        irc.send("AUTHENTICATE +")
    else:
        irc.send("AUTHENTICATE " + base64.b64encode(f"\0{account}\0{password}".encode()).decode())
    result = irc.until(lambda line: " 903 " in line or " 904 " in line, "a SASL result")
    return irc, result


def signs_in(port, account, password, expected_account, context=None):
    """Sign in and finish registration; the caller closes the session."""
    irc, result = sasl(port, account, password, context)
    try:
        assert " 903 " in result, f"{account} should sign in"
        irc.send("CAP END")
        irc.expect(f" 001 {expected_account} ")
        irc.expect(" 376 ")
    except BaseException:
        irc.__exit__()
        raise
    return irc


def refused(port, account, password):
    irc, result = sasl(port, account, password)
    with irc:
        assert " 904 " in result, f"{account} should be refused"


def smoke():
    project = "subspace-irc-hook-" + uuid.uuid4().hex[:10]
    image = f"{project}-irc"
    base = ["docker", "compose", "--env-file", "/dev/null", "-f", str(ROOT / "compose.yaml"), "-p", project]
    operator_pw = secrets.token_urlsafe(24)
    agent_pw = secrets.token_urlsafe(24)
    hook_token = secrets.token_urlsafe(32)
    env = {key: value for key, value in os.environ.items() if not key.startswith("IRC_")}
    env.update(
        IRC_OPER_NAME=OPERATOR,
        IRC_OPER_PASSWORD=operator_pw,
        IRC_ACCOUNTS_JSON=json.dumps([{"name": AGENT, "password": agent_pw}]),
        IRC_BCRYPT_COST="4",
        IRC_AUTH_API_URL=API_URL + "/",
        IRC_AUTH_API_TOKEN_FILE=TOKEN_PATH,
        IRC_AUTH_API_CA_FILE=CA_PATH,
        IRC_AUTH_LOCAL_ACCOUNTS="hook-handmade, Hook-Other-Local",
        IRC_PLAIN_PORT="0",
        IRC_TLS_PORT="0",
        IRC_WS_PORT="0",
    )
    secrets_in_logs = (operator_pw, agent_pw, hook_token, "sirc1.")

    with TemporaryDirectory(dir=ROOT, prefix=".auth-hook-") as directory:
        directory = Path(directory)
        override = directory / "compose.json"
        override.write_text("{}", encoding="utf-8")
        command = base + ["-f", str(override)]

        def compose(*args, check=True, capture=False, overrides=None):
            return subprocess.run(
                command + list(args), env={**env, **(overrides or {})},
                timeout=180, check=check, capture_output=capture, text=True,
            )

        def port(container_port=6667):
            address = compose("port", "irc", str(container_port), capture=True).stdout.strip()
            host, host_port = address.rsplit(":", 1)
            assert host == "127.0.0.1", "test service must be on loopback"
            return int(host_port)

        def irc_exec(*args, check=True):
            return compose("exec", "-T", "irc", *args, check=check, capture=True)

        def logs(service):
            return compose("logs", "--no-log-prefix", service, capture=True).stdout

        def stub_requests():
            return [line.split(" ", 1)[1] for line in logs("auth-stub").splitlines()
                    if line.startswith("request ")]

        def account_exists(port_number, name):
            with signs_in(port_number, AGENT, agent_pw, AGENT) as agent:
                agent.send(f"PRIVMSG NickServ :INFO {name}")
                line = agent.until(
                    lambda text: f"Account: {name}" in text or "not registered" in text
                    or "does not exist" in text, f"NickServ INFO for {name}",
                )
                return f"Account: {name}" in line

        def fails_to_start(message, **overrides):
            result = compose("run", "--rm", "--no-deps", "-T", "irc",
                             check=False, capture=True, overrides=overrides)
            output = result.stdout + result.stderr
            assert result.returncode != 0 and message in output, message
            assert not any(value in output for value in secrets_in_logs), "credential in startup logs"

        try:
            subprocess.run(base + ["build"], env=env, timeout=600, check=True)

            # Self-signed certificates made with the image's openssl: one acts as the
            # stub's private CA and server certificate, the other as an IRC client's.
            def certificate(subject, extension):
                pem = subprocess.run(
                    ["docker", "run", "--rm", "--entrypoint", "sh", image, "-c",
                     "cd /tmp && openssl req -x509 -newkey rsa:2048 -noenc -days 1 -sha256 "
                     f"-subj /CN={subject} -addext {extension} "
                     "-keyout key.pem -out cert.pem 2>/dev/null && cat cert.pem key.pem"],
                    timeout=60, check=True, capture_output=True, text=True,
                ).stdout
                marker = "-----END CERTIFICATE-----\n"
                cert, key = pem.split(marker, 1)
                return cert + marker, key

            cert, key = certificate("auth-stub", "subjectAltName=DNS:auth-stub")
            client_cert, client_key = certificate("hook-client", "extendedKeyUsage=clientAuth")
            files = {
                "cert.pem": cert, "key.pem": key, "token": hook_token + "\n",
                "client.pem": client_cert + client_key,
            }
            for name, content in files.items():
                (directory / name).write_text(content, encoding="utf-8")
                (directory / name).chmod(0o644)

            def bind(source, target):
                return {"type": "bind", "source": str(source), "target": target, "read_only": True}

            override.write_text(json.dumps({"services": {
                "irc": {
                    "volumes": [bind(directory / "token", TOKEN_PATH), bind(directory / "cert.pem", CA_PATH)],
                    "depends_on": {"auth-stub": {"condition": "service_healthy"}},
                },
                "auth-stub": {
                    "image": image,
                    "pull_policy": "never",
                    "entrypoint": ["python3", "-I", "/stub/auth_stub.py", OPERATOR],
                    "volumes": [
                        bind(ROOT / "tests" / "auth_stub.py", "/stub/auth_stub.py"),
                        bind(directory / "cert.pem", "/stub/cert.pem"),
                        bind(directory / "key.pem", "/stub/key.pem"),
                        bind(directory / "token", "/stub/token"),
                    ],
                    "healthcheck": {
                        "test": ["CMD", "python3", "-c",
                                 "import socket; socket.create_connection(('127.0.0.1', 8443), 2).close()"],
                        "interval": "1s", "timeout": "3s", "retries": 30,
                    },
                },
            }}), encoding="utf-8")

            compose("up", "--wait", "--wait-timeout", "60")
            plain = port()
            config = irc_exec("cat", "/run/subspace-irc/ircd.yaml").stdout
            assert "    auth-script:\n        enabled: true\n" in config
            for setting in ('command: "/ircd-bin/subspace-auth"', "autocreate: true", "timeout: 9s",
                            "kill-timeout: 1s", "max-concurrency: 256"):
                assert setting in config, setting
            assert "allow-environment-overrides: false" in config
            assert "registration:\n        enabled: false" in config
            assert "require-sasl:\n        enabled: true" in config
            assert "force-nick-equals-account: true" in config
            assert irc_exec("stat", "-c", "%a %u", "/run/subspace-irc/auth-slots").stdout.split() == ["700", "10001"]
            for path in ("/run/subspace-irc/auth-token", "/run/subspace-irc/local-accounts.json"):
                mode = irc_exec("stat", "-c", "%a %u", path).stdout.split()
                assert mode == ["600", "10001"], (path, mode)
            assert irc_exec("stat", "-c", "%a %u", "/run/subspace-irc").stdout.split() == ["700", "10001"]
            local_list = json.loads(irc_exec("cat", "/run/subspace-irc/local-accounts.json").stdout)
            assert local_list == sorted([AGENT, OPERATOR, "hook-handmade", "hook-other-local"]), local_list
            assert not stub_requests(), "the bootstrap server must not call the hook"

            # Accepted: signed in under the name the API returned, account created without a password.
            with signs_in(plain, "hook-presented@laptop", "sirc1.valid", "hook-member") as member:
                member.send("PRIVMSG NickServ :PASSWD sirc1.valid replacement-pw replacement-pw")
                member.expect("managed externally")
            assert account_exists(plain, "hook-member")
            # Extra fields are ignored; Ergo gets a fresh object with the validated name.
            signs_in(plain, "hook-extra-user", "sirc1.extra", "hook-extra").__exit__()
            # The same credential for another presented name is refused by the stub.
            refused(plain, "hook-other", "sirc1.valid")
            refused(plain, "hook-refused", "sirc1.refuse")
            for passphrase, name in FAIL_CLOSED.items():
                refused(plain, name, passphrase)
            for name in ["hook-refused", "hook-other", *FAIL_CLOSED.values(), *CREATED_NEVER]:
                assert not account_exists(plain, name), f"{name} must not be created"

            # Local logins never reach the API: seeded accounts, the operator even with a
            # prefixed passphrase, listed hand-made accounts and unprefixed passwords.
            signs_in(plain, OPERATOR, operator_pw, OPERATOR).__exit__()
            refused(plain, OPERATOR, "sirc1.valid")
            refused(plain, "HOOK-HANDMADE", "sirc1.valid")
            refused(plain, "hook-other-local@phone", "sirc1.valid")
            refused(plain, "hook-local-only", "plain-local-password")

            # Certificate logins stay local while IRC_AUTH_CERT_LOGINS is off.
            server_cert = irc_exec("openssl", "x509", "-in", "/ircd/local-tls.pem").stdout
            client_context = ssl.create_default_context(cadata=server_cert)
            client_context.load_cert_chain(directory / "client.pem")
            fingerprint = hashlib.sha256(ssl.PEM_cert_to_DER_cert(client_cert)).hexdigest()
            with signs_in(plain, AGENT, agent_pw, AGENT) as agent:
                agent.send(f"PRIVMSG NickServ :CERT ADD {fingerprint}")
                agent.expect("successfully added")
            signs_in(port(6697), "hook-cert-nick", None, AGENT, client_context).__exit__()

            requests = stub_requests()
            assert "hook-presented" in requests and "hook-extra-user" in requests, requests
            for name in (AGENT, OPERATOR, "HOOK-HANDMADE", "hook-other-local", "hook-local-only", "certfp"):
                assert name not in requests, f"local login for {name} reached the API"

            irc_logs = logs("irc")
            for reason in ("answered HTTP 500", "answered HTTP 302", "request failed (curl exit 28)",
                           "request failed (curl exit 63)",
                           "answered with an invalid body", "named a local-only account"):
                assert f"subspace-auth: Subspace API {reason}" in irc_logs, (
                    reason, sorted({line.split("subspace-auth: ")[1] for line in irc_logs.splitlines()
                                    if "subspace-auth: " in line}))
            # A body shorter than its Content-Length: a partial file (18), or a receive
            # error (56) when the TLS stream ends without a close_notify.
            assert any(f"request failed (curl exit {code})" in irc_logs for code in (18, 56))
            for text in (*secrets_in_logs, "INJECTED", "API text never logged"):
                assert text not in irc_logs, "API or credential text in Ergo logs"
            print("Passed hook sign-in, fresh answers, local-only accounts and fail-closed answers")

            # The local-only list fails closed: unset, empty, null, [] or truncated
            # refuses every login that would be forwarded, without calling the API.
            def run_hook(name, local_list):
                options = ["-e", f"IRC_AUTH_API_URL={API_URL}", "-e", f"IRC_AUTH_API_CA_FILE={CA_PATH}",
                           "-e", "IRC_AUTH_API_TOKEN_FILE=/run/subspace-irc/auth-token",
                           "-e", "IRC_AUTH_SLOTS_DIR=/run/subspace-irc/auth-slots",
                           "-e", "IRC_AUTH_API_CONCURRENCY=4", "-e", f"LIST={local_list or ''}"]
                if local_list is not None:
                    options += ["-e", "IRC_AUTH_LOCAL_LIST=/tmp/local-list.json"]
                request = json.dumps({"accountName": name, "passphrase": "sirc1.unit", "ip": "192.0.2.1"})
                return subprocess.run(
                    command + ["exec", "-T", *options, "irc", "sh", "-c",
                               'printf %s "$LIST" > /tmp/local-list.json && exec /ircd-bin/subspace-auth'],
                    input=request + "\n", env=env, timeout=30, check=True, capture_output=True, text=True,
                ).stdout

            invalid = '{"success":false,"error":"subspace-auth: the local-only account list is missing or invalid"}\n'
            for index, local_list in enumerate((None, "", "null", "[]", '["hook-agent"', '["a"] ["b"]',
                                                '["hook@device"]')):
                assert run_hook(f"hook-list-{index}", local_list) == invalid, local_list
            assert run_hook("hook-list-ok", '["hook-agent"]') == '{"success":false}\n'
            assert run_hook("hook-agent", '["hook-agent"]') == '{"success":false}\n'
            requests = stub_requests()
            assert "hook-list-ok" in requests and "hook-agent" not in requests, requests
            assert not any(name.startswith("hook-list-") and name != "hook-list-ok" for name in requests)
            print("Passed the fail-closed local-only list")

            # Saturation: more concurrent API-bound logins than Ergo's 256 script slots
            # against a hanging API. The script admits at most 128 and refuses the rest
            # at once, so the operator's password login still finds a slot.
            results = {}

            def flood_login(index):
                try:
                    irc, result = sasl(plain, f"hook-flood-{index}", "sirc1.hang")
                    irc.__exit__()
                    results[index] = (" 904 " in result, time.monotonic())
                except BaseException as error:  # recorded and asserted below
                    results[index] = (error, time.monotonic())

            flood = [threading.Thread(target=flood_login, args=(i,)) for i in range(FLOOD)]
            for thread in flood:
                thread.start()
            deadline = time.monotonic() + 30
            while "subspace-auth: API concurrency limit reached" not in logs("irc"):
                assert time.monotonic() < deadline, "the flood never reached the concurrency limit"
                time.sleep(0.2)
            started = time.monotonic()
            signs_in(plain, OPERATOR, operator_pw, OPERATOR).__exit__()
            finished = time.monotonic()
            done_before = len(results)
            for thread in flood:
                thread.join(60)
            assert len(results) == FLOOD, f"{FLOOD - len(results)} flood logins never finished"
            failures = {i: r for i, (r, _) in results.items() if r is not True}
            assert not failures, f"flood logins that were not refused: {failures}"
            forwarded = sum(name.startswith("hook-flood-") for name in stub_requests())
            assert 0 < forwarded < FLOOD, forwarded
            # Relative, not absolute, timing: an admitted flood login holds Ergo's slot
            # for curl's four-second limit, so a queued operator login would take at
            # least that long, and would finish only after flood logins released slots.
            assert finished - started < 4, f"operator login took {finished - started:.1f}s"
            assert done_before < FLOOD, "the flood had ended before the operator signed in"
            print(f"Passed operator login in {finished - started:.2f}s under {FLOOD} API-bound logins "
                  f"({forwarded} reached the API, the rest refused at once)")

            # API down: local accounts still sign in; API credentials do not.
            compose("stop", "auth-stub")
            signs_in(plain, AGENT, agent_pw, AGENT).__exit__()
            signs_in(plain, OPERATOR, operator_pw, OPERATOR).__exit__()
            refused(plain, "hook-presented", "sirc1.valid")

            # TLS is verified: without the private CA the stub's certificate is refused.
            compose("up", "--wait", "--wait-timeout", "60", "--force-recreate",
                    overrides={"IRC_AUTH_API_CA_FILE": ""})
            refused(port(), "hook-presented", "sirc1.valid")
            assert "request failed (curl exit 60)" in logs("irc")
            print("Passed API outage fallback and TLS verification")

            # A custom prefix, and certificate logins forwarded when enabled. The API's
            # answer then overrides the local NickServ CERT mapping.
            compose("up", "--wait", "--wait-timeout", "60", "--force-recreate",
                    overrides={"IRC_AUTH_PASSPHRASE_PREFIX": "custom-", "IRC_AUTH_CERT_LOGINS": "true"})
            signs_in(port(), "hook-custom-user", "custom-valid", "hook-custom").__exit__()
            refused(port(), "hook-sirc-user", "sirc1.valid")
            signs_in(port(6697), "hook-cert-nick", None, "hook-cert", client_context).__exit__()
            assert account_exists(port(), "hook-cert")
            requests = stub_requests()
            assert "hook-custom-user" in requests and "certfp" in requests, requests
            assert "hook-sirc-user" not in requests, requests
            print("Passed the passphrase prefix and certificate forwarding")

            compose("stop", "irc")
            fails_to_start("IRC_AUTH_API_URL needs the hook token", IRC_AUTH_API_TOKEN_FILE="")
            fails_to_start("Set only one of IRC_AUTH_API_TOKEN and IRC_AUTH_API_TOKEN_FILE",
                           IRC_AUTH_API_TOKEN=hook_token)
            fails_to_start("The hook token must be", IRC_AUTH_API_TOKEN_FILE="",
                           IRC_AUTH_API_TOKEN="too-short-for-the-api-0123456")
            fails_to_start("Cannot read IRC_AUTH_API_TOKEN_FILE", IRC_AUTH_API_TOKEN_FILE="/run/missing")
            for url in ("ftp://auth-stub", "https://user:pw@auth-stub", "https://auth-stub/?q=1", "auth-stub"):
                fails_to_start("IRC_AUTH_API_URL must be an http or https URL", IRC_AUTH_API_URL=url)
            fails_to_start("must use https unless IRC_AUTH_API_ALLOW_HTTP=true", IRC_AUTH_API_URL="http://auth-stub")
            # With plain HTTP allowed, the URL passes and start-up reaches the next check.
            fails_to_start("IRC_AUTH_PASSPHRASE_PREFIX must be", IRC_AUTH_API_URL="http://auth-stub",
                           IRC_AUTH_API_ALLOW_HTTP="true", IRC_AUTH_PASSPHRASE_PREFIX="has space")
            fails_to_start("must be true or false", IRC_AUTH_CERT_LOGINS="yes")
            fails_to_start("names must not contain @", IRC_AUTH_LOCAL_ACCOUNTS="hook-handmade@laptop")
            fails_to_start("IRC_AUTH_API_CONCURRENCY must be an integer from 1 to 192",
                           IRC_AUTH_API_CONCURRENCY="193")
            fails_to_start("IRC_AUTH_API_CA_FILE must be readable", IRC_AUTH_API_CA_FILE="/run/missing.pem")
            fails_to_start("IRC_AUTH_PASSPHRASE_PREFIX must be", IRC_AUTH_PASSPHRASE_PREFIX="tab\tprefix")
            fails_to_start("Set only one of IRC_AUTH_LOCAL_ACCOUNTS and IRC_AUTH_LOCAL_ACCOUNTS_FILE",
                           IRC_AUTH_LOCAL_ACCOUNTS_FILE=TOKEN_PATH)
            fails_to_start("Seeded passwords must not start with IRC_AUTH_PASSPHRASE_PREFIX",
                           IRC_ACCOUNTS_JSON=json.dumps([{"name": AGENT, "password": "sirc1.seeded-pw"}]))
            fails_to_start("need IRC_AUTH_API_URL", IRC_AUTH_API_URL="")
            fails_to_start("need IRC_AUTH_API_URL", IRC_AUTH_API_URL="", IRC_AUTH_API_TOKEN_FILE="",
                           IRC_AUTH_API_CA_FILE="", IRC_AUTH_LOCAL_ACCOUNTS="", IRC_AUTH_CERT_LOGINS="true")
            print("Passed hook start-up validation")
        except BaseException:
            compose("logs", "--tail", "60", check=False)
            raise
        finally:
            compose("down", "--volumes", "--remove-orphans", "--rmi", "local", check=False)


if __name__ == "__main__":
    smoke()
