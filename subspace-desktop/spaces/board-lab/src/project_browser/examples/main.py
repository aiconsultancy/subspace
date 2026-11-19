"""A local, simulated live dashboard. No host metrics or network are read."""

import asyncio
import math
import sys
import time
from collections import deque

# The definition's output IDs are the keys in results below. Inputs are
# accepted by Board Lab before callbacks run; they are not environment values.
# Working files do not run until Start or Apply & Restart approves a snapshot.
# Private settings are separate and are not part of the source snapshot.
history = deque(maxlen=60)
sample_number = 0
timer_task = None  # Keep a strong reference until cancelled or process exit.


def sample():
    global sample_number
    sample_number += 1
    # Deterministic waves make change visible without observing this machine.
    history.append((sample_number, round(50 + 25 * math.sin(sample_number / 5)),
                    round(60 + 15 * math.sin(sample_number / 8)),
                    round(35 + 10 * math.sin(sample_number / 11))))


def chart(kind, title, labels, series):
    return {"kind": kind, "orientation": "vertical", "stacked": False,
            "title": title, "subtitle": "SIMULATED — not host measurements",
            "labels": labels, "categoryAxisTitle": "Sample", "valueAxisTitle": "%",
            "series": series, "xScale": "number", "legend": True, "format": "percent"}


def result_patch(context):
    # A patch replaces only these output components. Each result has a scope,
    # evaluation time, and a typed value. Chart labels and values must align;
    # doughnuts instead have one one-value series for each labelled slice.
    stamp = int(time.time() * 1000)
    def output(value):
        return {"scope": "simulated", "evaluated_at_ms": stamp, "value": value}

    latest = history[-1]
    labels = [str(row[0]) for row in history]
    cpu, ram, disk = latest[1:]
    cpu_slices = [{"label": "Used", "values": [cpu]},
                  {"label": "Free", "values": [100 - cpu]}]
    results = {
        "summary": {"type": "text", "text": f"SIMULATED CPU {cpu}% · RAM {ram}% · disk {disk}% | {context.values.get('mode', 'Running')} | {len(history)} samples"},
        "utilization": {"type": "chart", "chart": chart("doughnut", "SIMULATED CPU", ["Used", "Free"], cpu_slices)},
        "history": {"type": "chart", "chart": chart("line", "SIMULATED CPU history", labels, [{"label": "CPU", "values": [row[1] for row in history]}])},
        "memory": {"type": "chart", "chart": chart("area", "SIMULATED RAM history", labels, [{"label": "RAM", "values": [row[2] for row in history]}])},
        "comparison": {"type": "chart", "chart": chart("bar", "SIMULATED utilization", ["CPU", "RAM", "Disk"], [{"label": "Utilization", "values": [cpu, ram, disk]}])},
        "rows": {"type": "table", "table": {"columns": ["Sample", "SIMULATED CPU %", "SIMULATED RAM %", "SIMULATED disk %"], "rows": [list(row) for row in reversed(history)]}},
    }
    return {"results": {key: output(value) for key, value in results.items()}}


async def publish_samples(context):
    # One retained task sleeps before each publication. Input callbacks cancel
    # it and start a new sleep, so changing rate takes effect immediately.
    while True:
        await asyncio.sleep(float(context.values.get("rate", 1)))
        sample()
        try:
            context.publish(result_patch(context))
        except (ValueError, OSError) as error:
            # Background failures bypass callback dispatch. Report once on
            # stderr and stop; cancellation still propagates normally.
            print(f"SIMULATED dashboard publication failed: {error}", file=sys.stderr)
            return


def schedule(context):
    global timer_task
    if timer_task is not None:
        timer_task.cancel()
        timer_task = None
    if context.values.get("mode", "Running") == "Running":
        timer_task = asyncio.create_task(publish_samples(context))


def on_init(context, _event):
    # Return the first sample, not an empty patch: it is visible before a tick.
    sample()
    schedule(context)
    return result_patch(context)


def on_input(context, _event):
    # mode is Running/Paused; rate is accepted seconds in the declared range.
    # Pausing stops timer publication, while this patch reports the new state.
    schedule(context)
    return result_patch(context)


def on_action(context, event):
    if event["action"] == "reset":
        history.clear()
        sample()
    return result_patch(context)


# The helper reports callback exceptions as errors; don't print protocol JSON.
# There is no on_stop callback. Stop exits the helper; asyncio.run cancels the
# retained task during loop shutdown. Restart creates a fresh process/history.
