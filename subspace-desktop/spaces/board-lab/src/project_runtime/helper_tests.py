"""Direct helper contract checks, not containment or managed-package proof.

Run with explicit installed Python, Node, and Bun executable paths. This test
does not install dependencies, inherit credentials, or call any network.
"""

import argparse
import json
import os
import pathlib
import selectors
import subprocess
import tempfile
import time

PYTHON_PROJECT = '''
import asyncio
import os
import sibling

counter = 0

async def on_init(context, event):
    assert "HOME" not in os.environ
    print(os.environ["SYNTHETIC_VALUE"])
    async def poll():
        await asyncio.sleep(0.05)
        context.publish({"values": {"amount": sibling.VALUE}})
    asyncio.create_task(poll())

def on_input(context, event):
    assert context.values["amount"] == event["value"]

def on_action(context, event):
    global counter
    counter += 1
    return {"values": {"amount": sibling.VALUE + counter}}
'''

JAVASCRIPT_PROJECT = '''
const sibling = require('./sibling.cjs');
let counter = 0;
module.exports = {
  on_init(context, event) {
    if (process.env.HOME !== undefined) throw new Error('inherited HOME');
    console.log(process.env.SYNTHETIC_VALUE);
    setTimeout(() => context.publish({ values: { amount: sibling.VALUE } }), 50);
  },
  on_input(context, event) {
    if (context.values.amount !== event.value) throw new Error('missing input');
  },
  on_action(context, event) {
    return { values: { amount: sibling.VALUE + ++counter } };
  },
};
'''


class Protocol:
    def __init__(self, process):
        self.process = process
        self.buffer = b""
        self.selector = selectors.DefaultSelector()
        self.selector.register(process.stdout, selectors.EVENT_READ)

    def send(self, request, revision, event):
        message = {
            "context": {
                "version": 1,
                "session_id": "helper-fixture",
                "request_id": request,
                "input_revision": revision,
            },
            "event": event,
        }
        self.process.stdin.write(json.dumps(message).encode() + b"\n")
        self.process.stdin.flush()

    def read(self):
        deadline = time.monotonic() + 3
        while b"\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            assert remaining > 0, "helper output timed out"
            assert self.selector.select(remaining), "helper output timed out"
            chunk = os.read(self.process.stdout.fileno(), 4096)
            assert chunk, "helper protocol closed unexpectedly"
            self.buffer += chunk
            assert len(self.buffer) <= 256 * 1024, "helper frame too large"
        line, self.buffer = self.buffer.split(b"\n", 1)
        return json.loads(line)

    def close(self):
        self.selector.close()


def check(runtime, kind, helper_directory):
    with tempfile.TemporaryDirectory(prefix="subspace-helper-contract-") as directory:
        directory = pathlib.Path(directory)
        if kind == "python":
            entry = directory / "main.py"
            entry.write_text(PYTHON_PROJECT)
            (directory / "sibling.py").write_text("VALUE = 7\n")
            command = [runtime, "-I", "-u", str(helper_directory / "python.py"), str(entry)]
        else:
            entry = directory / "main.cjs"
            entry.write_text(JAVASCRIPT_PROJECT)
            (directory / "sibling.cjs").write_text("exports.VALUE = 7;\n")
            command = [runtime, str(helper_directory / "javascript.mjs"), str(entry)]
        process = subprocess.Popen(
            command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env={"SYNTHETIC_VALUE": "dummy-test-only-value"}, cwd=directory,
        )
        protocol = Protocol(process)
        try:
            protocol.send("init", 0, {"type": "init", "definition": {}, "values": {"amount": 3}})
            assert protocol.read()["event"] == {"type": "ready"}
            timer = protocol.read()
            assert timer["event"] == {"type": "patch", "patch": {"values": {"amount": 7}}}
            assert timer["context"]["request_id"] == "init"
            protocol.send("input", 1, {"type": "input", "key": "amount", "value": 8})
            assert protocol.read()["event"] == {"type": "ack"}
            for number in (1, 2):
                protocol.send(f"action-{number}", 1, {"type": "action", "action": "refresh", "values": {"amount": 8}})
                result = protocol.read()
                assert result["event"] == {"type": "result", "patch": {"values": {"amount": 7 + number}}}
                assert result["context"]["input_revision"] == 1
            protocol.send("stop", 1, {"type": "stop"})
            process.wait(timeout=3)
            assert process.returncode == 0
            assert "dummy-test-only-value" in process.stderr.read().decode()
        finally:
            protocol.close()
            if process.poll() is None:
                process.kill()
                process.wait(timeout=3)
            for pipe in (process.stdin, process.stdout, process.stderr):
                pipe.close()
    print(f"PASS {kind}: retained state, sibling import, timer, input/action, protocol-only stdout")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for kind in ("python", "node", "bun"):
        parser.add_argument("--" + kind, required=True)
    arguments = parser.parse_args()
    helper_directory = pathlib.Path(__file__).resolve().parent
    for kind in ("python", "node", "bun"):
        check(getattr(arguments, kind), kind, helper_directory)
