import asyncio
import os
import sys
import packaging.version
import sibling

count = 0
background_tasks = set()


def result(text):
    """Build a synthetic heading result."""
    return {"results": {"heading": {
        "scope": "synthetic-local-contained", "evaluated_at_ms": 1234,
        "value": {"type": "text", "text": text},
    }}}


def on_init(context, event):
    """Validate the contained environment and schedule a timer update."""
    assert os.environ["SYNTHETIC_VALUE"] == "dummy-test-only-selected-value"
    assert "SUBSPACE_RUNTIME_TEST_ROOT" not in os.environ
    assert packaging.version.Version("1.2.3").major == 1
    assert sibling.VALUE == 7
    print(os.environ["SYNTHETIC_VALUE"])

    async def timer():
        """Publish the delayed fixture result."""
        await asyncio.sleep(0.05)
        context.publish(result("timer"))

    task = asyncio.create_task(timer())
    background_tasks.add(task)
    task.add_done_callback(background_tasks.discard)


def on_action(context, event):
    """Return a counter result or emit a malformed protocol frame."""
    global count
    if event["action"] == "malformed":
        sys.__stdout__.write("not-json\n")
        sys.__stdout__.flush()
        return {}
    count += 1
    return result(f"count:{count}:{os.getpid()}")
