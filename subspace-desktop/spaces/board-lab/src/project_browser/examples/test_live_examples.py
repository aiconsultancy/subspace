"""Exercise the real starter files through the Python, Node and Bun helpers.

Run with: python3 test_live_examples.py --python PATH --node PATH --bun PATH
This checks helper RPC, not managed environments or macOS containment.
"""
import argparse
import asyncio
import contextlib
import importlib.util
import io
import pathlib
import subprocess
import unittest

HERE = pathlib.Path(__file__).resolve().parent
RUNTIME = HERE.parent.parent / "project_runtime"
spec = importlib.util.spec_from_file_location("helper_tests", RUNTIME / "helper_tests.py")
helper_tests = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper_tests)
RUNTIMES = {}


class TimerErrors(unittest.IsolatedAsyncioTestCase):
    async def test_python_publication_failure_stops_and_reports_to_stderr(self):
        project_spec = importlib.util.spec_from_file_location("starter_example", HERE / "main.py")
        project = importlib.util.module_from_spec(project_spec)
        project_spec.loader.exec_module(project)

        for error_type in (ValueError, OSError):
            with self.subTest(error=error_type.__name__):
                class Context:
                    values = {"rate": 0.001}
                    calls = 0

                    def publish(self, patch):
                        self.calls += 1
                        raise error_type("dummy publication failure")

                context = Context()
                stdout, stderr = io.StringIO(), io.StringIO()
                with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    await asyncio.wait_for(project.publish_samples(context), timeout=1)
                self.assertEqual(context.calls, 1)
                self.assertEqual(stdout.getvalue(), "")
                self.assertIn("SIMULATED", stderr.getvalue())
                self.assertIn("dummy publication failure", stderr.getvalue())

        task = asyncio.create_task(project.publish_samples(Context()))
        await asyncio.sleep(0)
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task


class LiveExamples(unittest.TestCase):
    def test_live_controls_and_restart(self):
        for kind, executable in RUNTIMES.items():
            with self.subTest(runtime=kind):
                self.exercise(kind, executable)

    def exercise(self, kind, executable):
        helper = RUNTIME / ("python.py" if kind == "python" else "javascript.mjs")
        source = HERE / ("main.py" if kind == "python" else "main.mjs")
        command = [executable, "-I", "-u", str(helper), str(source)] if kind == "python" else [executable, str(helper), str(source)]

        def launch():
            return subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

        values = {"rate": 0.1, "mode": "Running"}
        process = launch()
        protocol = helper_tests.Protocol(process)
        try:
            protocol.send("init", 0, {"type": "init", "definition": {}, "values": values})
            initial = protocol.read()["event"]
            self.assertEqual(initial["type"], "patch")
            self.assertEqual(len(initial["patch"]["results"]["history"]["value"]["chart"]["labels"]), 1)
            self.assertEqual(protocol.read()["event"]["type"], "ready")
            latest = None
            for _ in range(65):
                latest = protocol.read()["event"]
                self.assertEqual(latest["type"], "patch")
            chart = latest["patch"]["results"]["history"]["value"]["chart"]
            self.assertEqual(len(chart["labels"]), 60)
            self.assertEqual(len(chart["series"][0]["values"]), 60)
            self.assertEqual(len(latest["patch"]["results"]["rows"]["value"]["table"]["rows"]), 60)
            results = latest["patch"]["results"]
            self.assertEqual([results[key]["value"]["chart"]["kind"] for key in
                              ("utilization", "history", "memory", "comparison")],
                             ["doughnut", "line", "area", "bar"])
            doughnut = results["utilization"]["value"]["chart"]
            self.assertEqual(len(doughnut["labels"]), len(doughnut["series"]))
            self.assertTrue(all(len(series["values"]) == 1 for series in doughnut["series"]))
            protocol.send("pause", 1, {"type": "input", "key": "mode", "value": "Paused"})
            while protocol.read()["event"]["type"] != "ack":
                pass
            self.assertEqual(protocol.buffer, b"")
            self.assertFalse(protocol.selector.select(0.35), "paused timer published")
            protocol.send("reset", 1, {"type": "action", "action": "reset", "values": {"rate": 0.1, "mode": "Paused"}})
            reset = protocol.read()["event"]
            self.assertEqual(reset["type"], "result")
            self.assertEqual(len(reset["patch"]["results"]["rows"]["value"]["table"]["rows"]), 1)
            protocol.send("rate", 2, {"type": "input", "key": "rate", "value": 0.5})
            while protocol.read()["event"]["type"] != "ack":
                pass
            protocol.send("resume", 3, {"type": "input", "key": "mode", "value": "Running"})
            while protocol.read()["event"]["type"] != "ack":
                pass
            self.assertEqual(protocol.buffer, b"")
            self.assertFalse(protocol.selector.select(0.25), "rate change did not reschedule")
            self.assertEqual(protocol.read()["event"]["type"], "patch")
            self.assertFalse(protocol.selector.select(0.25), "duplicate timer chain")
            protocol.send("faster", 4, {"type": "input", "key": "rate", "value": 0.1})
            while protocol.read()["event"]["type"] != "ack":
                pass
            self.assertTrue(protocol.selector.select(0.35), "active rate change did not reschedule")
            self.assertEqual(protocol.read()["event"]["type"], "patch")
            protocol.send("stop", 4, {"type": "stop"})
            process.wait(timeout=3)
            self.assertEqual(process.returncode, 0)
        finally:
            protocol.close()
            if process.poll() is None:
                process.kill()
                process.wait(timeout=3)
            for pipe in (process.stdin, process.stdout, process.stderr):
                pipe.close()

        process = launch()
        protocol = helper_tests.Protocol(process)
        try:
            protocol.send("restart", 0, {"type": "init", "definition": {}, "values": values})
            initial = protocol.read()["event"]
            self.assertEqual(len(initial["patch"]["results"]["history"]["value"]["chart"]["labels"]), 1)
            protocol.send("stop", 0, {"type": "stop"})
            process.wait(timeout=3)
            self.assertEqual(process.returncode, 0)
        finally:
            protocol.close()
            if process.poll() is None:
                process.kill()
                process.wait(timeout=3)
            for pipe in (process.stdin, process.stdout, process.stderr):
                pipe.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for name in ("python", "node", "bun"):
        parser.add_argument("--" + name, required=True)
    arguments, remaining = parser.parse_known_args()
    RUNTIMES = vars(arguments)
    unittest.main(argv=[__file__, *remaining])
