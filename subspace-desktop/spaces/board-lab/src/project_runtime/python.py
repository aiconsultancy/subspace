"""Persistent stdlib helper. Project callbacks return patches, not UI code.

Define on_init(context, event), on_input(context, event), and
on_action(context, event). Async callbacks and asyncio background tasks work.
context.publish(patch) emits timer/stream updates using current host correlation.
"""

import asyncio
import importlib.util
import inspect
import json
import pathlib
import sys
import threading

MAX_FRAME = 256 * 1024
protocol = sys.stdout
sys.stdout = sys.stderr
output_lock = threading.Lock()


class Context:
    def __init__(self):
        self.context = None
        self.values = {}
        self.definition = None

    def emit(self, event, correlation=None):
        message = {"context": correlation or dict(self.context), "event": event}
        line = json.dumps(message, allow_nan=False, separators=(",", ":")) + "\n"
        if len(line.encode("utf-8")) > MAX_FRAME:
            raise ValueError("worker output exceeds frame budget")
        with output_lock:
            protocol.write(line)
            protocol.flush()

    def publish(self, patch):
        self.emit({"type": "patch", "patch": patch})


def load_project(entry):
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
    sys.path.insert(0, str(entry.parent))
    spec = importlib.util.spec_from_file_location("subspace_project", entry)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


async def dispatch(module, context, message):
    correlation = message["context"]
    event = message["event"]
    kind = event["type"]
    context.context = correlation
    if kind == "stop":
        return False
    if kind == "init":
        context.definition = event["definition"]
        context.values = dict(event["values"])
    elif kind == "input":
        context.values[event["key"]] = event["value"]
    elif kind == "action":
        context.values = dict(event["values"])
    try:
        callback = getattr(module, "on_" + kind, None)
        patch = callback(context, event) if callback else None
        if inspect.isawaitable(patch):
            patch = await patch
        if kind == "action":
            context.emit({"type": "result", "patch": patch or {}}, correlation)
        else:
            if patch is not None:
                context.publish(patch)
            context.emit({"type": "ready" if kind == "init" else "ack"}, correlation)
    except Exception as error:
        context.emit({"type": "error", "message": str(error)[:4096]}, correlation)
    return True


async def main():
    entry = pathlib.Path(sys.argv[1]).resolve()
    module = load_project(entry)
    context = Context()
    while True:
        line = await asyncio.to_thread(sys.stdin.buffer.readline, MAX_FRAME + 1)
        if not line:
            return
        if len(line) > MAX_FRAME or not line.endswith(b"\n"):
            raise ValueError("invalid host frame")
        if not await dispatch(module, context, json.loads(line)):
            return


if __name__ == "__main__":
    asyncio.run(main())
