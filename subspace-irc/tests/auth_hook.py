"""Exercise sign-in through the auth hook against a stub API, using the real image."""

import base64
import json
import os
import secrets
import subprocess
import sys
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
}


def sasl(port, account, password, nick=None):
    """Sign in with SASL PLAIN; return the irc session and the 903/904 line."""
    nick = nick or account.split("@")[0]
    irc = connect(port)
    irc.connection.settimeout(15)
    irc.send("CAP LS 302", f"NICK {nick}", f"USER {nick} 0 * :Test account")
    irc.until(lambda line: " CAP * LS " in line and " CAP * LS * " not in line, "end of CAP LS")
    irc.send("CAP REQ :sasl")
    irc.expect(" ACK ")
    irc.send("AUTHENTICATE PLAIN")
    irc.expect("AUTHENTICATE +")
    irc.send("AUTHENTICATE " + base64.b64encode(f"\0{account}\0{password}".encode()).decode())
    result = irc.until(lambda line: " 903 " in line or " 904 " in line, "a SASL result")
    return irc, result


def signs_in(port, account, password, expected_account):
    """Sign in and finish registration; the caller closes the session."""
    irc, result = sasl(port, account, password)
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

        def port():
            address = compose("port", "irc", "6667", capture=True).stdout.strip()
            host, host_port = address.rsplit(":", 1)
            assert host == "127.0.0.1", "test service must be on loopback"
            return int(host_port)

        def irc_exec(*args, check=True):
            return compose("exec", "-T", "irc", *args, check=check, capture=True)

        def logs(service):
            return compose("logs", "--no-log-prefix", service, capture=True).stdout

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

            # A private CA and a server certificate for the stub, made with the image's openssl.
            pem = subprocess.run(
                ["docker", "run", "--rm", "--entrypoint", "sh", image, "-c",
                 "cd /tmp && openssl req -x509 -newkey rsa:2048 -noenc -days 1 -sha256 "
                 "-subj /CN=auth-stub -addext subjectAltName=DNS:auth-stub "
                 "-keyout key.pem -out cert.pem 2>/dev/null && cat cert.pem key.pem"],
                timeout=60, check=True, capture_output=True, text=True,
            ).stdout
            marker = "-----END CERTIFICATE-----\n"
            cert, key = pem.split(marker, 1)
            files = {
                "cert.pem": cert + marker, "key.pem": key, "token": hook_token + "\n",
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
                            "kill-timeout: 1s", "max-concurrency: 64"):
                assert setting in config, setting
            assert "allow-environment-overrides: false" in config
            assert "registration:\n        enabled: false" in config
            assert "require-sasl:\n        enabled: true" in config
            assert "force-nick-equals-account: true" in config
            mode = irc_exec("stat", "-c", "%a %u", "/run/subspace-irc/auth-token").stdout.split()
            assert mode == ["600", "10001"], mode
            assert irc_exec("stat", "-c", "%a %u", "/run/subspace-irc").stdout.split() == ["700", "10001"]
            assert "request" not in logs("auth-stub"), "the bootstrap server must not call the hook"

            # Accepted: signed in under the name the API returned, account created without a password.
            with signs_in(plain, "hook-presented@laptop", "sirc1.valid", "hook-member") as member:
                member.send("PRIVMSG NickServ :PASSWD sirc1.valid replacement-pw replacement-pw")
                member.expect("managed externally")
            assert account_exists(plain, "hook-member")
            assert f"request {OPERATOR}" not in logs("auth-stub")
            # The same credential for another presented name is refused by the stub.
            refused(plain, "hook-other", "sirc1.valid")
            refused(plain, "hook-refused", "sirc1.refuse")
            for passphrase, name in FAIL_CLOSED.items():
                refused(plain, name, passphrase)
            # Seeded local accounts still sign in while the hook is on.
            signs_in(plain, OPERATOR, operator_pw, OPERATOR).__exit__()
            assert f"request {OPERATOR}" not in logs("auth-stub"), "operator password sent to the API"
            for name in ["hook-refused", "hook-other", *FAIL_CLOSED.values()]:
                assert not account_exists(plain, name), f"{name} must not be created"

            irc_logs = logs("irc")
            for reason in ("answered HTTP 500", "answered HTTP 302", "unreachable or timed out",
                           "answered with an invalid body"):
                assert f"subspace-auth: Subspace API {reason}" in irc_logs, reason
            assert not any(value in irc_logs for value in secrets_in_logs), "credential in Ergo logs"
            print("Passed hook sign-in, autocreation, refusal, fail-closed answers and local fallback")

            # API down: local accounts still sign in; API credentials do not.
            compose("stop", "auth-stub")
            signs_in(plain, AGENT, agent_pw, AGENT).__exit__()
            signs_in(plain, OPERATOR, operator_pw, OPERATOR).__exit__()
            refused(plain, "hook-presented", "sirc1.valid")

            # TLS is verified: without the private CA the stub's certificate is refused.
            compose("up", "--wait", "--wait-timeout", "60", "--force-recreate",
                    overrides={"IRC_AUTH_API_CA_FILE": ""}, check=True)
            refused(port(), "hook-presented", "sirc1.valid")
            assert "unreachable or timed out" in logs("irc")
            print("Passed API outage fallback and TLS verification")

            compose("stop", "irc")
            fails_to_start("IRC_AUTH_API_URL needs the hook token", IRC_AUTH_API_TOKEN_FILE="")
            fails_to_start("Set only one of IRC_AUTH_API_TOKEN and IRC_AUTH_API_TOKEN_FILE",
                           IRC_AUTH_API_TOKEN=hook_token)
            fails_to_start("The hook token must be", IRC_AUTH_API_TOKEN_FILE="",
                           IRC_AUTH_API_TOKEN="short")
            fails_to_start("Cannot read IRC_AUTH_API_TOKEN_FILE", IRC_AUTH_API_TOKEN_FILE="/run/missing")
            for url in ("ftp://auth-stub", "https://user:pw@auth-stub", "https://auth-stub/?q=1", "auth-stub"):
                fails_to_start("IRC_AUTH_API_URL must be an http or https URL", IRC_AUTH_API_URL=url)
            fails_to_start("IRC_AUTH_API_CA_FILE must be readable", IRC_AUTH_API_CA_FILE="/run/missing.pem")
            fails_to_start("need IRC_AUTH_API_URL", IRC_AUTH_API_URL="")
            fails_to_start("need IRC_AUTH_API_URL", IRC_AUTH_API_URL="", IRC_AUTH_API_TOKEN_FILE="",
                           IRC_AUTH_API_CA_FILE="", IRC_AUTH_API_TOKEN=hook_token)
            print("Passed hook start-up validation")
        except BaseException:
            compose("logs", "--tail", "60", check=False)
            raise
        finally:
            compose("down", "--volumes", "--remove-orphans", "--rmi", "local", check=False)


if __name__ == "__main__":
    smoke()
