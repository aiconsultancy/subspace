"""Exercise a real fetch into the rear admiral's reference checkout."""
import io
import json
from threading import Barrier
from unittest.mock import patch
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

DIR_HERE = Path(__file__).resolve().parents[1] / 'lib'
sys.path.insert(0, str(DIR_HERE))
spec = importlib.util.spec_from_file_location('repo_refresh', DIR_HERE / 'repo_refresh.py')
refresh = importlib.util.module_from_spec(spec)
spec.loader.exec_module(refresh)


def git(directory, *args):
    return subprocess.check_output(
        ['git', '-C', str(directory), '-c', 'user.name=Fleet Test',
         '-c', 'user.email=fleet@example.com', '-c', 'commit.gpgsign=false', *args],
        stderr=subprocess.PIPE, text=True,
    ).strip()


class ReferenceRefresh(unittest.TestCase):
    def test_reads_overlap_on_one_mcp_connection(self):
        readers = Barrier(5, timeout=5)

        def read(_tool, arguments):
            readers.wait()
            return {'startLine': arguments['startLine']}

        requests = [{'id': n, 'method': 'tools/call', 'params': {'name': refresh.READ,
                     'arguments': {'repository': 'demo', 'branch': 'main', 'path': 'README',
                                   'startLine': n}}} for n in range(1, 6)]
        stream = io.BytesIO(b''.join(json.dumps(req).encode() + b'\n' for req in requests))
        output = io.StringIO()
        with patch.object(refresh, 'ask', side_effect=read):
            refresh.mcp(stream, output)
        replies = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(sorted(r['id'] for r in replies), list(range(1, 6)))
        for reply in replies:
            self.assertEqual(json.loads(reply['result']['content'][0]['text'])['startLine'], reply['id'])

    def test_fetch_lock_is_per_checkout(self):
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            with refresh.refresh_lock(a), refresh.refresh_lock(b):
                with self.assertRaises(BlockingIOError), refresh.refresh_lock(a):
                    self.fail('Concurrent fetch entered the same checkout')
            with refresh.refresh_lock(a):
                pass

    def test_fetch_published_refs_without_touching_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = root / 'source'
            source.mkdir()
            git(source, 'init', '-b', 'main')
            (source / 'README').write_text('initial\n')
            git(source, 'add', 'README')
            git(source, 'commit', '-m', 'initial')
            reference = root / 'reference'
            git(root, 'clone', '--no-local', str(source), str(reference))
            original = git(reference, 'rev-parse', 'HEAD')
            git(reference, 'branch', 'local-work')
            git(reference, 'config', 'fetch.prune', 'true')
            git(reference, 'update-ref', 'refs/remotes/origin/old', original)
            (reference / 'README').write_text('local edit\n')
            (reference / '.git' / 'FETCH_HEAD').write_text('existing fetch result\n')
            (source / 'README').write_text('published change\n')
            git(source, 'commit', '-am', 'published change')
            git(source, 'tag', 'published-tag')
            published = git(source, 'rev-parse', 'HEAD')

            def run_git(*args):
                return git(reference, *args)

            with self.assertRaisesRegex(ValueError, 'origin'):
                refresh.refresh_refs(run_git, str(root / 'other'))
            first = refresh.refresh_refs(run_git, str(source))
            self.assertEqual(first['checkoutHead'], original)
            self.assertEqual(first['updatedRefs'], ['origin/main ' + published])
            self.assertEqual(refresh.refresh_refs(run_git, str(source))['updatedCount'], 0)
            self.assertEqual(refresh.read_origin_file(run_git, 'main', 'README'), {
                'commit': published, 'startLine': 1, 'lines': ['published change'], 'nextLine': None,
            })
            with self.assertRaisesRegex(ValueError, 'credential'):
                refresh.read_origin_file(run_git, 'main', '.env')
            self.assertEqual(git(reference, 'symbolic-ref', 'HEAD'), 'refs/heads/main')
            self.assertEqual(git(reference, 'rev-parse', 'local-work'), original)
            self.assertEqual(first['deletedRefs'], ['origin/old'])
            self.assertNotIn('origin/old', git(reference, 'branch', '-r'))
            with self.assertRaises(subprocess.CalledProcessError):
                refresh.read_origin_file(run_git, 'old', 'README')
            self.assertEqual(git(reference, 'tag'), '')
            self.assertEqual((reference / 'README').read_text(), 'local edit\n')
            self.assertEqual((reference / '.git' / 'FETCH_HEAD').read_text(), 'existing fetch result\n')

    def test_tool_refuses_paths_and_urls(self):
        for value in ('../other', '/tmp/other', 'https://example.com/repo', {'name': 'repo'}):
            with self.subTest(value=value):
                result = refresh.message({'method': 'tools/call', 'params': {
                    'name': refresh.REFRESH, 'arguments': {'repository': value},
                }})
                self.assertTrue(result['isError'])


if __name__ == '__main__':
    unittest.main()
