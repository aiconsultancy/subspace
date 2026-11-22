#!/usr/bin/env python3
"""Offline checks of the Stargate Fleet API fixture. Works with python3 -I and uses only the stdlib."""

import contextlib
import hashlib
import importlib.util
import io
import json
import re
import threading
import unittest
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

SPEC = importlib.util.spec_from_file_location('fleet_fixture', Path(__file__).with_name('fleet_api_test_server.py'))
FIXTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIXTURE)

TOKEN = 'fixture-fleet'
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
