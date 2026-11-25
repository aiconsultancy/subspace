#!/usr/bin/env python3
"""Offline checks of the Stargate Fleet API fixture. Works with python3 -I and uses only the stdlib."""

import base64
import contextlib
import hashlib
import importlib.util
import io
import json
import re
import socket
import struct
import threading
import unittest
import zlib
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

SPEC = importlib.util.spec_from_file_location('fleet_fixture', Path(__file__).with_name('fleet_api_test_server.py'))
FIXTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIXTURE)

TOKEN = 'fixture-fleet'
WREN_DIFF = '/api/v1/ships/aurora-lyra/crew/wren/diff'
FLEET_FIELDS = set('apiVersion generatedAt name host gateway admiral ships longestIdle'.split())
SHIP_FIELDS = set('id name captain repository available crew features'.split())
MEMBER_FIELDS = set('id name role model backend activity since idleSince contexts subagents'.split())
CONTEXT_FIELDS = set('project activity provisioned observed current queuedCount queued lastOutcome recent '
                     'recentCount git pullRequest'.split())


class FleetFixtureTests(unittest.TestCase):
    def setUp(self):
        self.server = FIXTURE.FleetServer(0, TOKEN, tick=0.2)
        self.worker = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.worker.start()
        self.addCleanup(self.close)

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.worker.join()

    def open(self, path, token=TOKEN, method='GET', origin=None):
        headers = {'Authorization': f'Bearer {token}'} if token else {}
        if origin:
            headers['Origin'] = origin
        try:
            return urlopen(Request(self.server.origin + path, headers=headers, method=method), timeout=3)
        except HTTPError as error:
            return error

    def get(self, path, **options):
        with self.open(path, **options) as response:
            return response.status, json.loads(response.read() or b'null'), response.headers

    def diff(self, route=WREN_DIFF, **params):
        return self.get(route + '?' + urlencode(params))[:2]

    def test_crew_diff_list_pages_and_modes_share_a_fingerprint(self):
        project = 'navigation-console'
        status, first = self.diff(project=project, mode='branch')
        self.assertEqual(status, 200)
        status, second = self.diff(project=project, mode='branch', cursor=first['nextCursor'])
        self.assertEqual(status, 200)
        files = first['files'] + second['files']
        paths = [entry['path'] for entry in files]
        self.assertEqual(paths, sorted(set(paths)))
        self.assertEqual((first['total'], second['total'], len(files)), (6, 6, 6))
        self.assertEqual((first['additions'], first['deletions']), (7, 2))
        self.assertNotIn('nextCursor', second)
        self.assertEqual(first['fingerprint'], second['fingerprint'])
        self.assertEqual((first['ship'], first['member'], first['project']), ('aurora-lyra', 'wren', project))
        self.assertEqual(len(first['head']), 40)
        for mode, count in [('working', 4), ('committed', 2)]:
            status, page = self.diff(mode=mode)
            self.assertEqual((status, page['mode'], page['total']), (200, mode, count))
            self.assertIsNone(page['project'])
            self.assertEqual(page['fingerprint'], first['fingerprint'])
        self.assertEqual(self.diff(mode='working', cursor=first['nextCursor']), (400, None))

    def test_patch_continuation_and_reread(self):
        path = 'src/chart/tiles.ts'
        first_list = self.diff()[1]
        last_list = self.diff(cursor=first_list['nextCursor'])[1]
        inline = next(file for file in last_list['files'] if file['path'] == path)
        params = dict(path=path, at=first_list['fingerprint'])
        status, first = self.diff(WREN_DIFF + '/file', **params)
        self.assertEqual((status, first['file']), (200, inline))
        status, last = self.diff(WREN_DIFF + '/file', page=inline['patchNextPage'], **params)
        self.assertEqual((status, last['fingerprint']), (200, params['at']))
        self.assertNotIn('patchNextPage', last['file'])
        joined = first['file']['patch'] + last['file']['patch']
        self.assertIn('return `${tile.sector}:${tile.id}`;', joined)
        self.assertIn('+export const MAX_VISIBLE_TILES = 48;\n', joined)
        long_line = next(file['patch'] for file in last_list['files'] if file['path'] == 'src/chart/waypoints.ts')
        self.assertGreater(len(long_line.splitlines()[-1]), 550)

    def test_binary_pages_reassemble_both_png_sides_and_end_exactly_at_eof(self):
        snapshot = self.diff()[1]
        png_path = 'fixtures/chart-tile.png'
        png = next(file for file in snapshot['files'] if file['path'] == png_path)
        old = self.collect_bytes(snapshot, png_path, expected_side='old', side='old')
        new = self.collect_bytes(snapshot, png_path, expected_side='new')
        self.assertNotEqual(old, new)
        for data in (old, new):
            self.assertEqual(data[:8], b'\x89PNG\r\n\x1a\n')
            self.assertEqual(struct.unpack('>II', data[16:24]), (160, 96))
            self.assertEqual(data[37:41], b'IDAT')
            end = 41 + int.from_bytes(data[33:37], 'big')
            self.assertEqual(len(zlib.decompress(data[41:end])), 96 * (1 + 160 * 3))
        self.assertEqual(len(new), png['size'])
        removed_path = 'fixtures/legacy-tile.bin'
        removed = self.collect_bytes(snapshot, removed_path, expected_side='old')
        self.assertEqual(removed, b'CHART\x00\x01\xff' + bytes(range(184)))
        self.assertEqual(self.diff(WREN_DIFF + '/file', path=removed_path, side='new',
                                   at=snapshot['fingerprint']), (404, None))

    def collect_bytes(self, snapshot, path, expected_side, **params):
        chunks = bytearray()
        for number in range(16):  # The image preview's page budget.
            status, page = self.diff(WREN_DIFF + '/file', path=path, at=snapshot['fingerprint'], **params)
            self.assertEqual((status, page['fingerprint']), (200, snapshot['fingerprint']))
            content = page['content']
            self.assertEqual((content['side'], content['encoding'], content['offset']),
                             (expected_side, 'base64', len(chunks)))
            chunks.extend(base64.b64decode(content['data'], validate=True))
            self.assertEqual('nextPage' in content, len(chunks) < content['size'])
            if 'nextPage' not in content:
                self.assertGreater(number, 0)
                self.assertEqual(len(chunks), content['size'])
                return bytes(chunks)
            params['page'] = content['nextPage']
        self.fail('Binary continuation never ended')

    def test_crew_diff_empty_unavailable_and_invalid_requests(self):
        status, clean = self.diff('/api/v1/ships/aurora-lyra/crew/talon/diff')
        self.assertEqual((status, clean['total'], clean['files']), (200, 0, []))
        self.assertEqual(self.diff('/api/v1/ships/aurora-castor/crew/corvin/diff'), (404, None))
        self.assertEqual(self.diff(project='missing'), (404, None))
        self.assertEqual(self.diff(mode='missing'), (400, None))
        self.assertEqual(self.diff(cursor='bad'), (400, None))
        at = self.diff()[1]['fingerprint']
        route = WREN_DIFF + '/file'
        self.assertEqual(self.diff(route, at=at, path='docs/chart notes.md')[0], 200)
        self.assertEqual(self.diff(route, at=at, path='../outside'), (400, None))
        self.assertEqual(self.diff(route, at=at, path='missing'), (404, None))
        self.assertEqual(self.diff(route, at='stale', path='src/chart/tiles.ts'), (409, None))
        self.assertEqual(self.get(WREN_DIFF, token=None)[0], 401)
        with self.open(WREN_DIFF, method='HEAD') as response:
            self.assertEqual((response.status, response.read()), (200, b''))

    def test_diff_fingerprint_is_added_to_ship_events_only(self):
        fingerprint = self.diff()[1]['fingerprint']

        def wren_git(ship):
            return next(who for who in ship['crewMembers'] if who['id'] == 'wren')['contexts'][0]['git']

        with self.open('/api/v1/events') as response:
            self.assertEqual(response.readline(), b'event: fleet\n')
            response.readline()
            response.readline()
            self.assertEqual(response.readline(), b'event: ship\n')
            ship = json.loads(response.readline()[6:])
        self.assertEqual(wren_git(ship)['fingerprint'], fingerprint)
        self.assertNotIn('fingerprint', wren_git(self.get('/api/v1/ships/aurora-lyra')[1]))
        self.assertNotIn('fingerprint', self.get('/api/v1/ships/aurora-lyra/crew/wren')[1]['member']['contexts'][0]['git'])
        with self.server.lock:
            snapshot = self.server.fleet.snapshot()
            detail = self.server.fleet.ships[0]
            wren_git(detail)['head'] = 'b15cafe'
            changed = self.server.fleet.changed(detail, detail['crewMembers'][0])[0][1]
        self.assertNotIn('fingerprint', wren_git(snapshot['ships']['aurora-lyra']))
        self.assertNotEqual(wren_git(changed)['fingerprint'], fingerprint)
        self.assertEqual(wren_git(changed)['fingerprint'], self.diff()[1]['fingerprint'])

    def test_a_server_refusing_streams_still_serves_reads(self):
        busy = FIXTURE.FleetServer(0, TOKEN, tick=0, refuse_events=True)
        worker = threading.Thread(target=busy.serve_forever, daemon=True)
        worker.start()
        try:
            request = Request(busy.origin + '/api/v1/events', headers={'Authorization': f'Bearer {TOKEN}'})
            with self.assertRaises(HTTPError) as refused:
                urlopen(request, timeout=3)
            self.assertEqual((refused.exception.code, refused.exception.headers['Retry-After']), (503, '5'))
            request = Request(busy.origin + '/api/v1/fleet', headers={'Authorization': f'Bearer {TOKEN}'})
            with urlopen(request, timeout=3) as response:
                self.assertEqual(response.status, 200)
        finally:
            busy.shutdown()
            busy.server_close()
            worker.join()

    def test_a_request_log_fingerprints_the_token_without_printing_it(self):
        self.server.log_requests = True
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            self.get('/api/v1/fleet')
            self.get('/api/v1/fleet', token='other-token')
            self.get('/api/v1/fleet', token=None)
        lines = printed.getvalue().splitlines()
        given = hashlib.sha256(f'Bearer {TOKEN}'.encode()).hexdigest()[:12]
        other = hashlib.sha256(b'Bearer other-token').hexdigest()[:12]
        self.assertEqual(lines, [f'request GET /api/v1/fleet 200 auth={given}',
                                 f'request GET /api/v1/fleet 401 auth={other}',
                                 'request GET /api/v1/fleet 401 auth=none'])
        self.assertNotIn(TOKEN, printed.getvalue())
        self.assertNotIn('other-token', printed.getvalue())

    def raw(self, request):
        """Sends `request` as bytes and returns the whole response."""
        host, port = self.server.server_address[:2]
        with socket.create_connection((host, port), timeout=3) as conn:
            conn.sendall(request)
            return conn.makefile('rb').read()

    def test_a_malformed_request_is_logged_without_failing_the_log(self):
        # A one-word request line: some Python versions answer it as
        # HTTP/0.9, with no status line, so the error is checked in the
        # body and the log, which every version writes.
        self.server.log_requests = True
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            response = self.raw(b'NONSENSE\r\n\r\n')
        self.assertIn(b'400', response)
        self.assertIn(b'Bad request syntax', response)
        self.assertEqual(printed.getvalue().splitlines(), ['request - - 400 auth=none'])

    def test_a_target_that_cannot_be_parsed_is_refused_and_logged(self):
        self.server.log_requests = True
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            response = self.raw(b'GET http://[bad/x HTTP/1.1\r\nConnection: close\r\n\r\n')
        self.assertTrue(response.startswith(b'HTTP/1.1 400 '), response[:40])
        self.assertEqual(printed.getvalue().splitlines(), ['request GET - 400 auth=none'])

    def test_control_characters_in_the_method_and_path_are_escaped(self):
        self.server.log_requests = True
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            self.raw(b'GE\x1bT /api/v1/fl\x1b[2Jeet HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n')
        line = printed.getvalue().splitlines()[0]
        self.assertNotIn('\x1b', line)
        self.assertTrue(line.startswith('request GE\\x1bT /api/v1/fl\\x1b[2Jeet '), line)

    def test_every_route_needs_the_token_and_is_not_cached(self):
        for token in (None, 'wrong'):
            status, _, headers = self.get('/api/v1/fleet', token=token)
            self.assertEqual(status, 401)
        status, _, headers = self.get('/api/v1/fleet')
        self.assertEqual(status, 200)
        self.assertEqual(headers['Cache-Control'], 'no-store')
        self.assertEqual(self.get('/api/v1/nothing')[0], 404)
        self.assertEqual(self.get('/api/v1/ships/missing')[0], 404)
        self.assertEqual(self.get('/api/v1/ships/aurora-lyra/crew/missing')[0], 404)
        with self.open('/api/v1/fleet', method='HEAD') as response:
            self.assertEqual((response.status, response.read()), (200, b''))

    def test_the_overview_has_the_v1_shape_and_a_busy_fleet(self):
        fleet = self.get('/api/v1/fleet')[1]
        self.assertEqual(set(fleet), FLEET_FIELDS)
        self.assertEqual((fleet['apiVersion'], fleet['name'], fleet['host']['hostname']), (1, 'aurora', 'aurora-vm-01'))
        self.assertEqual(fleet['admiral']['role'], 'rear_admiral')
        self.assertEqual(len(fleet['host']['disks']), 2)
        self.assertEqual(set(fleet['gateway']), {'version', 'opencodeVersion', 'bunVersion', 'startedAt', 'features'})
        ships = fleet['ships']
        self.assertEqual([ship['id'] for ship in ships], ['aurora-lyra', 'aurora-orion', 'aurora-vega', 'aurora-castor'])
        for ship in ships:
            self.assertEqual(set(ship), SHIP_FIELDS)
            self.assertTrue(ship['available'])
        self.assertEqual(sum(ship['crew']['people'] for ship in ships), 16)
        self.assertEqual(sum(ship['crew']['busy'] for ship in ships), 9)
        self.assertEqual(sum(ship['crew']['queued'] for ship in ships), 5)
        self.assertEqual(fleet['longestIdle']['member'], 'basil')
        self.assertEqual(sum(1 for ship in ships if any(f['key'].endswith('peerCaptains') and f['enabled']
                                                         for f in ship['features'])), 2)

    def test_ship_and_crew_detail_match_the_overview(self):
        detail = self.get('/api/v1/ships/aurora-orion')[1]
        self.assertNotIn('detailUpdatedAt', detail)
        activities = {member['id']: member['activity'] for member in detail['crewMembers']}
        self.assertEqual(activities, dict(marlow='running', pike='uncertain', ines='running', sable='running'))
        for member in detail['crewMembers']:
            self.assertEqual(set(member), MEMBER_FIELDS)
            for context in member['contexts']:
                self.assertLessEqual(CONTEXT_FIELDS, set(context))
        ines = self.get('/api/v1/ships/aurora-orion/crew/ines')[1]
        self.assertEqual(ines['ship'], 'aurora-orion')
        self.assertEqual([context['project'] for context in ines['member']['contexts']], ['sector-atlas', 'relay-handover'])

    def test_only_subspace_origins_get_cors_headers(self):
        with self.open('/api/v1/fleet', token=None, method='OPTIONS', origin='subspace://fleet-console') as response:
            self.assertEqual(response.status, 204)
            self.assertEqual(response.headers['Access-Control-Allow-Origin'], 'subspace://fleet-console')
            self.assertIn('Authorization', response.headers['Access-Control-Allow-Headers'])
        with self.open('/api/v1/fleet', token=None, method='OPTIONS', origin='https://elsewhere.example.com') as response:
            self.assertEqual(response.status, 403)
            self.assertIsNone(response.headers['Access-Control-Allow-Origin'])
        headers = self.get('/api/v1/fleet', origin='https://subspace.fleet-console')[2]
        self.assertEqual(headers['Access-Control-Allow-Origin'], 'https://subspace.fleet-console')

    def test_the_stream_sends_a_snapshot_then_changes(self):
        with self.open('/api/v1/events') as response:
            self.assertEqual(response.headers['Content-Type'], 'text/event-stream')
            kinds = []
            while len(kinds) < 7:
                line = response.readline().decode()
                if line.startswith('event: '):
                    kinds.append(line[7:].strip())
                elif line.startswith('data: '):
                    json.loads(line[6:])
        self.assertEqual(kinds[:5], ['fleet', 'ship', 'ship', 'ship', 'ship'])
        self.assertIn('activity', kinds[5:])

    def test_linked_issues_resolve_in_the_subspace_api_fixture(self):
        spec = importlib.util.spec_from_file_location('api_fixture', Path(__file__).with_name('subspace_api_test_server.py'))
        api = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(api)
        known = {row[0] for row in api.FLEET_ISSUES}
        text = json.dumps(FIXTURE.Fleet().snapshot()) + ' '.join(FIXTURE.POOL)
        found = set(re.findall(r'(?<![A-Za-z0-9])((?:NAV|MAP|WARP|RELAY)-[1-9][0-9]*)(?![0-9])', text, re.IGNORECASE))
        found = {identifier.upper() for identifier in found}
        self.assertGreaterEqual(len(found), 15)
        self.assertEqual(found - known, set())

    def test_the_simulation_keeps_the_fleet_consistent(self):
        fleet = FIXTURE.Fleet(now=1_800_000_000_000, seed=3)
        kinds = set()
        for step in range(200):
            fleet.set_now(1_800_000_000_000 + step * 5000)
            for kind, data in fleet.tick():
                kinds.add(kind if kind != 'activity' else data['kind'])
        self.assertLessEqual({'phase', 'outcome', 'dispatch', 'ship', 'fleet'}, kinds)
        overview = fleet.overview()
        self.assertGreaterEqual(sum(ship['crew']['busy'] for ship in overview['ships']), 4)
        for ship in overview['ships']:
            detail = fleet.ship(ship['id'])
            self.assertEqual(ship['crew']['busy'], sum(m['activity'] == 'running' for m in detail['crewMembers']))


if __name__ == '__main__':
    unittest.main()
