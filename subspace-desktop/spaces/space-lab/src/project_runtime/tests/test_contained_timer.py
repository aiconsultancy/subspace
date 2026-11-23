"""Check the contained fixture's background timer lifetime without a container."""

import asyncio
import gc
import importlib.util
import os
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import patch
import weakref


class TimerLifetimeTest(unittest.IsolatedAsyncioTestCase):
    async def test_timer_survives_collection_and_publishes(self):
        """A pending timer stays alive even when the event loop holds weak references."""
        packaging = types.ModuleType("packaging")
        packaging.version = types.SimpleNamespace(Version=lambda _value: types.SimpleNamespace(major=1))
        sibling = types.SimpleNamespace(VALUE=7)
        spec = importlib.util.spec_from_file_location("contained_fixture", Path(__file__).with_name("contained.py"))
        module = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {"packaging": packaging, "packaging.version": packaging.version, "sibling": sibling}):
            spec.loader.exec_module(module)

        pending = None
        task_ref = None
        create_task = asyncio.create_task
        published = []

        async def paused_sleep(_delay):
            nonlocal pending
            future = asyncio.get_running_loop().create_future()
            pending = weakref.ref(future)
            await future

        def track_task(coroutine):
            nonlocal task_ref
            task = create_task(coroutine)
            task_ref = weakref.ref(task)
            return task

        with patch.dict(os.environ, {"SYNTHETIC_VALUE": "dummy-test-only-selected-value"}, clear=True):
            with patch.object(module.asyncio, "sleep", paused_sleep), patch.object(module.asyncio, "create_task", track_task):
                module.on_init(types.SimpleNamespace(publish=published.append), {})
                await asyncio.tasks.sleep(0)
                gc.collect()
                self.assertIsNotNone(task_ref(), "pending timer was collected")
                pending().set_result(None)
                await asyncio.tasks.sleep(0)

        self.assertEqual(published, [module.result("timer")])


if __name__ == "__main__":
    unittest.main()
