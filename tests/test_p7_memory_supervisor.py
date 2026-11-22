"""Small deterministic supervisor tests; no Node/native subprocesses or large allocations."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/verify-p7-automatic-supervised.py'
SPEC = importlib.util.spec_from_file_location('p7_supervisor', SCRIPT)
supervisor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(supervisor)


class Process:
    def __init__(self, rss=1, private=1, children=()):
        self.info = SimpleNamespace(rss=rss, private=private)
        self.descendants = list(children)
        self.killed = False

    def memory_info(self):
        return self.info

    def children(self, recursive):
        return self.descendants

    def kill(self):
        self.killed = True


class Popen:
    pid = 42

    def __init__(self):
        self.killed = False

    def poll(self):
        return -9 if self.killed else None

    def kill(self):
        self.killed = True

    def wait(self):
        return -9


class PolicyTests(unittest.TestCase):
    def test_admission_boundaries(self):
        f = supervisor.memory_refusal
        cases = [
            (False, 149, None, True),
            (False, 150, None, False),
            (True, 0, 150, False),
            (True, 9999, 149, True),
            (True, 9999, None, True),
        ]
        for paging, available, headroom, refused in cases:
            with self.subTest(paging=paging, available=available, headroom=headroom):
                self.assertEqual(bool(f(paging, available, headroom, 0, 100, 50, True)), refused)

    def test_runtime_reserve_and_cap(self):
        f = supervisor.memory_refusal
        self.assertTrue(f(False, 49, None, 0, 100, 50))
        self.assertIsNone(f(True, 0, 50, 100, 100, 50))
        self.assertTrue(f(True, 0, 49, 0, 100, 50))
        self.assertEqual(f(True, 9999, 9999, 101, 100, 50), 'process-tree private commit cap exceeded')
        self.assertEqual(f(False, 9999, None, 101, 100, 50), 'process-tree memory cap exceeded')

    def test_private_is_distinct_from_rss(self):
        self.assertEqual(supervisor.tree_memory(Process(10, 100), [Process(20, 200)], True), (30, 20, 300))

    def test_missing_private_refuses_without_rss_fallback(self):
        process = Process(10)
        del process.info.private
        with self.assertRaisesRegex(RuntimeError, 'RSS is not commit'):
            supervisor.tree_memory(process, [], True)
        self.assertEqual(supervisor.tree_memory(process, [], False), (10, 0, None))

    def test_child_peak_when_parent_disappears(self):
        parent = Process()
        with patch.object(parent, 'memory_info', side_effect=supervisor.psutil.NoSuchProcess(42)):
            self.assertEqual(supervisor.tree_memory(parent, [Process(20, 200), Process(30, 300)], True), (50, 30, 500))

    def test_non_windows_commit_refuses(self):
        with patch.object(supervisor.sys, 'platform', 'linux'):
            with self.assertRaisesRegex(RuntimeError, 'requires Windows'):
                supervisor.windows_commit_headroom()


class LifecycleTests(unittest.TestCase):
    def test_guard_and_cancel_cleanup(self):
        expected = {
            'cap': 'process-tree private commit cap exceeded',
            'commit': 'Windows commit headroom fell below reserve',
            'cancel': 'cancelled by user',
            'timeout': 'outer lifecycle guard exceeded',
            'metric': 'memory supervision metric unavailable: test missing counter',
        }
        for case, reason in expected.items():
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory, contextlib.ExitStack() as stack:
                child = Process(private=300 * supervisor.MIB if case == 'cap' else 1)
                process = Process(children=[child])
                popen = Popen()
                argv = ['supervisor', '--allow-os-paging', '--native-memory-mib', '256', '--tree-memory-mib', '256', '--reserve-mib', '1']
                stack.enter_context(patch.object(sys, 'argv', argv))
                stack.enter_context(patch.object(supervisor.Path, 'cwd', return_value=Path(directory)))
                stack.enter_context(patch.object(supervisor.psutil, 'virtual_memory', return_value=SimpleNamespace(available=0)))
                stack.enter_context(patch.object(supervisor.psutil, 'Process', return_value=process))
                stack.enter_context(patch.object(supervisor.subprocess, 'Popen', return_value=popen))
                headroom = [1000 * supervisor.MIB, 0] if case == 'commit' else [1000 * supervisor.MIB, RuntimeError('test missing counter')] if case == 'metric' else lambda: 1000 * supervisor.MIB
                stack.enter_context(patch.object(supervisor, 'windows_commit_headroom', side_effect=headroom))
                stack.enter_context(patch.object(supervisor.time, 'sleep', side_effect=KeyboardInterrupt if case == 'cancel' else AssertionError('unexpected wait')))
                stack.enter_context(patch.object(supervisor.time, 'perf_counter', side_effect=[0, 1000, 1001] if case == 'timeout' else lambda: 0))
                stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
                self.assertEqual(supervisor.main(), 1)
                result = json.loads(next(Path(directory).rglob('evidence.json')).read_text())
                self.assertEqual(result['refusal'], reason)
                self.assertTrue(popen.killed)
                self.assertTrue(child.killed)
                self.assertEqual(result['memoryCapMetric'], 'sum-tree-private-committed-bytes')
                self.assertEqual(result['minAvailableRamBytes'], 0)


if __name__ == '__main__':
    unittest.main()
