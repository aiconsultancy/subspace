#!/usr/bin/env python3
"""Offline HTTP contract checks for the NETT fixture. Works with python3 -I and
uses only the stdlib.

    python3 -I scripts/test-nett-api-test-server.py [--openapi PATH]

--openapi PATH (or NETT_OPENAPI=PATH) also checks every /api/v1 route the
fixture serves against the NETT OpenAPI document: the method and path template
must exist, and a sample response of each route may use only property names
its response schema declares. Without a document those checks are skipped.
"""

import base64
import hashlib
import http.client
import importlib.util
import json
import os
import re
import socket
import subprocess
import sys
import threading
import time
import unittest
import uuid
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FutureTimeout
from contextlib import closing
from pathlib import Path
from unittest.mock import patch
from urllib.error import HTTPError
from urllib.parse import parse_qs, unquote, urlencode, urlsplit
from urllib.request import ProxyHandler, Request, build_opener

# Fixture traffic stays on loopback: no proxy from the environment sees it.
DIRECT = build_opener(ProxyHandler({}))

SERVER_PATH = Path(__file__).resolve().with_name('nett_api_test_server.py')
SPEC = importlib.util.spec_from_file_location('nett_fixture', SERVER_PATH)
NETT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NETT)


def openapi_argument(argv):
    """Takes --openapi PATH out of argv so unittest never sees it."""
    for index, argument in enumerate(argv):
        if argument == '--openapi':
            if index + 1 >= len(argv):
                sys.exit('--openapi needs a path')
            path = argv[index + 1]
            del argv[index:index + 2]
            return path
        if argument.startswith('--openapi='):
            del argv[index]
            return argument.split('=', 1)[1]
    return os.environ.get('NETT_OPENAPI')


OPENAPI = openapi_argument(sys.argv) if __name__ == '__main__' else os.environ.get('NETT_OPENAPI')
TOKEN = NETT.DEFAULT_ACCESS_TOKEN
CLIENT_ID = 'client_00000000000000000000000000'
WORKS, SECOND = NETT.uid('org/works'), NETT.uid('org/second')
PERSONAL, OUTSIDE = NETT.uid('org/personal'), NETT.uid('org/outside')
ADA, GRACE, LINUS = NETT.ADA['userId'], NETT.GRACE['userId'], NETT.LINUS['userId']
OPS, WEB = NETT.uid('works/team/OPS'), NETT.uid('works/team/WEB')
LAUNCH, HARDENING = NETT.uid('works/project/Launch'), NETT.uid('works/project/Hardening')
BUG, DOCS = NETT.uid('works/label/bug'), NETT.uid('works/label/docs')
IMAGE, NOTES, TIMING = NETT.uid('media/backup-dashboard'), NETT.uid('media/backup-runbook'), NETT.uid('media/restore-timing')
FIRST_ROOT = NETT.uid('comment/ops-1/first')
CHECKLIST = NETT.uid('list/ops-1/checklist')


def state(team_key, kind):
    return NETT.uid(f'works/team/{team_key}/state/{kind}')


def call(origin, method, path, body=None, token=TOKEN, headers=None, form=None, raw=None):
    headers = dict(headers or {})
    data = None
    if token is not None:
        headers['Authorization'] = f'Bearer {token}'
    if form is not None:
        data = urlencode(form).encode()
        headers['Content-Type'] = 'application/x-www-form-urlencoded'
    elif raw is not None:
        data = raw
    elif body is not None:
        data = json.dumps(body).encode()
        headers.setdefault('Content-Type', 'application/json')
    try:
        response = DIRECT.open(Request(origin + path, data=data, headers=headers, method=method), timeout=5)
    except HTTPError as error:
        response = error
    with response:
        payload = response.read()
        kind = response.headers.get('Content-Type', '')
        value = json.loads(payload) if payload and kind.startswith('application/') and 'json' in kind else payload
        return response.status, value, response.headers


class FixtureCase(unittest.TestCase):
    options = {}

    def setUp(self):
        self.server = NETT.FixtureServer(port=0, **self.options)
        self.worker = threading.Thread(target=self.server.serve_forever, args=(0.05,), daemon=True)
        self.worker.start()
        self.addCleanup(self.stop)

    def stop(self):
        self.server.shutdown()
        self.server.server_close()
        self.worker.join()

    def call(self, method, path, body=None, org_header=True, **options):
        """A request to the fixture. An organisation route gets the one matching
        x-org-id NETT requires unless the test sends its own or org_header is false."""
        match = re.match(r'/api/v1/orgs/([^/?]+)/', path)
        headers = dict(options.pop('headers', None) or {})
        if match and org_header and not any(name.lower() == 'x-org-id' for name in headers):
            headers['x-org-id'] = unquote(match.group(1))
        return call(self.server.origin, method, path, body, headers=headers, **options)

    def org(self, method, rest, body=None, org=WORKS, **options):
        return self.call(method, f'/api/v1/orgs/{org}{rest}', body, **options)

    def ok(self, method, rest, body=None, status=200, **options):
        code, value, headers = self.org(method, rest, body, **options)
        self.assertEqual(code, status, value)
        self.assertEqual(headers['Content-Type'], 'application/json')
        return value

    def get(self, rest, **options):
        return self.ok('GET', rest, **options)

    def problem(self, status, method, path, body=None, **options):
        code, value, headers = self.call(method, path, body, **options)
        self.assertEqual(code, status, value)
        self.assertEqual(headers['Content-Type'], 'application/problem+json')
        self.assertEqual(set(value) - {'currentVersion', 'code'}, {'type', 'title', 'status', 'detail'})
        self.assertEqual(value['status'], status)
        return value

    def org_problem(self, status, method, rest, body=None, org=WORKS, **options):
        return self.problem(status, method, f'/api/v1/orgs/{org}{rest}', body, **options)

    def identifiers(self, query=''):
        return [item['identifier'] for item in self.get('/issues' + query)['items']]

    def issue(self, ref, org=WORKS):
        return self.get(f'/issues/{ref}', org=org)

    def activity_item(self, identifier):
        return next(i for i in self.get('/issues?includeArchived=true&limit=100')['items']
                    if i['identifier'] == identifier)

    def start_device(self):
        code, value, _ = self.call('POST', '/user_management/authorize/device', token=None,
                                   form=dict(client_id=CLIENT_ID))
        self.assertEqual(code, 200, value)
        return value

    def poll(self, device_code, client_id=CLIENT_ID):
        return self.call('POST', '/user_management/authenticate', token=None,
                         form=dict(grant_type=NETT.DEVICE_GRANT, device_code=device_code, client_id=client_id))[:2]

    def upload(self, kind='issueCommentDraft', data=None, content_type='image/png', key=None, **target):
        data = data or NETT.fixture_png(16, 16)
        body = dict(requestKey=key or str(uuid.uuid4()), target=dict(kind=kind, **target), fileName='sketch.png',
                    contentType=content_type, sizeBytes=len(data), sha256=hashlib.sha256(data).hexdigest())
        return body, data

    def transfer(self, state, data, content_type='image/png', token=None):
        url = urlsplit(state['upload']['uploadUrl'])
        self.assertEqual(f'{url.scheme}://{url.netloc}', self.server.origin)
        return self.call('PUT', f'{url.path}?{url.query}', raw=data, token=token,
                         headers={'Content-Type': content_type})


class CommandLineTests(unittest.TestCase):
    def test_refuses_hosts_that_are_not_numeric_loopback(self):
        for host in ('0.0.0.0', 'localhost', '10.0.0.1', '::', 'example.test'):
            with self.assertRaises(ValueError, msg=host):
                NETT.FixtureServer(host=host, port=0)
        result = subprocess.run([sys.executable, '-I', str(SERVER_PATH), '--host', '0.0.0.0', '--port', '0'],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, '')
        self.assertIn('loopback', result.stderr)

    def test_prints_exactly_one_ready_line_and_serves(self):
        process = subprocess.Popen([sys.executable, '-I', str(SERVER_PATH), '--port', '0', '--access-token', 'cli'],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            line = process.stdout.readline()
            match = re.fullmatch(r'NETT fixture listening on (http://127\.0\.0\.1:(\d+))\n', line)
            self.assertIsNotNone(match, line)
            status, value, _ = call(match[1], 'GET', '/api/v1/me', token='cli')
            self.assertEqual((status, value), (200, {'isAuthenticated': True}))
        finally:
            process.terminate()
            out, err = process.communicate(timeout=10)
        self.assertEqual(out, '')
        self.assertNotIn('cli', err)


class DeviceFlowTests(FixtureCase):
    def test_device_flow_issues_tokens_the_api_accepts_and_refresh_rotates(self):
        device = self.start_device()
        self.assertEqual(set(device), {'device_code', 'user_code', 'verification_uri', 'verification_uri_complete',
                                       'expires_in', 'interval'})
        self.assertRegex(device['user_code'], r'^[A-Z]{4}-[A-Z]{4}$')
        self.assertEqual(device['verification_uri'], self.server.origin + '/device')
        self.assertEqual(device['verification_uri_complete'],
                         f"{self.server.origin}/device?user_code={device['user_code']}")
        self.assertEqual((device['expires_in'], device['interval']), (300, 1))
        status, pending = self.poll(device['device_code'])
        self.assertEqual((status, pending['error']), (400, 'authorization_pending'))
        self.assertEqual(set(pending), {'error', 'error_description'})
        self.assertEqual(self.poll(device['device_code'])[1]['error'], 'slow_down')
        status, page, headers = self.call('GET', '/device?user_code=' + device['user_code'], token=None)
        self.assertEqual(status, 200)
        logged = self.call('GET', '/__fixture/requests', token=None)[1][-1]
        self.assertEqual(logged['query'], dict(user_code=['REDACTED']), 'the approval code is not logged')
        self.assertTrue(headers['Content-Type'].startswith('text/html'))
        self.assertIn(device['user_code'].encode(), page)
        status, value, _ = self.call('POST', '/device/approve', dict(user_code=device['user_code']), token=None)
        self.assertEqual((status, value['status']), (200, 'approved'))
        time.sleep(0.85)
        status, tokens = self.poll(device['device_code'])
        self.assertEqual(status, 200, tokens)
        self.assertEqual(tokens['user'], dict(id='user_fixture_ada', email='ada@example.com', first_name='Ada',
                                              last_name='Fixture'))
        self.assertIsNone(tokens['organization_id'])
        header, claims, signature = tokens['access_token'].split('.')
        claims = json.loads(base64.urlsafe_b64decode(claims + '=' * (-len(claims) % 4)))
        self.assertEqual(set(claims), {'sub', 'client_id', 'iat', 'exp'})
        self.assertEqual((claims['sub'], claims['client_id'], claims['exp'] - claims['iat']),
                         ('user_fixture_ada', CLIENT_ID, 3600))
        self.assertTrue(header and signature)
        self.assertEqual(self.poll(device['device_code'])[1]['error'], 'invalid_grant', 'redeemable once')
        self.assertEqual(self.call('GET', '/api/v1/me', token=tokens['access_token'])[0], 200)
        refresh = dict(grant_type='refresh_token', refresh_token=tokens['refresh_token'], client_id=CLIENT_ID)
        status, rotated, _ = self.call('POST', '/user_management/authenticate', refresh, token=None)
        self.assertEqual(status, 200, rotated)
        self.assertNotEqual(rotated['access_token'], tokens['access_token'])
        self.assertNotEqual(rotated['refresh_token'], tokens['refresh_token'])
        self.assertEqual(self.call('GET', '/api/v1/auth/orgs', token=rotated['access_token'])[0], 200)
        status, value, _ = self.call('POST', '/user_management/authenticate', token=None, form=refresh)
        self.assertEqual((status, value['error']), (400, 'invalid_grant'))
        status, value, _ = self.call('POST', '/user_management/authorize/device', token=None, form=dict(client_id=''))
        self.assertEqual((status, value['error']), (400, 'invalid_request'))

    def test_device_codes_can_be_denied_or_expire_and_are_bound_to_the_client(self):
        denied = self.start_device()
        status, value, _ = self.call('POST', '/device/deny', token=None, form=dict(user_code=denied['user_code']))
        self.assertEqual((status, value['status']), (200, 'denied'))
        self.assertEqual(self.poll(denied['device_code'])[1]['error'], 'access_denied')
        self.problem(409, 'POST', '/device/approve', dict(user_code=denied['user_code']), token=None)
        expired = self.start_device()
        self.server.authority.devices[expired['device_code']]['expires'] = time.monotonic() - 1
        self.assertEqual(self.poll(expired['device_code'])[1]['error'], 'expired_token')
        other = self.start_device()
        self.assertEqual(self.poll(other['device_code'], 'other-fixture-client')[1]['error'],
                         'invalid_grant')
        status, value, _ = self.call('POST', '/user_management/authenticate', token=None,
                                     form=dict(grant_type='password', client_id=CLIENT_ID))
        self.assertEqual((status, value['error']), (400, 'unsupported_grant_type'))
        self.problem(404, 'POST', '/device/approve', dict(user_code='ZZZZ-ZZZZ'), token=None)


class AutoApproveTests(FixtureCase):
    options = dict(auto_approve=True)

    def test_the_second_poll_approves_without_slow_down(self):
        device = self.start_device()
        self.assertEqual(self.poll(device['device_code'])[1]['error'], 'authorization_pending')
        status, tokens = self.poll(device['device_code'])
        self.assertEqual(status, 200, tokens)
        self.assertEqual(self.call('GET', '/api/v1/me', token=tokens['access_token'])[0], 200)


class AuthTests(FixtureCase):
    def test_every_api_route_needs_a_bearer(self):
        for token in (None, 'wrong', ''):
            self.problem(401, 'GET', '/api/v1/me', token=token)
            self.org_problem(401, 'GET', '/issues', token=token)
        self.problem(401, 'GET', '/api/v1/me', headers={'Authorization': f'Basic {TOKEN}'}, token=None)
        self.problem(401, 'GET', '/api/v1/nowhere', token=None)
        self.problem(404, 'GET', '/api/v1/nowhere')
        self.assertEqual(self.call('GET', '/api/v1/me')[:2], (200, {'isAuthenticated': True}))

    def test_auth_context_needs_x_org_id_with_several_orgs(self):
        status, context, _ = self.call('GET', '/api/v1/auth/context')
        self.assertEqual(status, 200)
        self.assertEqual(context['userId'], ADA)
        self.assertEqual(context['user']['email'], 'ada@example.com')
        self.assertIsNone(context['currentOrgId'])
        self.assertIsNone(context['currentOrg'])
        self.assertEqual([o['name'] for o in context['organisations']],
                         ['Fixture Works', 'Second Fixture Org', 'Ada Fixture'])
        self.assertEqual(context['accessibleOrgIds'], [WORKS, SECOND, PERSONAL])
        context = self.call('GET', '/api/v1/auth/context', headers={'x-org-id': WORKS.upper()})[1]
        self.assertEqual((context['currentOrgId'], context['currentOrg']['name']), (WORKS, 'Fixture Works'))
        self.assertIn('issues.view', context['permissions'])
        for header in (OUTSIDE, 'not-a-uuid'):
            self.assertIsNone(self.call('GET', '/api/v1/auth/context', headers={'x-org-id': header})[1]['currentOrgId'])
        orgs = self.call('GET', '/api/v1/auth/orgs')[1]
        self.assertEqual([(o['name'], o['isPersonalOrg'], o['featurePacks']) for o in orgs],
                         [('Fixture Works', False, ['issue-tracking']), ('Second Fixture Org', False, ['issue-tracking']),
                          ('Ada Fixture', True, [])])
        self.assertEqual(orgs[0]['role'], 'owner')


class SingleOrgTests(FixtureCase):
    options = dict(single_org=True)

    def test_the_sole_org_is_current_without_a_header(self):
        context = self.call('GET', '/api/v1/auth/context')[1]
        self.assertEqual(context['currentOrgId'], WORKS)
        self.assertEqual(context['accessibleOrgIds'], [WORKS])
        self.org_problem(404, 'GET', '/issues', org=SECOND)
        self.org_problem(404, 'GET', '/issues', org=PERSONAL)


class ScopeTests(FixtureCase):
    def test_org_routes_check_membership_header_and_pack(self):
        self.org_problem(403, 'GET', '/issues', headers={'x-org-id': SECOND})
        self.assertEqual(self.org('GET', '/issues', headers={'x-org-id': WORKS.upper()})[0], 200)
        for org in (NETT.uid('org/unknown'), OUTSIDE, 'not-a-uuid'):
            self.org_problem(404, 'GET', '/issues', org=org)
        for rest in ('/issues', '/issues/OPS-1', '/issue-teams', '/issue-cycles', '/mention-candidates?kind=issue'):
            self.org_problem(404, 'GET', rest, org=PERSONAL)
        self.assertEqual(self.org('GET', '/members', org=PERSONAL)[0], 200)

    def test_unknown_routes_methods_and_query_parameters(self):
        self.problem(404, 'GET', '/nowhere', token=None)
        self.org_problem(404, 'GET', '/nowhere')
        problem = self.org_problem(405, 'DELETE', '/issues/OPS-1')
        self.assertIn('DELETE', problem['detail'])
        code, _, headers = self.org('DELETE', '/issues/OPS-1')
        self.assertEqual((code, headers['Allow']), (405, 'GET, PATCH'))
        self.org_problem(400, 'GET', '/issues?subscribed=true')
        self.org_problem(415, 'POST', '/issues', raw=b'{}', headers={'Content-Type': 'text/plain'})


class TeamTests(FixtureCase):
    def test_teams_by_key_and_uuid_with_states_and_scales(self):
        teams = self.get('/issue-teams')
        self.assertEqual([t['key'] for t in teams['items']], ['OPS', 'WEB'])
        first = self.get('/issue-teams?limit=1')
        self.assertEqual([t['key'] for t in first['items']], ['OPS'])
        second = self.get('/issue-teams?limit=1&cursor=' + first['nextCursor'])
        self.assertEqual(([t['key'] for t in second['items']], second['nextCursor']), (['WEB'], None))
        self.org_problem(400, 'GET', '/issue-teams?limit=101')
        ops = self.get('/issue-teams/ops')
        self.assertEqual(self.get(f'/issue-teams/{OPS}'), ops)
        self.assertEqual((ops['id'], ops['name'], ops['estimateScale']), (OPS, 'Operations', 'fibonacci'))
        self.assertEqual(self.get('/issue-teams/WEB')['estimateScale'], 'notUsed')
        self.assertEqual([(s['name'], s['type']) for s in ops['states']], list(zip(NETT.STATE_NAMES, NETT.STATE_TYPES)))
        self.assertEqual(ops['defaultStateId'], state('OPS', 'backlog'))
        states = self.get(f'/issue-teams/{OPS}/workflow-states')['items']
        self.assertEqual([s['id'] for s in states], [s['id'] for s in ops['states']])
        self.org_problem(400, 'GET', '/issue-teams/OPS/workflow-states')
        self.org_problem(404, 'GET', f'/issue-teams/{NETT.uid("nope")}/workflow-states')
        self.org_problem(404, 'GET', '/issue-teams/NOPE')


class IssueListTests(FixtureCase):
    def test_default_order_and_filters(self):
        everything = ['OPS-5', 'WEB-2', 'WEB-1', 'OPS-4', 'OPS-3', 'OPS-2', 'OPS-1']
        self.assertEqual(self.identifiers(), everything)
        self.assertEqual(self.identifiers('?includeArchived=true'), everything + ['WEB-3'])
        cases = {
            '?team=web': ['WEB-2', 'WEB-1'], f'?team={WEB}': ['WEB-2', 'WEB-1'],
            '?stateType=started': ['WEB-1', 'OPS-2', 'OPS-1'],
            '?stateType=started&stateType=triage': ['OPS-5', 'WEB-1', 'OPS-2', 'OPS-1'],
            f"?state={state('OPS', 'unstarted')}": ['OPS-4'],
            '?assignee=me': ['WEB-1', 'OPS-1'], '?assignee=none': ['OPS-5', 'OPS-4'],
            f'?assignee={LINUS}&assignee=none': ['OPS-5', 'WEB-2', 'OPS-4'],
            '?parent=none': ['OPS-5', 'WEB-2', 'WEB-1', 'OPS-2', 'OPS-1'],
            '?parent=ops-2': ['OPS-4', 'OPS-3'], '?parent=OPS-99': [],
            f'?project={HARDENING}': ['OPS-4', 'OPS-3', 'OPS-2'], f'?project={LAUNCH}': ['WEB-1', 'OPS-1'],
            f'?label={BUG}': ['WEB-1', 'OPS-1'], '?q=BACKUP': ['OPS-1'], '?type=tracker': ['OPS-2'],
            '?priority=1&priority=4': ['WEB-1', 'OPS-4'], f'?creator={GRACE}': ['OPS-5'],
            '?creator=me&team=OPS&parent=none': ['OPS-2', 'OPS-1'],
            '?order=priority': ['WEB-1', 'OPS-1', 'OPS-3', 'OPS-2', 'OPS-4', 'OPS-5', 'WEB-2'],
            '?order=manual&team=OPS': ['OPS-1', 'OPS-2', 'OPS-3', 'OPS-4', 'OPS-5'],
        }
        for query, expected in cases.items():
            self.assertEqual(self.identifiers(query), expected, query)
        parent = self.issue('OPS-2')['id']
        self.assertEqual(self.identifiers(f'?parent={parent}'), ['OPS-4', 'OPS-3'])
        item = self.get('/issues')['items'][-1]
        self.assertEqual(item['identifier'], 'OPS-1')
        self.assertEqual((item['project']['name'], item['milestone']['name']), ('Launch', 'Beta'))
        self.assertEqual(item['lastActivity']['kind'], 'comment.created')
        self.assertEqual(item['lastActivityAt'], item['lastActivity']['occurredAt'])
        self.assertEqual(self.activity_item('OPS-4')['effectiveProjectId'], HARDENING)
        self.assertIsNone(self.activity_item('OPS-5')['lastActivity'], 'creation is still the latest activity')
        for query in ('?limit=0', '?limit=101', '?limit=x', '?q=', '?q=' + 'x' * 201, '?stateType=open',
                      '?assignee=someone', '?priority=5', '?order=newest', '?order=manual', '?type=epic',
                      '?project=Launch', '?includeArchived=maybe', '?label=bug'):
            self.org_problem(400, 'GET', '/issues' + query)
        self.org_problem(404, 'GET', '/issues?team=NOPE')

    def test_cursor_paging_is_bound_to_filters_and_order(self):
        pages, cursor = [], None
        while True:
            value = self.get('/issues?limit=3' + (f'&cursor={cursor}' if cursor else ''))
            pages.append([i['identifier'] for i in value['items']])
            cursor = value['nextCursor']
            if cursor is None:
                break
        self.assertEqual([len(p) for p in pages], [3, 3, 1])
        self.assertEqual(sum(pages, []), self.identifiers())
        cursor = self.get('/issues?limit=3')['nextCursor']
        self.assertEqual(len(self.get(f'/issues?limit=10&cursor={cursor}')['items']), 4, 'limit may change')
        for query in (f'?team=OPS&cursor={cursor}', f'?order=activity&cursor={cursor}', '?cursor=bm90LWEtY3Vyc29y'):
            self.org_problem(400, 'GET', '/issues' + query)

    def test_activity_order_and_incremental_reads_after_a_change_elsewhere(self):
        self.assertEqual(self.identifiers('?order=activity')[:2], ['OPS-1', 'OPS-5'])
        status, touched, _ = self.call('POST', '/__fixture/touch', dict(org=WORKS, issue='web-2'), token=None)
        self.assertEqual(status, 200, touched)
        self.assertEqual(touched['identifier'], 'WEB-2')
        since = touched['lastActivityAt']
        self.assertEqual(self.identifiers('?order=activity&' + urlencode(dict(activitySince=since))), ['WEB-2'])
        offset = since.replace('Z', '+00:00')
        self.assertEqual(self.identifiers('?' + urlencode(dict(order='activity', activitySince=offset))), ['WEB-2'])
        self.assertEqual(self.identifiers('?order=activity')[0], 'WEB-2')
        item = self.activity_item('WEB-2')
        self.assertEqual((item['lastActivityAt'], item['updatedAt'], item['version']), (since, since, 1))
        self.call('POST', '/__fixture/touch', dict(org=SECOND, issue='OPS-1'), token=None)
        self.assertEqual(self.identifiers('?' + urlencode(dict(activitySince=since))), ['WEB-2'])
        for value in ('2026-01-01T00:00:00', '2026-01-01', 'yesterday'):
            self.org_problem(400, 'GET', '/issues?' + urlencode(dict(order='activity', activitySince=value)))
        self.problem(404, 'POST', '/__fixture/touch', dict(org=WORKS, issue='OPS-99'), token=None)

    def test_groups_by_state_and_state_type_include_empty_groups(self):
        groups = self.get('/issues/groups?groupBy=state&team=WEB')
        self.assertEqual(groups['groupBy'], 'state')
        self.assertEqual([g['stateId'] for g in groups['groups']], [state('WEB', k) for k in NETT.STATE_TYPES])
        self.assertEqual([g['count'] for g in groups['groups']], [0, 1, 0, 1, 0, 0, 0])
        self.assertEqual([[i['identifier'] for i in g['items']] for g in groups['groups']],
                         [[], ['WEB-2'], [], ['WEB-1'], [], [], []])
        by_type = self.get('/issues/groups?groupBy=stateType&limitPerGroup=1')['groups']
        self.assertEqual([(g['stateId'], g['stateType'], g['count']) for g in by_type],
                         [(None, kind, count) for kind, count in zip(NETT.STATE_TYPES, (1, 1, 1, 3, 1, 0, 0))])
        started = by_type[3]
        self.assertEqual([i['identifier'] for i in started['items']], ['WEB-1'])
        self.assertEqual((by_type[4]['items'], by_type[4]['nextCursor']), ([], None), 'terminal groups are counts only')
        rest = self.get('/issues?stateType=started&cursor=' + started['nextCursor'])
        self.assertEqual(([i['identifier'] for i in rest['items']], rest['nextCursor']), (['OPS-2', 'OPS-1'], None))
        self.org_problem(400, 'GET', '/issues?cursor=' + started['nextCursor'])
        counts = self.get('/issues/groups?groupBy=stateType&limitPerGroup=0&team=OPS')['groups']
        self.assertEqual(([g['count'] for g in counts], [g['items'] for g in counts]), ([1, 0, 1, 2, 1, 0, 0], [[]] * 7))
        for query in ('?groupBy=state', '?groupBy=label', '?groupBy=stateType&limitPerGroup=26',
                      '?groupBy=stateType&limit=5', '?groupBy=stateType&activitySince=2026-01-01T00:00:00Z'):
            self.org_problem(400, 'GET', '/issues/groups' + query)

    def test_detail_by_identifier_or_uuid_and_colliding_identifiers(self):
        detail = self.issue('ops-1')
        self.assertEqual(self.issue(detail['id']), detail)
        self.assertEqual((detail['identifier'], detail['version'], detail['createdBy']['displayName']),
                         ('OPS-1', 1, 'Ada Fixture'))
        self.assertNotIn('lastActivityAt', detail)
        self.assertEqual([(m['mediaId'], m['kind'], m['contentType']) for m in detail['media']],
                         [(IMAGE, 'image', 'image/png'), (NOTES, 'file', 'text/markdown')])
        self.assertIn(f'nett-media:{IMAGE}', detail['description'])
        self.assertEqual([(r['identifier'], r['kind']) for r in detail['relations']['items']], [('WEB-1', 'relatedTo')])
        self.assertEqual((detail['project']['name'], detail['milestone']['name']), ('Launch', 'Beta'))
        tracker = self.issue('OPS-2')
        self.assertEqual([c['identifier'] for c in tracker['children']['items']], ['OPS-3', 'OPS-4'])
        self.assertEqual((tracker['progress']['done'], tracker['progress']['total']), (1, 2))
        self.assertEqual(self.issue('OPS-3')['parent']['identifier'], 'OPS-2')
        other = self.issue('OPS-1', org=SECOND)
        self.assertNotEqual(other['id'], detail['id'])
        self.assertEqual(other['title'], 'Plan the second office move')
        self.org_problem(404, 'GET', f"/issues/{detail['id']}", org=SECOND)
        self.org_problem(404, 'GET', '/issues/OPS-99')
        self.org_problem(404, 'GET', '/issues/not-an-issue')


class IssueWriteTests(FixtureCase):
    def test_create_has_a_closed_body_and_checks_values(self):
        beta = NETT.uid(f'{LAUNCH}/milestone/Beta')
        cycle = NETT.uid(f'{OPS}/cycle/3')
        for body in (dict(teamId=OPS, title='Extra', summary='Not a NETT field'), dict(teamId=OPS),
                     dict(teamId=OPS, title='Two\nlines'), dict(teamId=NETT.uid('nope'), title='Unknown team'),
                     dict(teamId=OPS, title='Numbered', number=5), dict(teamId=OPS, title='Odd', estimate=4),
                     dict(teamId=WEB, title='Unscaled', estimate=1), dict(teamId=OPS, title='Old', assigneeId=LINUS),
                     dict(teamId=OPS, title='Nested', type='tracker', parentId='OPS-2'),
                     dict(teamId=OPS, title='Elsewhere', projectId=HARDENING, milestoneId=beta),
                     dict(teamId=OPS, title='Late', dueDate='19/11/2026'), dict(teamId=OPS, title='Loud', priority=5),
                     dict(teamId=OPS, title='Legacy', labelIds=[NETT.uid('works/label/legacy')])):
            self.org_problem(400, 'POST', '/issues', body)
        created = self.ok('POST', '/issues', dict(teamId=OPS, title='  Check the restore drill  ', description='Notes.',
                                                  assigneeId=GRACE, priority=2, estimate=5, labelIds=[DOCS],
                                                  projectId=LAUNCH, milestoneId=beta, cycleId=cycle,
                                                  dueDate='2026-12-01', parentId=None), status=201)
        self.assertEqual((created['identifier'], created['version'], created['title']),
                         ('OPS-6', 1, 'Check the restore drill'))
        self.assertEqual((created['state']['type'], created['assignee']['userId'], created['estimate']),
                         ('backlog', GRACE, 5))
        self.assertEqual(([l['name'] for l in created['labels']], created['milestone']['name'], created['cycle']['number']),
                         (['docs'], 'Beta', 3))
        self.assertEqual(created['createdBy']['userId'], ADA)
        child = self.ok('POST', '/issues', dict(teamId=OPS, title='Child', parentId='ops-2',
                                                stateId=state('OPS', 'started')), status=201)
        self.assertEqual((child['identifier'], child['parentId'], child['effectiveProjectId']),
                         ('OPS-7', self.issue('OPS-2')['id'], HARDENING))
        self.assertIsNotNone(child['startedAt'])
        self.assertIn('OPS-7', [c['identifier'] for c in self.issue('OPS-2')['children']['items']])
        listed = self.activity_item('OPS-6')
        self.assertEqual((listed['lastActivityAt'], listed['lastActivity']), (listed['createdAt'], None))

    def test_patch_is_sparse_versioned_and_null_clears(self):
        detail = self.issue('OPS-1')
        for body in (dict(title='No version'), dict(version='1', title='Text version'),
                     dict(version=1, summary='Unknown'), dict(version=1), dict(version=1, archived=True)):
            self.org_problem(400, 'PATCH', '/issues/OPS-1', body)
        conflict = self.org_problem(409, 'PATCH', '/issues/OPS-1', dict(version=7, title='Stale'))
        self.assertEqual(conflict['currentVersion'], 1)
        saved = self.ok('PATCH', '/issues/ops-1', dict(version=1, title='Restore the backup check', assigneeId=None,
                                                       labelIds=[DOCS]))
        self.assertEqual((saved['version'], saved['title'], saved['assignee']), (2, 'Restore the backup check', None))
        self.assertEqual([l['id'] for l in saved['labels']], [DOCS])
        self.assertEqual((saved['priority'], saved['estimate']), (detail['priority'], detail['estimate']), 'sparse')
        self.assertGreater(saved['updatedAt'], detail['updatedAt'])
        saved = self.ok('PATCH', '/issues/OPS-1', dict(version=2, labelIds=[], stateId=state('OPS', 'completed')))
        self.assertEqual((saved['labels'], saved['state']['type']), ([], 'completed'))
        self.assertIsNotNone(saved['completedAt'])
        saved = self.ok('PATCH', '/issues/OPS-1', dict(version=3, projectId=HARDENING))
        self.assertEqual((saved['projectId'], saved['milestoneId']), (HARDENING, None), 'milestone follows its project')
        item = self.activity_item('OPS-1')
        self.assertEqual((item['lastActivity']['kind'], item['lastActivityAt']), ('issue.changed', saved['updatedAt']))
        activity = self.get('/issues/OPS-1/activity')['items']
        self.assertEqual(activity[0]['kind'], 'issue.changed')
        self.assertEqual(self.identifiers('?order=activity')[0], 'OPS-1')
        self.org_problem(409, 'PATCH', '/issues/OPS-2', dict(version=1, type='issue'))
        self.org_problem(409, 'PATCH', '/issues/WEB-3', dict(version=2, title='Archived'))


class DiscussionTests(FixtureCase):
    def test_comments_roots_replies_and_posts(self):
        roots = self.get('/issues/OPS-1/comments')['items']
        self.assertEqual([r['id'] for r in roots], [FIRST_ROOT, NETT.uid('comment/ops-1/second')])
        self.assertEqual((roots[0]['replyCount'], roots[0]['hasReplies'], roots[0]['actualAuthor']['displayName']),
                         (2, True, 'Grace Fixture'))
        self.assertEqual([m['mediaId'] for m in roots[1]['media']], [TIMING])
        replies = self.get(f'/issues/OPS-1/comments/{FIRST_ROOT}/replies')['items']
        self.assertEqual([r['actualAuthor']['displayName'] for r in replies], ['Ada Fixture', 'Grace Fixture'])
        first = self.get('/issues/OPS-1/comments?limit=1')
        self.assertEqual(len(self.get('/issues/OPS-1/comments?limit=1&cursor=' + first['nextCursor'])['items']), 1)
        self.org_problem(400, 'GET', '/issues/OPS-1/comments?limit=51')
        self.org_problem(404, 'GET', f"/issues/OPS-1/comments/{replies[0]['id']}/replies")
        before = self.activity_item('OPS-1')
        for body in (dict(body='Hi', extra=True), dict(body='   '), dict(body='x' * 50001), dict(),
                     dict(body='Reply to a reply', parentCommentId=replies[0]['id']),
                     dict(body='No draft', mediaDraftId=str(uuid.uuid4())), dict(body='Imported', provenance={})):
            self.org_problem(400, 'POST', '/issues/OPS-1/comments', body)
        posted = self.ok('POST', '/issues/OPS-1/comments', dict(body='Fixed on staging.\r\nWatching tonight.'),
                         status=201)
        self.assertEqual((posted['outcome'], posted['body'], posted['parentCommentId']),
                         ('created', 'Fixed on staging.\nWatching tonight.', None))
        after = self.activity_item('OPS-1')
        self.assertEqual((after['version'], after['commentCount']), (before['version'], before['commentCount'] + 1))
        self.assertEqual((after['lastActivityAt'], after['lastActivity']['kind']), (posted['createdAt'], 'comment.created'))
        reply = self.ok('POST', '/issues/OPS-1/comments', dict(body='Thanks', parentCommentId=posted['id']), status=201)
        self.assertEqual(reply['parentCommentId'], posted['id'])
        self.assertEqual(self.get(f"/issues/OPS-1/comments/{posted['id']}/replies")['items'][0]['id'], reply['id'])
        self.assertEqual(self.get('/issues/OPS-1/comments')['items'][-1]['replyCount'], 1)
        self.org_problem(400, 'POST', '/issues/OPS-1/comments',
                         dict(body='Resolve a reply', parentCommentId=posted['id'], resolved=True))
        resolved = self.ok('POST', '/issues/OPS-2/comments', dict(body='Done', resolved=True), status=201)
        self.assertEqual((resolved['resolved'], resolved['resolvedBy']['userId']), (True, ADA))
        self.assertEqual(self.get('/issues/OPS-1/activity')['items'][0]['comment']['id'], reply['id'])

    def test_relations_activity_pull_requests_and_images(self):
        relations = self.get('/issues/OPS-1/relations')['items']
        self.assertEqual([(r['identifier'], r['kind']) for r in relations], [('WEB-1', 'relatedTo')])
        self.assertEqual(self.get('/issues/OPS-1/relations?kind=blocks')['items'], [])
        self.org_problem(400, 'GET', '/issues/OPS-1/relations?kind=parent')
        activity = self.get('/issues/OPS-1/activity')
        kinds = [e['kind'] for e in activity['items']]
        self.assertEqual(kinds[0], 'comment.created')
        self.assertEqual(kinds[-1], 'issue.created')
        self.assertEqual(kinds.count('decision.logged'), 2)
        self.assertEqual(activity['items'][0]['comment']['issueId'], self.issue('OPS-1')['id'])
        self.assertEqual({d['status'] for d in activity['references']['decisions']}, {'accepted', 'superseded'})
        self.assertIn(GRACE, [p['userId'] for p in activity['references']['people']])
        self.assertEqual(activity['references']['lists'][0]['name'], 'Checklist')
        pulls = self.get('/issues/OPS-1/pull-requests')['items']
        self.assertEqual([(p['number'], p['state'], p['referenceKind']) for p in pulls], [(42, 'open', 'closes')])
        self.assertEqual(self.issue('OPS-1')['pullRequestSummary']['closingPending'], 1)
        self.assertEqual(self.get('/issues/OPS-2/pull-requests')['items'], [])
        images = self.get('/issues/OPS-1/images')
        self.assertEqual([(i['media']['mediaId'], i['source']['kind'], i['altText']) for i in images['items']],
                         [(IMAGE, 'description', 'Backup dashboard'), (TIMING, 'comment', 'Restore timing')])
        self.assertEqual((images['issueId'], images['previousCursor'], images['nextCursor']),
                         (self.issue('OPS-1')['id'], None, None))
        first = self.get('/issues/OPS-1/images?limit=1')
        self.assertEqual(self.get('/issues/OPS-1/images?limit=1&cursor=' + first['nextCursor'])['items'][0]['media']['mediaId'],
                         TIMING)
        around = self.get(f'/issues/OPS-1/images?limit=1&around={TIMING}')
        self.assertEqual([i['media']['mediaId'] for i in around['items']], [TIMING])
        self.assertIsNotNone(around['previousCursor'])
        self.assertEqual(self.get('/issues/OPS-1/images?limit=1&cursor=' + around['previousCursor'])['items'][0]
                         ['media']['mediaId'], IMAGE)
        self.org_problem(400, 'GET', f"/issues/OPS-1/images?around={TIMING}&cursor={first['nextCursor']}")
        self.org_problem(400, 'GET', '/issues/OPS-1/images?around=not-a-uuid')
        self.org_problem(404, 'GET', f'/issues/OPS-1/images?around={NOTES}')
        self.org_problem(400, 'GET', '/issues/OPS-1/images?limit=51')


class MediaTests(FixtureCase):
    def fetch(self, url, **options):
        parts = urlsplit(url)
        self.assertEqual(f'{parts.scheme}://{parts.netloc}', self.server.origin)
        return self.call('GET', f'{parts.path}?{parts.query}', **options)

    def test_media_read_grants_files_that_never_take_credentials(self):
        read = self.get(f'/issues/media/{IMAGE}')
        self.assertEqual((read['mediaId'], read['status'], read['media']['kind']), (IMAGE, 'ready', 'image'))
        grant = read['grant']
        self.assertEqual(grant['publicFileId'], read['media']['publicFileId'])
        self.assertTrue(grant['url'].startswith(f"{self.server.origin}/files/{grant['publicFileId']}?grant="))
        expires = NETT.parse_stamp(grant['expiresAtUtc']) - NETT.utc_now()
        self.assertTrue(240 < expires.total_seconds() <= 300)
        status, data, headers = self.fetch(grant['url'], token=None)
        self.assertEqual(status, 200)
        self.assertTrue(data.startswith(b'\x89PNG\r\n\x1a\n'))
        self.assertEqual((headers['Content-Type'], headers['Content-Length'], headers['X-Content-Type-Options']),
                         ('image/png', str(len(data)), 'nosniff'))
        self.assertEqual(len(data), read['media']['sizeBytes'])
        self.assertEqual(self.fetch(grant['url'])[0], 400, 'a bearer never goes to the file host')
        self.assertEqual(self.fetch(grant['url'], token=None, headers={'Cookie': 'session=1'})[0], 400)
        self.assertEqual(self.fetch(grant['url'] + 'x', token=None)[0], 403)
        notes = self.get(f'/issues/media/{NOTES}')
        status, data, headers = self.fetch(notes['grant']['url'], token=None)
        self.assertEqual((status, data, headers['Content-Type']), (200, NETT.FIXTURE_RUNBOOK, 'text/markdown'))
        token = urlsplit(notes['grant']['url']).query.split('=', 1)[1]
        self.server.fixture.grants[token] = (notes['media']['publicFileId'], NETT.utc_now())
        self.assertEqual(self.fetch(notes['grant']['url'], token=None)[0], 403, 'expired grant')
        self.org_problem(404, 'GET', f'/issues/media/{NETT.uid("nope")}')
        self.org_problem(404, 'GET', f'/issues/media/{IMAGE}', org=SECOND)

    def test_file_grants_need_a_matching_x_org_id(self):
        ids = [self.get(f'/issues/media/{m}')['media']['publicFileId'] for m in (IMAGE, NOTES)]
        self.org_problem(403, 'POST', '/files/grants', dict(publicFileIds=ids), org_header=False)
        self.org_problem(403, 'POST', '/files/grants', dict(publicFileIds=ids), headers={'x-org-id': SECOND})
        grants = self.ok('POST', '/files/grants', dict(publicFileIds=ids), headers={'x-org-id': WORKS.upper()})
        self.assertEqual([g['publicFileId'] for g in grants['items']], ids)
        self.assertEqual(self.fetch(grants['items'][1]['url'], token=None)[1], NETT.FIXTURE_RUNBOOK)
        org = {'x-org-id': WORKS}
        for ids_value in ([], ids[:1] * 2, [f'pf_{n}' for n in range(51)], 'pf_x', [7]):
            self.org_problem(400, 'POST', '/files/grants', dict(publicFileIds=ids_value), headers=org)
        self.org_problem(404, 'POST', '/files/grants', dict(publicFileIds=['pf_unknown']), headers=org)

    def test_upload_prepare_transfer_complete_and_attach_to_a_comment(self):
        issue_id = self.issue('OPS-1')['id']
        draft = str(uuid.uuid4())
        body, data = self.upload(issueId=issue_id, draftId=draft)
        prepared = self.ok('POST', '/issues/media/prepare', body)
        self.assertEqual((prepared['status'], prepared['version'], prepared['media']), ('prepared', 1, None))
        transfer = prepared['upload']
        self.assertEqual((transfer['method'], transfer['contentType'], transfer['sizeBytes']), ('PUT', 'image/png', len(data)))
        self.assertTrue(transfer['uploadUrl'].startswith(f"{self.server.origin}/u/{prepared['uploadId']}?grant="))
        self.assertEqual(self.ok('POST', '/issues/media/prepare', body), prepared, 'the same requestKey repeats')
        self.org_problem(409, 'POST', '/issues/media/prepare', dict(body, fileName='other.png'))
        for change in (dict(sha256=body['sha256'].upper()), dict(sizeBytes=0), dict(sizeBytes=25_000_001),
                       dict(contentType='application/zip'), dict(fileName='a/b.png'), dict(requestKey='key'),
                       dict(target=dict(kind='issueThing', issueId=issue_id)), dict(target=dict(kind='issueCommentDraft')),
                       dict(target=dict(kind='issueDescription')), dict(extra=1)):
            self.org_problem(400, 'POST', '/issues/media/prepare', dict(body, requestKey=str(uuid.uuid4())) | change)
        self.org_problem(409, 'POST', '/issues/media/from-upload', dict(uploadId=prepared['uploadId']))
        self.assertEqual(self.transfer(prepared, data, token=TOKEN)[0], 400, 'no bearer on the transfer URL')
        self.assertEqual(self.transfer(prepared, data, content_type='image/jpeg')[0], 400)
        self.assertEqual(self.transfer(prepared, data[:-1] + b'\x00')[0], 400, 'checksum')
        self.assertEqual(self.transfer(prepared, data + b'\x00')[0], 400, 'length')
        broken = dict(prepared, upload=dict(transfer, uploadUrl=transfer['uploadUrl'] + 'x'))
        self.assertEqual(self.transfer(broken, data)[0], 403)
        status, empty, headers = self.transfer(prepared, data)
        self.assertEqual((status, empty, headers['Content-Type']), (204, b'', None))
        self.assertEqual(self.transfer(prepared, data)[0], 409, 'the bytes go once')
        checking = self.ok('POST', '/issues/media/from-upload', dict(uploadId=prepared['uploadId']), status=202)
        self.assertEqual((checking['status'], checking['upload']), ('checking', None))
        self.ok('POST', '/issues/media/from-upload', dict(uploadId=prepared['uploadId']), status=202)
        ready = self.get(f"/issues/media/uploads/{prepared['uploadId']}")
        self.assertEqual((ready['status'], ready['media']['mediaId'], ready['media']['kind']),
                         ('ready', prepared['mediaId'], 'image'))
        self.assertEqual(self.ok('POST', '/issues/media/from-upload', dict(uploadId=prepared['uploadId']))['status'], 'ready')
        text = f"With a sketch\n\n![Sketch](nett-media:{prepared['mediaId']})"
        posted = self.ok('POST', '/issues/OPS-1/comments', dict(body=text, mediaDraftId=draft), status=201)
        self.assertEqual([m['mediaId'] for m in posted['media']], [prepared['mediaId']])
        self.assertEqual(posted['body'], text)
        self.org_problem(400, 'POST', '/issues/OPS-1/comments', dict(body='Again', mediaDraftId=draft))
        read = self.get(f"/issues/media/{prepared['mediaId']}")
        self.assertEqual(self.fetch(read['grant']['url'], token=None)[1], data)
        images = {i['media']['mediaId']: i for i in self.get('/issues/OPS-1/images')['items']}
        self.assertEqual((images[prepared['mediaId']]['altText'], images[prepared['mediaId']]['source']['commentId']),
                         ('Sketch', posted['id']))
        self.org_problem(404, 'GET', f"/issues/media/uploads/{prepared['uploadId']}", org=SECOND)

    def test_upload_to_a_new_issue_description_and_cancellation(self):
        draft = str(uuid.uuid4())
        body, data = self.upload('issueDescriptionDraft', draftId=draft)
        prepared = self.ok('POST', '/issues/media/prepare', body)
        self.transfer(prepared, data)
        self.ok('POST', '/issues/media/from-upload', dict(uploadId=prepared['uploadId']), status=202)
        self.get(f"/issues/media/uploads/{prepared['uploadId']}")
        created = self.ok('POST', '/issues', dict(teamId=WEB, title='With a picture', mediaDraftId=draft), status=201)
        self.assertEqual([m['mediaId'] for m in created['media']], [prepared['mediaId']])
        body, data = self.upload('issueDescriptionDraft', draftId=str(uuid.uuid4()))
        doomed = self.ok('POST', '/issues/media/prepare', body)
        self.transfer(doomed, data)
        stale = self.org_problem(409, 'PUT', f"/issues/media/{doomed['mediaId']}/upload", dict(version=9, cancel=True))
        self.assertEqual(stale['currentVersion'], 1)
        self.org_problem(400, 'PUT', f"/issues/media/{doomed['mediaId']}/upload", dict(version=1, cancel=False))
        cancelled = self.ok('PUT', f"/issues/media/{doomed['mediaId']}/upload", dict(version=1, cancel=True))
        self.assertEqual(cancelled, dict(mediaId=doomed['mediaId'], version=2, outcome='cancelled', retention='cleaned'))
        self.org_problem(409, 'POST', '/issues/media/from-upload', dict(uploadId=doomed['uploadId']))
        self.org_problem(409, 'PUT', f"/issues/media/{prepared['mediaId']}/upload", dict(version=3, cancel=True))
        self.org_problem(409, 'PUT', f'/issues/media/{IMAGE}/upload', dict(version=1, cancel=True))

    def test_a_rejected_status_read_leaves_the_upload_checking_and_unattached(self):
        path = '/issues/OPS-1'
        issue = self.get(path)
        body, data = self.upload('issueDescription', issueId=issue['id'])
        prepared = self.ok('POST', '/issues/media/prepare', body)
        self.assertEqual(self.transfer(prepared, data)[0], 204)
        complete = dict(uploadId=prepared['uploadId'])
        checking = self.ok('POST', '/issues/media/from-upload', complete, status=202)
        status_path = f"/issues/media/uploads/{prepared['uploadId']}"
        self.org_problem(400, 'GET', status_path + '?unexpected=1')
        self.assertEqual(self.ok('POST', '/issues/media/from-upload', complete, status=202), checking)
        self.assertEqual(self.get(path)['media'], issue['media'])
        self.assertEqual(self.get(status_path)['status'], 'ready')
        self.assertIn(prepared['mediaId'], [m['mediaId'] for m in self.get(path)['media']])

    def test_media_can_be_ready_when_its_completion_answers(self):
        status, value, _ = self.call('POST', '/__fixture/config', dict(mediaReadyAtOnce=True), token=None)
        self.assertEqual(status, 200, value)
        body, data = self.upload(issueId=self.issue('OPS-1')['id'], draftId=str(uuid.uuid4()))
        prepared = self.ok('POST', '/issues/media/prepare', body)
        self.transfer(prepared, data)
        ready = self.ok('POST', '/issues/media/from-upload', dict(uploadId=prepared['uploadId']), status=200)
        self.assertEqual((ready['status'], ready['media']['mediaId']), ('ready', prepared['mediaId']))
        self.assertEqual(self.call('POST', '/__fixture/config', dict(mediaReadyAtOnce=1), token=None)[0], 400)


class FileCase(FixtureCase):
    """Shared steps for the prepare, PUT and complete file flows."""

    def declaration(self, data, name='notes.md', content_type='text/markdown', key=None):
        return dict(requestKey=key or str(uuid.uuid4()), fileName=name, contentType=content_type,
                    sizeBytes=len(data), sha256=hashlib.sha256(data).hexdigest())

    def put(self, upload, data, content_type=None, token=None, headers=None):
        url = urlsplit(upload['uploadUrl'])
        self.assertEqual(f'{url.scheme}://{url.netloc}', self.server.origin)
        return self.call('PUT', f'{url.path}?{url.query}', raw=data, token=token,
                         headers={'Content-Type': content_type or upload['contentType'], **(headers or {})})

    def fetch(self, url, **options):
        parts = urlsplit(url)
        self.assertEqual(f'{parts.scheme}://{parts.netloc}', self.server.origin)
        return self.call('GET', f'{parts.path}?{parts.query}', token=None, **options)

    def assert_download(self, grant, data, content_type):
        self.assertEqual(set(grant), {'publicFileId', 'url', 'expiresAtUtc'})
        status, body, headers = self.fetch(grant['url'])
        self.assertEqual((status, body), (200, data))
        self.assertEqual((headers['Content-Type'], headers['Content-Length'], headers['X-Content-Type-Options'],
                          headers['Content-Disposition']), (content_type, str(len(data)), 'nosniff', 'attachment'))

    def grants(self, *public_ids, **options):
        return self.ok('POST', '/files/grants', dict(publicFileIds=list(public_ids)), headers={'x-org-id': WORKS},
                       **options)['items']

    def configure(self, **settings):
        status, value, _ = self.call('POST', '/__fixture/config', settings, token=None)
        self.assertEqual(status, 200, value)


class AttachmentTests(FileCase):
    options = dict(served=('attachments',))

    def test_attachments_list_in_pages_and_download_through_file_grants(self):
        listed = self.get('/issues/OPS-1/attachments')
        self.assertEqual([a['fileName'] for a in listed['items']], ['restore-log.txt', 'restore-chart.png'])
        self.assertIsNone(listed['nextCursor'])
        log, chart = listed['items']
        self.assertEqual(set(log), {'mediaId', 'version', 'kind', 'fileName', 'contentType', 'sizeBytes', 'sha256',
                                    'publicFileId', 'expiresAt', 'uploadId'})
        self.assertIsNone(log['uploadId'], 'a seeded file was never uploaded')
        self.assertEqual((log['kind'], chart['kind'], log['expiresAt'], log['version']), ('file', 'image', None, 1))
        first = self.get('/issues/OPS-1/attachments?limit=1')
        self.assertEqual([a['fileName'] for a in first['items']], ['restore-log.txt'])
        rest = self.get('/issues/OPS-1/attachments?limit=1&cursor=' + first['nextCursor'])
        self.assertEqual(([a['fileName'] for a in rest['items']], rest['nextCursor']), (['restore-chart.png'], None))
        self.org_problem(400, 'GET', '/issues/OPS-2/attachments?cursor=' + first['nextCursor'])
        self.assertEqual(self.get('/issues/OPS-2/attachments'), dict(items=[], nextCursor=None))
        self.org_problem(404, 'GET', '/issues/OPS-1/attachments', org=PERSONAL)
        log_grant, chart_grant = self.grants(log['publicFileId'], chart['publicFileId'])
        self.assert_download(log_grant, b'Restore finished in 4 minutes.\n', 'text/plain')
        status, data, headers = self.fetch(chart_grant['url'])
        self.assertEqual((status, headers['Content-Type'], headers['Content-Disposition'], len(data)),
                         (200, 'image/png', 'attachment', chart['sizeBytes']))
        self.assertEqual(hashlib.sha256(data).hexdigest(), chart['sha256'])
        for extra in ({'Authorization': f'Bearer {TOKEN}'}, {'Cookie': 'session=1'}, {'x-org-id': WORKS}):
            self.assertEqual(self.fetch(log_grant['url'], headers=extra)[0], 400, extra)

    def test_prepare_put_once_complete_while_checking_and_replay(self):
        data = b'# Notes\n\nFixture notes.\n'
        body = self.declaration(data)
        upload = self.ok('POST', '/issues/OPS-1/attachments/prepare', body)
        self.assertEqual(set(upload), {'uploadId', 'uploadUrl', 'method', 'contentType', 'sizeBytes', 'expiresAtUtc'})
        self.assertEqual((upload['method'], upload['contentType'], upload['sizeBytes']),
                         ('PUT', 'text/markdown', len(data)))
        self.assertTrue(upload['uploadUrl'].startswith(f"{self.server.origin}/u/{upload['uploadId']}?grant="))
        self.assertEqual(self.ok('POST', '/issues/OPS-1/attachments/prepare', body), upload, 'an exact retry')
        self.org_problem(409, 'POST', '/issues/OPS-1/attachments/prepare', dict(body, fileName='other.md'))
        self.org_problem(409, 'POST', '/issues/OPS-2/attachments/prepare', body)  # The same key on another issue.
        for change, status in ((dict(requestKey='key'), 400), (dict(contentType='application/x-msdownload'), 400),
                               (dict(sizeBytes=0), 400), (dict(sizeBytes=25_000_001), 413),
                               (dict(sha256=body['sha256'].upper()), 400), (dict(fileName='a/b.md'), 400),
                               (dict(extra=1), 400)):
            self.org_problem(status, 'POST', '/issues/OPS-1/attachments/prepare',
                             dict(body, requestKey=str(uuid.uuid4())) | change)
        self.org_problem(404, 'POST', '/issues/OPS-99/attachments/prepare', self.declaration(data))
        self.org_problem(409, 'POST', '/issues/WEB-3/attachments/prepare', self.declaration(data))
        complete = dict(uploadId=upload['uploadId'])
        self.org_problem(409, 'POST', '/issues/OPS-1/attachments/from-upload', complete)
        for headers in ({'Cookie': 'session=1'}, {'x-org-id': WORKS}):
            self.assertEqual(self.put(upload, data, headers=headers)[0], 400, headers)
        self.assertEqual(self.put(upload, data, token=TOKEN)[0], 400, 'no bearer on the transfer URL')
        self.assertEqual(self.put(upload, data, content_type='text/plain')[0], 400)
        self.assertEqual(self.put(upload, data + b'!')[0], 400)
        self.assertEqual(self.put(upload, data)[:2], (204, b''))
        self.assertEqual(self.put(upload, data)[0], 409, 'the bytes go once')
        self.configure(checkingPolls=2, retryAfterSeconds=3)
        for _ in range(2):
            code, value, headers = self.org('POST', '/issues/OPS-1/attachments/from-upload', complete)
            self.assertEqual((code, value, headers['Retry-After']),
                             (202, dict(uploadId=upload['uploadId'], status='checking'), '3'))
        self.org_problem(404, 'POST', '/issues/OPS-2/attachments/from-upload', complete)
        created = self.ok('POST', '/issues/OPS-1/attachments/from-upload', complete, status=201)
        self.assertEqual((created['fileName'], created['kind'], created['sizeBytes'], created['version']),
                         ('notes.md', 'file', len(data), 1))
        self.assertEqual(created['sha256'], body['sha256'])
        self.assertEqual(created['uploadId'], upload['uploadId'], 'the record names its transfer')
        self.assertEqual(self.ok('POST', '/issues/OPS-1/attachments/from-upload', complete), created, 'a replay')
        self.assertEqual(self.get('/issues/OPS-1/attachments')['items'][-1], created)
        self.org_problem(409, 'POST', '/issues/OPS-1/attachments/prepare', body)  # A spent key.
        self.org_problem(409, 'DELETE', f"/issues/OPS-1/attachments/pending/{upload['uploadId']}")
        self.assert_download(self.grants(created['publicFileId'])[0], data, 'text/markdown')
        puts = [r for r in self.call('GET', '/__fixture/requests', token=None)[1]
                if r['method'] == 'PUT' and r['path'] == f"/u/{upload['uploadId']}"]
        accepted = [r for r in puts if r['contentType'] == 'text/markdown' and r['contentLength'] == len(data)
                    and not r['authorization'] and not r['cookie'] and r['orgHeader'] is None]
        self.assertEqual(len(accepted), 2, 'the successful PUT and the refused repeat')
        self.assertTrue(all(r['query'] == {'grant': ['REDACTED']} for r in puts))

    def test_a_one_byte_file_and_versioned_deletion(self):
        data = b'x'
        upload = self.ok('POST', '/issues/OPS-2/attachments/prepare', self.declaration(data, 'x.txt', 'text/plain'))
        self.assertEqual(self.put(upload, data)[0], 204)
        created = self.ok('POST', '/issues/OPS-2/attachments/from-upload', dict(uploadId=upload['uploadId']),
                          status=201)
        self.assertEqual(created['sizeBytes'], 1)
        self.assert_download(self.grants(created['publicFileId'])[0], data, 'text/plain')
        path = f"/issues/OPS-2/attachments/{created['mediaId']}"
        self.org_problem(400, 'DELETE', path)
        self.org_problem(400, 'DELETE', path + '?version=0')
        stale = self.org_problem(409, 'DELETE', path + '?version=2')
        self.assertEqual(stale['currentVersion'], 1)
        self.org_problem(404, 'DELETE', f"/issues/OPS-1/attachments/{created['mediaId']}?version=1")
        status, body, _ = self.org('DELETE', path + '?version=1')
        self.assertEqual((status, body), (204, b''))
        self.assertEqual(self.get('/issues/OPS-2/attachments')['items'], [])
        self.org_problem(404, 'DELETE', path + '?version=1')
        self.org_problem(404, 'POST', '/files/grants', dict(publicFileIds=[created['publicFileId']]),
                         headers={'x-org-id': WORKS})
        status, value, _ = self.org('POST', '/issues/OPS-2/attachments/from-upload', dict(uploadId=upload['uploadId']))
        self.assertEqual(status, 404, 'a completion replay after deletion is no longer readable')

    def test_a_pending_upload_can_be_cancelled(self):
        data = b'draft'
        body = self.declaration(data, 'draft.txt', 'text/plain')
        upload = self.ok('POST', '/issues/OPS-1/attachments/prepare', body)
        pending = f"/issues/OPS-1/attachments/pending/{upload['uploadId']}"
        self.org_problem(404, 'DELETE', f"/issues/OPS-2/attachments/pending/{upload['uploadId']}")
        for _ in range(2):
            status, value, _ = self.org('DELETE', pending)
            self.assertEqual((status, value), (204, b''))
        self.assertEqual(self.put(upload, data)[0], 409)
        self.org_problem(409, 'POST', '/issues/OPS-1/attachments/from-upload', dict(uploadId=upload['uploadId']))
        self.org_problem(409, 'POST', '/issues/OPS-1/attachments/prepare', body)
        self.org_problem(404, 'DELETE', f'/issues/OPS-1/attachments/pending/{uuid.uuid4()}')


class ChatUploadTests(FileCase):
    options = dict(served=('chatUploads',))

    def upload_chat(self, data=b'%PDF-1.4 fixture', name='brief.pdf', content_type='application/pdf', key=None):
        key = key or str(uuid.uuid4())
        upload = self.ok('POST', '/chat/uploads/prepare', self.declaration(data, name, content_type, key))
        self.assertEqual(self.put(upload, data)[0], 204)
        return key, upload, self.ok('POST', '/chat/uploads/from-upload', dict(uploadId=upload['uploadId']),
                                    status=201)

    def test_a_chat_upload_completes_to_a_stable_reference_with_a_grant(self):
        data = b'%PDF-1.4 fixture brief'
        self.configure(checkingPolls=1, retryAfterSeconds=0)
        key = str(uuid.uuid4())
        upload = self.ok('POST', '/chat/uploads/prepare', self.declaration(data, 'brief.pdf', 'application/pdf', key))
        self.assertEqual(self.put(upload, data)[0], 204)
        code, value, headers = self.org('POST', '/chat/uploads/from-upload', dict(uploadId=upload['uploadId']))
        self.assertEqual((code, value['status'], headers['Retry-After']), (202, 'checking', '0'))
        chat = self.ok('POST', '/chat/uploads/from-upload', dict(uploadId=upload['uploadId']), status=201)
        self.assertEqual(set(chat), {'id', 'reference', 'fileName', 'contentType', 'sizeBytes', 'sha256',
                                     'uploadedByUserId', 'createdAt', 'expiresAt', 'version'})
        expected = 'UPL-' + base64.urlsafe_b64encode(uuid.UUID(key).bytes).decode().rstrip('=')
        self.assertEqual((chat['id'], chat['reference'], len(chat['reference'])), (key, expected, 26))
        self.assertEqual((chat['uploadedByUserId'], chat['version'], chat['sizeBytes']), (ADA, 1, len(data)))
        lifetime = NETT.parse_stamp(chat['expiresAt']) - NETT.parse_stamp(chat['createdAt'])
        self.assertAlmostEqual(lifetime.total_seconds(), 30 * 86400, delta=5)
        self.assertEqual(self.ok('POST', '/chat/uploads/from-upload', dict(uploadId=upload['uploadId'])), chat)
        self.assertIsNone(self.server.fixture.transfers[upload['uploadId']]['data'])
        self.assertEqual(self.get(f"/chat/uploads/{chat['reference']}"), chat)
        grant = self.ok('POST', f"/chat/uploads/{chat['reference']}/grants")
        self.assertLessEqual(NETT.parse_stamp(grant['expiresAtUtc']), NETT.parse_stamp(chat['expiresAt']))
        self.assert_download(grant, data, 'application/pdf')
        self.org_problem(409, 'POST', '/chat/uploads/prepare', self.declaration(data, 'brief.pdf', 'application/pdf',
                                                                                key))
        for reference in ('UPL-AAAAAAAAAAAAAAAAAAAAAA', 'nope'):
            self.org_problem(404, 'GET', f'/chat/uploads/{reference}')
            self.org_problem(404, 'POST', f'/chat/uploads/{reference}/grants')
        self.org_problem(404, 'GET', f"/chat/uploads/{chat['reference']}", org=SECOND)

    def test_the_same_draft_in_two_organisations_is_two_uploads(self):
        key = str(uuid.uuid4())
        mine, theirs = b'%PDF-1.4 works', b'%PDF-1.4 second org'
        uploads = {}
        for org, data in ((WORKS, mine), (SECOND, theirs)):
            upload = self.ok('POST', '/chat/uploads/prepare', self.declaration(data, 'brief.pdf', 'application/pdf',
                                                                               key), org=org)
            self.assertEqual(self.put(upload, data)[0], 204)
            uploads[org] = upload
        works = self.ok('POST', '/chat/uploads/from-upload', dict(uploadId=uploads[WORKS]['uploadId']), status=201)
        grant = self.ok('POST', f"/chat/uploads/{works['reference']}/grants")
        self.ok('POST', '/chat/uploads/from-upload', dict(uploadId=uploads[SECOND]['uploadId']), status=201,
                org=SECOND)
        # The second organisation's completion leaves the first's file alone.
        self.assert_download(grant, mine, 'application/pdf')
        self.assertEqual(self.ok('POST', '/chat/uploads/from-upload', dict(uploadId=uploads[WORKS]['uploadId'])),
                         works, 'a replay is still its own')
        second = self.ok('POST', f"/chat/uploads/{works['reference']}/grants", org=SECOND)
        self.assert_download(second, theirs, 'application/pdf')

    def test_accepted_types_cancellation_deletion_and_expiry(self):
        for content_type in ('application/zip', 'text/plain', 'image/webp',
                             'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'):
            self.ok('POST', '/chat/uploads/prepare', self.declaration(b'data', 'file', content_type))
        self.org_problem(400, 'POST', '/chat/uploads/prepare', self.declaration(b'<svg/>', 'a.svg', 'image/svg+xml'))
        upload = self.ok('POST', '/chat/uploads/prepare', self.declaration(b'later'))
        status, _, _ = self.org('DELETE', f"/chat/uploads/pending/{upload['uploadId']}")
        self.assertEqual(status, 204)
        self.org_problem(409, 'POST', '/chat/uploads/from-upload', dict(uploadId=upload['uploadId']))
        key, done, chat = self.upload_chat()
        self.org_problem(409, 'DELETE', f"/chat/uploads/pending/{done['uploadId']}")
        reference = chat['reference']
        self.org_problem(400, 'DELETE', f'/chat/uploads/{reference}')
        self.assertEqual(self.org_problem(409, 'DELETE', f'/chat/uploads/{reference}?version=2')['currentVersion'], 1)
        status, body, _ = self.org('DELETE', f'/chat/uploads/{reference}?version=1')
        self.assertEqual((status, body), (204, b''))
        self.org_problem(404, 'GET', f'/chat/uploads/{reference}')
        self.org_problem(404, 'POST', f'/chat/uploads/{reference}/grants')
        self.org_problem(404, 'POST', '/chat/uploads/from-upload', dict(uploadId=done['uploadId']))
        stored = self.server.fixture.chat_uploads[(WORKS, chat['id'])]
        self.assertNotIn(stored['publicFileId'], self.server.fixture.files)
        self.assertIsNone(self.server.fixture.transfers[done['uploadId']]['data'])
        self.org_problem(409, 'POST', '/chat/uploads/prepare', self.declaration(b'%PDF-1.4 fixture', 'brief.pdf',
                                                                                'application/pdf', key))
        _, _, old = self.upload_chat()
        self.server.fixture.chat_uploads[(WORKS, old['id'])]['expiresAt'] = NETT.stamp(NETT.utc_now())
        self.org_problem(404, 'GET', f"/chat/uploads/{old['reference']}")

    def test_a_token_denied_grants_reads_metadata_but_gets_no_grant(self):
        _, _, chat = self.upload_chat()
        status, value, _ = self.call('POST', '/__fixture/tokens', dict(token='second-account-token', denyGrants=True),
                                     token=None)
        self.assertEqual((status, value['denyGrants']), (200, True))
        other = dict(token='second-account-token')
        self.assertEqual(self.get(f"/chat/uploads/{chat['reference']}", **other), chat)
        self.org_problem(403, 'POST', f"/chat/uploads/{chat['reference']}/grants", **other)
        self.org_problem(403, 'GET', f'/issues/media/{IMAGE}', **other)
        public = self.get(f'/issues/media/{IMAGE}')['media']['publicFileId']
        self.org_problem(403, 'POST', '/files/grants', dict(publicFileIds=[public]), headers={'x-org-id': WORKS},
                         **other)
        self.assertEqual(self.issue('OPS-1')['identifier'], 'OPS-1')
        self.assertEqual(self.org('GET', '/issues/OPS-1', **other)[0], 200)
        for body in (dict(token='short'), dict(token='has spaces in it'), dict(token='okay-token', denyGrants='yes')):
            self.problem(400, 'POST', '/__fixture/tokens', body, token=None)


class DecisionTests(FixtureCase):
    def test_decisions_list_detail_and_status_filter(self):
        titles = lambda value: [d['title'] for d in value['items']]
        current = self.get('/decisions')
        self.assertEqual(titles(current), ['Use one release branch', 'Keep nightly backups for 90 days'])
        self.assertEqual((current['counts'], current['omittedSupersededCount']),
                         (dict(current=2, superseded=1, all=3), 1))
        self.assertEqual(titles(self.get('/decisions?status=all')),
                         ['Use one release branch', 'Keep nightly backups for 90 days', 'Keep nightly backups for 30 days'])
        self.assertEqual(titles(self.get('/decisions?status=superseded')), ['Keep nightly backups for 30 days'])
        issue_id = self.issue('OPS-1')['id']
        linked = self.get(f'/decisions?issueId={issue_id}')
        self.assertEqual((titles(linked), linked['counts']['all']), (['Keep nightly backups for 90 days'], 2))
        self.assertEqual([d['status'] for d in self.get(f'/decisions?issueId={issue_id}&status=all')['items']],
                         ['accepted', 'superseded'])
        page = self.get('/decisions?status=all&limit=1')
        self.assertEqual(titles(self.get('/decisions?status=all&limit=2&cursor=' + page['nextCursor'])),
                         ['Keep nightly backups for 90 days', 'Keep nightly backups for 30 days'])
        for query in ('?issueId=OPS-1', '?status=open', '?limit=51', '?limit=0'):
            self.org_problem(400, 'GET', '/decisions' + query)
        accepted = self.get(f"/decisions/{current['items'][1]['id']}")
        self.assertEqual((accepted['supersedes']['status'], accepted['issues']['items'][0]['identifier']),
                         ('superseded', 'OPS-1'))
        self.assertEqual((accepted['project']['name'], accepted['effectiveInitiative']['name']), ('Launch', 'Platform'))
        self.assertIsNotNone(accepted['acceptedAt'])
        self.assertTrue(accepted['decision'] and accepted['why'])
        self.org_problem(404, 'GET', f'/decisions/{NETT.uid("nope")}')
        self.assertEqual(self.get('/decisions', org=SECOND)['items'], [])


class IssueListItemTests(FixtureCase):
    def test_named_lists_items_create_update_and_conflict(self):
        issue_id = self.issue('OPS-1')['id']
        self.org_problem(400, 'GET', '/issues/OPS-1/lists')
        lists = self.get(f'/issues/{issue_id}/lists')['items']
        self.assertEqual([(l['id'], l['name'], l['itemCount'], l['completedItemCount']) for l in lists],
                         [(CHECKLIST, 'Checklist', 3, 1)])
        base = f'/issues/{issue_id}/lists/{CHECKLIST}/items'
        self.assertEqual(len(self.get(base)['items']), 3)
        done = self.get(base + '?completed=true')['items']
        self.assertEqual([(i['completion']['kind'], i['completion']['actor']['userId']) for i in done], [('nett', GRACE)])
        before = self.activity_item('OPS-1')['lastActivityAt']
        for body in (dict(text='Done already', completed=True), dict(text='x', colour='red'), dict(text='  '),
                     dict(text='x' * 2001), dict(text='Imported', provenance=dict(sourceSystem='s', sourceId='1'))):
            self.org_problem(400, 'POST', base, body)
        created = self.ok('POST', base, dict(text='Tell the team'), status=201)
        item = created['item']
        self.assertEqual((created['outcome'], item['version'], item['completed'], item['completion']),
                         ('created', 1, False, None))
        self.assertGreater(self.activity_item('OPS-1')['lastActivityAt'], before)
        saved = self.ok('PUT', f"{base}/{item['id']}", dict(version=1, completed=True))
        self.assertEqual((saved['outcome'], saved['item']['version'], saved['item']['completion']['actor']['userId']),
                         ('updated', 2, ADA))
        conflict = self.org_problem(409, 'PUT', f"{base}/{item['id']}", dict(version=1, text='Stale'))
        self.assertEqual(conflict['currentVersion'], 2)
        for body in (dict(version=2), dict(text='No version'), dict(version=2, completed='yes'), dict(version=2, pinned=True)):
            self.org_problem(400, 'PUT', f"{base}/{item['id']}", body)
        removed = self.ok('PUT', f"{base}/{item['id']}", dict(version=2, removed=True, text='Told the team'))
        self.assertEqual((removed['item']['removed'], removed['item']['text']), (True, 'Told the team'))
        self.assertEqual(len(self.get(base)['items']), 3)
        self.assertEqual(len(self.get(base + '?includeRemoved=true')['items']), 4)
        other = self.issue('OPS-2')['id']
        self.org_problem(404, 'GET', f'/issues/{other}/lists/{CHECKLIST}/items')
        self.org_problem(404, 'PUT', f'{base}/{NETT.uid("nope")}', dict(version=1, completed=True))


class PickerTests(FixtureCase):
    def test_members_labels_cycles_projects_milestones_and_mentions(self):
        members = self.get('/members')
        self.assertEqual(([m['firstName'] for m in members['items']], members['totalCount'], members['page'],
                          members['pageSize']), (['Ada', 'Grace'], 2, 1, 20))
        everyone = self.get('/members?status=all')['items']
        self.assertEqual([(m['firstName'], m['status']) for m in everyone],
                         [('Ada', 'active'), ('Grace', 'active'), ('Linus', 'removed')])
        self.assertEqual([m['firstName'] for m in self.get('/members?page=2&pageSize=1')['items']], ['Grace'])
        for query in ('?pageSize=101', '?pageSize=0', '?page=0'):
            self.org_problem(400, 'GET', '/members' + query)
        names = lambda value: [item['name'] for item in value['items']]
        self.assertEqual(names(self.get('/labels')), ['bug', 'docs', 'key client'])
        self.assertEqual(names(self.get('/labels?usedFor=issues')), ['bug', 'docs'])
        self.assertEqual(names(self.get('/labels?usedFor=issues&includeInactive=true')), ['bug', 'docs', 'legacy'])
        self.org_problem(400, 'GET', '/labels?usedFor=everything')
        cycles = self.get('/issue-cycles?team=OPS')
        self.assertEqual([(c['number'], c['state'], c['teamKey']) for c in cycles['items']],
                         [(3, 'current', 'OPS'), (4, 'upcoming', 'OPS')])
        self.assertEqual(cycles['items'][0]['progress']['counted'], 2)
        self.assertEqual(self.get('/issue-cycles?team=WEB')['items'], [])
        self.assertEqual([c['number'] for c in self.get('/issue-cycles?state=current')['items']], [3])
        self.org_problem(404, 'GET', '/issue-cycles?team=NOPE')
        self.assertEqual(names(self.get('/projects')), ['Hardening', 'Launch'])
        milestones = self.get(f'/projects/{LAUNCH}/milestones')
        self.assertEqual((names(milestones), milestones['doneCount'], milestones['totalCount']), (['Alpha', 'Beta'], 1, 2))
        self.assertTrue(milestones['items'][0]['description'])
        brief = self.get(f'/projects/{LAUNCH}/milestones?includeContent=false')['items']
        self.assertEqual([(m['description'], m['contentIncluded']) for m in brief], [(None, False)] * 2)
        beta = milestones['items'][1]['id']
        self.assertEqual(names(self.get(f'/projects/{LAUNCH}/milestones?milestoneId={beta}')), ['Beta'])
        self.org_problem(404, 'GET', f'/projects/{NETT.uid("nope")}/milestones')
        mentions = self.get('/mention-candidates?kind=issue')['items']
        self.assertEqual([m['displayName'] for m in mentions], ['Ada Fixture', 'Grace Fixture'])
        self.assertEqual([m['displayName'] for m in self.get('/mention-candidates?kind=issue&q=grace')['items']],
                         ['Grace Fixture'])
        suggested = self.get(f"/mention-candidates?kind=issue&contentId={self.issue('OPS-2')['id']}")['items']
        self.assertEqual([(m['displayName'], m['isSuggested']) for m in suggested],
                         [('Ada Fixture', True), ('Grace Fixture', True)])
        self.org_problem(400, 'GET', '/mention-candidates')
        self.org_problem(400, 'GET', '/mention-candidates?kind=document')

    def test_initiatives_and_their_projects(self):
        initiatives = self.get('/initiatives')
        names = ['Desktop', 'Empty', 'Platform', 'Research']
        self.assertEqual(([i['name'] for i in initiatives['items']], initiatives['totalCount']), (names, 4))
        self.assertEqual(initiatives['maxLevel'], 1)
        by_name = {i['name']: i for i in initiatives['items']}
        platform = by_name['Platform']
        self.assertEqual((platform['rollUp']['projectCount'], platform['childCount'], platform['level']), (1, 1, 0))
        desktop = by_name['Desktop']
        self.assertEqual((desktop['parentId'], desktop['level'], [n['name'] for n in desktop['path']]),
                         (platform['id'], 1, ['Platform', 'Desktop']))
        everything = self.get('/initiatives?includeArchived=true')['items']
        archived = {i['name']: (i['archived'], i['ancestorArchived']) for i in everything if i['name'] not in names}
        self.assertEqual(archived, {'Retired': (True, False), 'Legacy': (False, True)})
        detail = self.get(f"/initiatives/{platform['id']}")
        self.assertEqual((detail['description'], detail['directProjectCount']), ('Keep the platform dependable.', 1))
        projects = self.get(f"/initiatives/{platform['id']}/projects")
        self.assertEqual(([p['name'] for p in projects['items']], projects['totalCount'], projects['groups'][0]['name']),
                         (['Launch'], 1, 'Platform'))
        empty = self.get(f"/initiatives/{by_name['Empty']['id']}/projects")
        self.assertEqual((empty['items'], empty['totalCount'], empty['groups']), ([], 0, []))
        self.org_problem(404, 'GET', f'/initiatives/{NETT.uid("nope")}')


class HookTests(FixtureCase):
    def test_request_log_records_shape_and_never_secrets(self):
        self.call('GET', '/api/v1/auth/orgs')
        status, value, _ = self.call('POST', '/__fixture/requests/clear', token=None)
        self.assertEqual((status, value['cleared']), (200, 1))
        self.call('GET', '/api/v1/auth/context', headers={'x-org-id': WORKS})
        grant = self.get(f'/issues/media/{IMAGE}')['grant']['url']
        parts = urlsplit(grant)
        self.call('GET', f'{parts.path}?{parts.query}', token=None)
        self.org('GET', '/issues?team=OPS&stateType=started&stateType=triage', headers={'x-org-id': WORKS})
        log = self.call('GET', '/__fixture/requests', token=None)[1]
        self.assertEqual([(r['method'], r['path'], r['authorization'], r['orgHeader']) for r in log],
                         [('GET', '/api/v1/auth/context', True, WORKS),
                          ('GET', f'/api/v1/orgs/{WORKS}/issues/media/{IMAGE}', True, WORKS),
                          ('GET', parts.path, False, None),
                          ('GET', f'/api/v1/orgs/{WORKS}/issues', True, WORKS)])
        self.assertEqual(log[2]['query'], {'grant': ['REDACTED']})
        self.assertEqual({(r['cookie'], r['contentType'], r['contentLength']) for r in log}, {(False, None, None)})
        self.call('POST', '/api/v1/orgs/' + WORKS + '/issues/OPS-1/comments', dict(body='Logged'),
                  headers={'Cookie': 'session=secret-cookie'})
        posted = self.call('GET', '/__fixture/requests', token=None)[1][-1]
        self.assertEqual((posted['cookie'], posted['contentType'], posted['contentLength']),
                         (True, 'application/json', len(json.dumps(dict(body='Logged')))))
        self.assertNotIn('secret-cookie', json.dumps(posted))
        self.assertEqual(log[3]['query'], {'team': ['OPS'], 'stateType': ['started', 'triage']})
        text = json.dumps(log)
        self.assertNotIn(TOKEN, text)
        self.assertNotIn(parse_qs(parts.query)['grant'][0], text)

    def test_refused_requests_only_log_the_presence_of_unrecognised_query_parameters(self):
        query = urlencode([('access_token', TOKEN), ('access_token', 'second-value'),
                           ('team', TOKEN), ('q', 'private search')])
        self.problem(401, 'GET', f'/api/v1/auth/context?{query}', token=None)
        log = self.call('GET', '/__fixture/requests', token=None)[1]
        self.assertEqual(log[0]['query'], dict(access_token=['REDACTED'], team=['REDACTED'], q=['REDACTED']))


def flatten(schema, schemas, depth=0):
    """The concrete schemas a value may match: $ref resolved, allOf, oneOf and anyOf unioned."""
    if depth > 64 or not isinstance(schema, dict):
        return []
    if '$ref' in schema:
        target = schemas[schema['$ref'].rsplit('/', 1)[-1]]
        rest = {key: value for key, value in schema.items() if key != '$ref'}
        return flatten(target, schemas, depth + 1) + flatten(rest, schemas, depth + 1)
    found = [schema]
    for key in ('allOf', 'oneOf', 'anyOf'):
        for part in schema.get(key, []):
            found += flatten(part, schemas, depth + 1)
    return found


def undeclared(value, schema, schemas, path='$'):
    """Yields the JSON path of every property `value` has that `schema` does not declare."""
    parts = flatten(schema, schemas)
    if isinstance(value, list):
        items = [part['items'] for part in parts if isinstance(part.get('items'), dict)]
        for index, item in enumerate(value):
            yield from undeclared(item, {'anyOf': items}, schemas, f'{path}[{index}]')
    elif isinstance(value, dict):
        properties, extra = {}, []
        for part in parts:
            for name, sub in part.get('properties', {}).items():
                properties.setdefault(name, []).append(sub)
            if isinstance(part.get('additionalProperties'), dict) and part['additionalProperties']:
                extra.append(part['additionalProperties'])
        if not properties and not extra:
            return  # A free-form object, such as activity data.
        for name, item in value.items():
            if name in properties:
                yield from undeclared(item, {'anyOf': properties[name]}, schemas, f'{path}.{name}')
            elif extra:
                yield from undeclared(item, {'anyOf': extra}, schemas, f'{path}.{name}')
            else:
                yield f'{path}.{name}'


class WalkerTests(unittest.TestCase):
    def test_the_walker_reports_undeclared_properties_through_refs_and_compositions(self):
        schemas = dict(Leaf=dict(type='object', properties=dict(id={})),
                       Wrapper=dict(allOf=[{'$ref': '#/components/schemas/Leaf'},
                                           dict(properties=dict(items=dict(type='array', items={'nullable': True, 'oneOf': [
                                               {'$ref': '#/components/schemas/Leaf'}]})))]))
        value = dict(id=1, items=[dict(id=2), dict(id=3, extra=4)], other=5)
        self.assertEqual(list(undeclared(value, {'$ref': '#/components/schemas/Wrapper'}, schemas)),
                         ['$.items[1].extra', '$.other'])
        self.assertEqual(list(undeclared(dict(anything=1), dict(type='object'), schemas)), [])


class StreamCase(FixtureCase):
    def open_stream(self, keys=('OPS',), org=WORKS, token=TOKEN, orgs=None):
        """An update stream request; `orgs` sends that list of x-org-id headers instead of one."""
        connection = http.client.HTTPConnection(urlsplit(self.server.origin).netloc, timeout=5)
        query = '&'.join(f'project={key}' for key in keys)
        connection.putrequest('GET', f'/api/v1/updates?{query}')
        if token is not None:
            connection.putheader('Authorization', f'Bearer {token}')
        connection.putheader('Accept', 'text/event-stream')
        for value in ([org] if orgs is None else orgs):
            connection.putheader('x-org-id', value)
        connection.endheaders()
        response = connection.getresponse()
        self.addCleanup(connection.close)
        return response

    def refused(self, status, **options):
        response = self.open_stream(**options)
        body = json.loads(response.read())
        self.assertEqual((response.status, body['status']), (status, status), body)
        return response

    def records(self, response, until):
        """Server-sent records as (event, data) pairs, comments as (':', text)."""
        found, event, data = [], None, []
        for raw in response:
            line = raw.decode().rstrip('\n')
            if line.startswith(':'):
                found.append((':', line[1:].strip()))
            elif line.startswith('event:'):
                event = line[6:].strip()
            elif line.startswith('data:'):
                data.append(line[5:].strip())
            elif line == '' and event:
                found.append((event, json.loads('\n'.join(data))))
                event, data = None, []
            if until(found):
                break
        return found

    def ready(self, response):
        self.assertEqual(response.status, 200)
        self.assertEqual(self.records(response, lambda found: found), [('ready', None)])

    def ends_quickly(self, response, within=2.5):
        """Reads to the end of a stream the server closes, and returns the rest."""
        started = time.monotonic()
        rest = response.read()
        self.assertLess(time.monotonic() - started, within)
        return rest


class UpdateStreamTests(StreamCase):
    options = dict(stream_seconds=1.5, keepalive_seconds=0.3, max_streams_per_user=2, served=('updates',))

    def test_ready_then_issue_changes_for_followed_teams_with_keepalives(self):
        response = self.open_stream()
        self.assertEqual(response.status, 200)
        self.assertTrue(response.headers['Content-Type'].startswith('text/event-stream'))
        self.assertEqual(response.headers['Cache-Control'], 'private, no-store, no-transform')
        self.assertEqual(self.records(response, lambda found: found)[0], ('ready', None))
        self.ok('PATCH', '/issues/WEB-1', dict(version=1, priority=1))
        self.ok('PATCH', '/issues/OPS-4', dict(version=1, priority=3))
        found = self.records(response, lambda found: any(name == 'issueChanged' for name, _ in found))
        changed = [data for name, data in found if name == 'issueChanged']
        self.assertEqual([data['identifier'] for data in changed], ['OPS-4'], 'WEB is not followed')
        self.assertEqual(changed[0]['projectKey'], 'OPS')
        self.assertEqual(changed[0]['parentIdentifier'], 'OPS-2')
        self.assertEqual(changed[0]['version'], 2)
        self.assertEqual((changed[0]['action'], changed[0]['actorType']), ('issue.updated', 'human'))
        self.assertEqual(set(changed[0]), {'projectKey', 'issueId', 'identifier', 'parentIdentifier', 'version',
                                           'action', 'auditId', 'actorType', 'occurredAt'})
        self.assertEqual(changed[0]['auditId'], self.get('/issues/OPS-4/activity')['items'][0]['id'])
        self.call('POST', '/__fixture/touch', dict(org=WORKS, issue='OPS-1', actorType='integration', audit=False),
                  token=None)
        self.ok('POST', '/issues/OPS-5/comments', dict(body='Seen it.'), status=201)
        found = self.records(response, lambda found: sum(name == 'issueChanged' for name, _ in found) >= 2)
        touched, commented = [data for name, data in found if name == 'issueChanged']
        self.assertNotIn('auditId', touched, 'auditId is left out when no entry raised the change')
        self.assertEqual((touched['identifier'], touched['actorType'], touched['action']),
                         ('OPS-1', 'integration', 'issue.updated'))
        self.assertEqual((commented['identifier'], commented['action']), ('OPS-5', 'comment.created'))
        found = self.records(response, lambda found: (':', 'keepalive') in found)
        self.assertIn((':', 'keepalive'), found)
        # The stream closes by itself after its lifetime: reading to the end returns.
        self.assertNotIn(b'issueChanged', self.ends_quickly(response, 3))

    def test_streams_need_one_valid_organisation_header_and_known_teams(self):
        self.refused(400, orgs=[])
        self.refused(400, orgs=['nope'])
        self.refused(400, orgs=[WORKS, WORKS])
        self.refused(400, orgs=[WORKS, SECOND])
        self.refused(400, keys=())
        self.refused(400, keys=('O-PS',))
        self.refused(400, keys=[f'K{n}' for n in range(65)])
        self.refused(401, token=None)
        self.refused(401, token='wrong-token')
        self.refused(403, org=OUTSIDE)
        self.refused(403, org=NETT.uid('org/nowhere'))
        self.refused(404, org=PERSONAL)
        self.refused(404, keys=('NOPE',))
        self.refused(404, keys=('OPS', 'NOPE'))
        self.call('POST', '/__fixture/capabilities', dict(org=SECOND, features=[]), token=None)
        self.refused(404, org=SECOND)
        self.ready(self.open_stream(keys=('ops', 'WEB')))

    def test_streams_are_limited_and_refusals_carry_whole_second_waits(self):
        first, second = self.open_stream(), self.open_stream()
        self.assertEqual((first.status, second.status), (200, 200))
        self.assertEqual(self.refused(429).headers['Retry-After'], '1')
        self.call('POST', '/__fixture/updates', dict(unavailable=True), token=None)
        self.assertEqual(self.refused(503).headers['Retry-After'], '1')

    def test_the_refusal_hook_answers_the_next_requests(self):
        status, value, _ = self.call('POST', '/__fixture/config',
                                     dict(streamRefusal=dict(status=429, retryAfter=7, times=2)), token=None)
        self.assertEqual((status, value['streamRefusal']['times']), (200, 2))
        for _ in range(2):
            self.assertEqual(self.refused(429).headers['Retry-After'], '7')
        self.ready(self.open_stream())
        self.call('POST', '/__fixture/config', dict(streamRefusal=dict(status=503, retryAfter=0, times=1)),
                  token=None)
        self.assertEqual(self.refused(503).headers['Retry-After'], '0')
        # A wait as long as a client can be told is sent as it is.
        self.call('POST', '/__fixture/config', dict(streamRefusal=dict(status=429, retryAfter=2**64 - 1, times=1)),
                  token=None)
        self.assertEqual(self.refused(429).headers['Retry-After'], str(2**64 - 1))
        for body in (dict(streamRefusal=dict(status=500, retryAfter=1, times=1)),
                     dict(streamRefusal=dict(status=429, retryAfter=2**64, times=1)),
                     dict(streamRefusal=dict(status=429, retryAfter=1)), dict(other=1), dict(checkingPolls=-1)):
            self.problem(400, 'POST', '/__fixture/config', body, token=None)


class StreamCloseTests(StreamCase):
    options = dict(stream_seconds=6, keepalive_seconds=0.3, served=('updates',), auto_approve=True,
                   access_token_seconds=1.2)

    def test_a_closed_stream_ends_without_sending_what_is_queued(self):
        response = self.open_stream()
        self.ready(response)
        subscriber = self.server.subscribers[0]
        with subscriber.ready:  # Queue a hint, then lose authority before the stream can send it.
            subscriber.push(dict(projectKey='OPS', issueId=NETT.uid('x'), identifier='OPS-1', version=9))
            subscriber.close('lost authority')
        self.assertNotIn(b'issueChanged', self.ends_quickly(response))
        deadline = time.monotonic() + 2
        while self.server.subscribers and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertEqual(self.server.subscribers, [])

    def test_the_close_hook_ends_open_streams(self):
        works, second = self.open_stream(), self.open_stream(org=SECOND)
        self.ready(works)
        self.ready(second)
        status, value, _ = self.call('POST', '/__fixture/updates/close', dict(org=WORKS), token=None)
        self.assertEqual((status, value), (200, dict(closed=1)))
        self.ends_quickly(works)
        status, value, _ = self.call('POST', '/__fixture/updates/close', {}, token=None)
        self.assertEqual((status, value), (200, dict(closed=1)))
        self.ends_quickly(second)

    def test_a_queue_overflow_closes_the_stream(self):
        response = self.open_stream()
        self.ready(response)
        subscriber = self.server.subscribers[0]
        with subscriber.ready:
            for number in range(NETT.STREAM_QUEUE + 1):
                subscriber.push(dict(projectKey='OPS', issueId=str(number), identifier=f'OPS-{number + 1}', version=1))
            self.assertEqual(subscriber.closed, 'overflow')
        self.assertNotIn(b'issueChanged', self.ends_quickly(response))

    def test_an_archived_team_closes_its_streams_and_later_requests_get_404(self):
        response = self.open_stream(keys=('OPS', 'WEB'))
        self.ready(response)
        status, value, _ = self.call('POST', '/__fixture/teams/archive', dict(org=WORKS, key='OPS'), token=None)
        self.assertEqual((status, value['key'], value['closed']), (200, 'OPS', 1))
        self.ends_quickly(response)
        self.refused(404, keys=('OPS',))
        self.ready(self.open_stream(keys=('WEB',)))
        self.assertIsNotNone(self.get('/issue-teams/OPS')['archivedAt'])
        self.assertEqual([t['key'] for t in self.get('/issue-teams')['items']], ['WEB'])

    def test_a_stream_closes_when_its_token_expires_and_a_refreshed_token_reopens_it(self):
        device = self.start_device()
        self.poll(device['device_code'])
        status, tokens = self.poll(device['device_code'])
        self.assertEqual(status, 200, tokens)
        response = self.open_stream(token=tokens['access_token'])
        self.ready(response)
        self.ends_quickly(response, within=2.5)  # Long before the six-second lifetime.
        self.assertEqual(self.call('GET', '/api/v1/me', token=tokens['access_token'])[0], 401)
        self.refused(401, token=tokens['access_token'])
        status, rotated, _ = self.call('POST', '/user_management/authenticate', token=None,
                                       form=dict(grant_type='refresh_token', refresh_token=tokens['refresh_token'],
                                                 client_id=CLIENT_ID))
        self.assertEqual(status, 200, rotated)
        self.ready(self.open_stream(token=rotated['access_token']))


class MoveTests(StreamCase):
    options = dict(served=('issueMoves', 'updates'), stream_seconds=5, keepalive_seconds=0.3)

    def preview(self, ref='OPS-2', team=WEB, status=200, **body):
        issue = self.issue(ref)
        return self.ok('POST', f'/issues/{ref}/move-preview',
                       dict(destinationTeamId=team, version=issue['version']) | body, status=status)

    def test_a_preview_lists_the_subtree_with_null_new_identifiers(self):
        issue = self.issue('OPS-2')
        preview = self.preview()
        self.assertRegex(preview['token'], r'^[0-9a-f]{64}$')
        self.assertEqual(preview['source'], dict(id=OPS, key='OPS', name='Operations', archived=False))
        self.assertEqual(preview['destination'], dict(id=WEB, key='WEB', name='Web', archived=False))
        children = sorted(child['id'] for child in issue['children']['items'])
        self.assertEqual([row['id'] for row in preview['issues']], [issue['id']] + children)
        for row in preview['issues']:
            self.assertEqual(set(row), {'id', 'version', 'oldIdentifier', 'newIdentifier', 'stateId', 'stateName',
                                        'cycleCleared', 'milestoneCleared', 'estimateCleared', 'estimateAfter',
                                        'parentDetached', 'archived'})
            self.assertEqual((row['newIdentifier'], row['parentDetached'], row['archived']), (None, False, False))
        rows = {row['oldIdentifier']: row for row in preview['issues']}
        self.assertEqual((rows['OPS-3']['estimateCleared'], rows['OPS-3']['estimateAfter'],
                          rows['OPS-3']['cycleCleared']), (True, None, True))
        self.assertEqual((rows['OPS-3']['stateId'], rows['OPS-3']['stateName']), (state('WEB', 'completed'), 'Done'))
        self.assertEqual(preview['reviewContext'], dict(issueVersion=issue['version'], issueContextVersion=1,
                                                        effectiveProjectId=HARDENING, policyVersionId=None))
        self.org_problem(400, 'POST', '/issues/OPS-2/move-preview',
                         dict(destinationProjectId=WEB, version=issue['version']))
        self.org_problem(400, 'POST', '/issues/OPS-2/move-preview', dict(destinationTeamId=WEB))
        self.org_problem(400, 'POST', '/issues/OPS-2/move-preview', dict(destinationTeamId='web', version=1))
        self.org_problem(400, 'POST', '/issues/OPS-2/move-preview', dict(destinationTeamId=OPS, version=1))
        self.org_problem(404, 'POST', '/issues/OPS-2/move-preview',
                         dict(destinationTeamId=NETT.uid('second/team/OPS'), version=1))
        stale = self.org_problem(409, 'POST', '/issues/OPS-2/move-preview', dict(destinationTeamId=WEB, version=9))
        self.assertEqual(stale['currentVersion'], issue['version'])

    def test_a_confirmed_move_takes_the_whole_subtree_once(self):
        stream = self.open_stream(keys=('OPS',))
        self.ready(stream)
        preview = self.preview()
        confirm = dict(teamId=WEB, version=preview['reviewContext']['issueVersion'],
                       reviewContext=preview['reviewContext'], movePreviewToken=preview['token'])
        self.org_problem(400, 'PATCH', '/issues/OPS-2', dict(confirm, title='Renamed'))
        self.org_problem(400, 'PATCH', '/issues/OPS-2', dict(confirm, movePreviewToken='ABC'))
        self.org_problem(409, 'PATCH', '/issues/OPS-2', dict(confirm, teamId=OPS))
        self.org_problem(409, 'PATCH', '/issues/OPS-2', dict(confirm, reviewContext=dict(confirm['reviewContext'],
                                                                                         issueContextVersion=2)))
        self.org_problem(409, 'PATCH', '/issues/OPS-2', dict(confirm, movePreviewToken='0' * 64))
        moved = self.ok('PATCH', '/issues/OPS-2', confirm)
        self.assertEqual((moved['identifier'], moved['teamId'], moved['teamKey']), ('WEB-4', WEB, 'WEB'))
        self.assertEqual(moved['version'], confirm['version'] + 1)
        children = {child['id']: child for child in moved['children']['items']}
        self.assertEqual(sorted(child['identifier'] for child in children.values()), ['WEB-5', 'WEB-6'])
        self.assertTrue(all(child['parentId'] == moved['id'] and child['teamKey'] == 'WEB'
                            for child in children.values()))
        rotated = next(c for c in children.values() if c['title'] == 'Rotate the staging certificates')
        self.assertEqual((rotated['estimate'], rotated['cycle'], rotated['state']['type']), (None, None, 'completed'))
        self.org_problem(409, 'PATCH', '/issues/WEB-4', dict(confirm, version=moved['version']))  # A spent token.
        self.org_problem(404, 'GET', '/issues/OPS-2')
        found = self.records(stream, lambda found: sum(name == 'issueChanged' for name, _ in found) >= 3)
        hinted = sorted(data['identifier'] for name, data in found if name == 'issueChanged')
        self.assertEqual(hinted, ['WEB-4', 'WEB-5', 'WEB-6'], 'the source team hears of the move')

    def test_create_and_move_never_reuse_an_identifier_vacated_by_a_move(self):
        def move(ref, destination):
            preview = self.preview(ref, destination)
            context = preview['reviewContext']
            return self.ok('PATCH', f'/issues/{ref}', dict(teamId=destination, version=context['issueVersion'],
                                                          reviewContext=context, movePreviewToken=preview['token']))

        self.assertEqual(move('OPS-5', WEB)['identifier'], 'WEB-4')
        self.assertEqual(move('WEB-4', OPS)['identifier'], 'OPS-6')
        self.org_problem(404, 'GET', '/issues/OPS-5')
        created = self.ok('POST', '/issues', dict(teamId=WEB, title='A new browser check'), status=201)
        self.assertEqual(created['identifier'], 'WEB-5')
        self.org_problem(404, 'GET', '/issues/WEB-4')
        other = self.ok('POST', '/issues', dict(teamId=NETT.uid('second/team/OPS'), title='Independent numbering'),
                        status=201, org=SECOND)
        self.assertEqual(other['identifier'], 'OPS-2')

    def test_a_changed_plan_or_an_archived_sub_issue_refuses_the_move(self):
        preview = self.preview()
        confirm = dict(teamId=WEB, version=preview['reviewContext']['issueVersion'],
                       reviewContext=preview['reviewContext'], movePreviewToken=preview['token'])
        self.ok('PATCH', '/issues/OPS-4', dict(version=1, priority=1))
        changed = self.org_problem(409, 'PATCH', '/issues/OPS-2', confirm)
        self.assertIn('Preview it again', changed['detail'])
        self.assertEqual(changed['code'], 'IssueMove.PlanChanged')
        status, value, _ = self.call('POST', '/__fixture/issues/archive', dict(org=WORKS, issue='OPS-4'), token=None)
        self.assertEqual((status, value['identifier']), (200, 'OPS-4'))
        preview = self.preview()
        archived = {row['oldIdentifier']: row['archived'] for row in preview['issues']}
        self.assertEqual(archived, {'OPS-2': False, 'OPS-3': False, 'OPS-4': True})
        confirm = dict(teamId=WEB, version=preview['reviewContext']['issueVersion'],
                       reviewContext=preview['reviewContext'], movePreviewToken=preview['token'])
        refused = self.org_problem(409, 'PATCH', '/issues/OPS-2', confirm)
        self.assertIn('archived sub-issue', refused['detail'])
        self.assertEqual(self.issue('OPS-2')['teamKey'], 'OPS', 'nothing moved')
        self.org_problem(409, 'POST', '/issues/WEB-3/move-preview', dict(destinationTeamId=OPS, version=2))

    def test_a_patch_without_a_token_is_an_ordinary_update(self):
        updated = self.ok('PATCH', '/issues/OPS-1', dict(version=1, priority=1))
        self.assertEqual((updated['priority'], updated['teamKey']), (1, 'OPS'))
        self.org_problem(400, 'PATCH', '/issues/OPS-1', dict(version=2, teamId=WEB))


class MoveReplayTests(StreamCase):
    options = dict(served=('issueMoves', 'issueMoveReplay', 'updates'), stream_seconds=5, keepalive_seconds=0.3)

    def confirmation(self, ref='OPS-2', team=WEB):
        issue = self.issue(ref)
        preview = self.ok('POST', f'/issues/{ref}/move-preview', dict(destinationTeamId=team, version=issue['version']))
        return preview, dict(teamId=team, version=preview['reviewContext']['issueVersion'],
                             reviewContext=preview['reviewContext'], movePreviewToken=preview['token'])

    def configure(self, **settings):
        status, value, _ = self.call('POST', '/__fixture/config', settings, token=None)
        self.assertEqual(status, 200, value)

    def test_the_capability_is_served_only_when_enabled(self):
        self.assertIn('issueMoveReplay', self.get('/desktop/capabilities')['features'])
        self.call('POST', '/__fixture/capabilities', dict(org=WORKS, features=['issueMoves']), token=None)
        preview, confirm = self.confirmation()
        moved = self.ok('PATCH', '/issues/OPS-2', confirm)
        self.assertNotIn('move', moved, 'no move object without the capability')
        self.org_problem(409, 'PATCH', f"/issues/{moved['id']}", confirm)

    def test_a_first_commit_returns_the_move_and_an_identical_confirmation_replays_it(self):
        preview, confirm = self.confirmation()
        stream = self.open_stream(keys=('WEB',))
        self.ready(stream)
        moved = self.ok('PATCH', '/issues/OPS-2', confirm)
        move = moved['move']
        self.assertEqual(set(move), {'token', 'committedAt', 'replayed', 'actorType', 'issues'})
        self.assertEqual((move['token'], move['replayed'], move['actorType']), (preview['token'], False, 'human'))
        self.assertEqual([row['id'] for row in move['issues']], [row['id'] for row in preview['issues']])
        self.assertEqual([row['oldIdentifier'] for row in move['issues']],
                         [row['oldIdentifier'] for row in preview['issues']])
        self.assertEqual([row['newIdentifier'] for row in move['issues']], ['WEB-4', 'WEB-5', 'WEB-6'])
        self.assertEqual(move['issues'][0]['version'], moved['version'])
        self.records(stream, lambda found: sum(name == 'issueChanged' for name, _ in found) >= 3)
        activity = len(self.get(f"/issues/{moved['id']}/activity")['items'])
        # The pre-move version is stale; the replay does not look at it.
        replayed = self.ok('PATCH', f"/issues/{moved['id']}", confirm)
        self.assertEqual(replayed['move'], dict(move, replayed=True))
        self.assertEqual(replayed['identifier'], 'WEB-4')
        self.assertEqual(len(self.get(f"/issues/{moved['id']}/activity")['items']), activity, 'no new activity')
        # The stream is idle until its next keep-alive: no event for a replay.
        after = self.records(stream, lambda found: any(name == ':' for name, _ in found))
        self.assertEqual([name for name, _ in after if name != ':'], [], 'no event for a replay')

    def test_a_replay_after_a_sub_issue_moved_again_returns_the_rows_as_committed(self):
        _, confirm = self.confirmation()
        move = self.ok('PATCH', '/issues/OPS-2', confirm)['move']
        _, onward = self.confirmation('WEB-5', OPS)
        self.ok('PATCH', '/issues/WEB-5', onward)
        self.ok('PATCH', '/issues/WEB-6', dict(version=self.issue('WEB-6')['version'], priority=2))
        replayed = self.ok('PATCH', f"/issues/{move['issues'][0]['id']}", confirm)['move']
        self.assertEqual(replayed['issues'], move['issues'])

    def test_without_a_record_a_changed_plan_is_409_and_an_unchanged_one_commits(self):
        _, confirm = self.confirmation()
        self.ok('PATCH', '/issues/OPS-4', dict(version=1, priority=1))
        changed = self.org_problem(409, 'PATCH', '/issues/OPS-2', confirm)
        self.assertEqual(changed['code'], 'IssueMove.PlanChanged')
        self.assertEqual(self.issue('OPS-2')['teamKey'], 'OPS', 'nothing moved')
        _, confirm = self.confirmation()
        self.assertFalse(self.ok('PATCH', '/issues/OPS-2', confirm)['move']['replayed'])

    def test_a_lost_reply_commits_and_the_retry_replays(self):
        _, confirm = self.confirmation()
        self.configure(dropMoveReply=1)
        with self.assertRaises((http.client.RemoteDisconnected, ConnectionError, OSError)):
            self.org('PATCH', '/issues/OPS-2', confirm)
        root = self.issue('WEB-4')
        self.assertEqual(root['teamKey'], 'WEB', 'the move committed before the reply was lost')
        replayed = self.ok('PATCH', f"/issues/{root['id']}", confirm)['move']
        self.assertTrue(replayed['replayed'])
        self.assertEqual(replayed['issues'][0]['newIdentifier'], 'WEB-4')

    def test_a_replay_during_a_slow_commit_waits_for_it(self):
        _, confirm = self.confirmation()
        root = self.issue('OPS-2')['id']
        self.configure(slowMoveCommit=0.6)
        with ThreadPoolExecutor(2) as pool:
            first = pool.submit(self.org, 'PATCH', f'/issues/{root}', confirm)
            time.sleep(0.2)
            # Other requests carry on meanwhile.
            self.assertEqual(self.issue('OPS-1')['identifier'], 'OPS-1')
            replay = pool.submit(self.org, 'PATCH', f'/issues/{root}', confirm)
            (code, original, _), (replay_code, replayed, _) = first.result(5), replay.result(5)
        self.assertEqual((code, replay_code), (200, 200), (original, replayed))
        self.assertFalse(original['move']['replayed'])
        self.assertTrue(replayed['move']['replayed'])
        self.assertEqual(replayed['move']['issues'], original['move']['issues'])

    def test_records_are_scoped_to_the_organisation_and_purged_after_seven_days(self):
        _, confirm = self.confirmation()
        root = self.ok('PATCH', '/issues/OPS-2', confirm)['id']
        # Another organisation never sees the record.
        other = self.issue('OPS-1', org=SECOND)
        self.org_problem(409, 'PATCH', f"/issues/{other['id']}",
                         dict(confirm, teamId=NETT.uid('second/team/OPS')), org=SECOND)
        status, value, _ = self.call('POST', '/__fixture/moves/age', dict(hours=6 * 24), token=None)
        self.assertEqual((status, value), (200, dict(records=1)))
        self.assertTrue(self.ok('PATCH', f'/issues/{root}', confirm)['move']['replayed'])
        status, value, _ = self.call('POST', '/__fixture/moves/age', dict(hours=25), token=None)
        self.assertEqual((status, value), (200, dict(records=0)))
        changed = self.org_problem(409, 'PATCH', f'/issues/{root}', confirm)
        self.assertEqual(changed['code'], 'IssueMove.PlanChanged')

    def test_a_slow_commit_checks_access_again_before_it_moves(self):
        status, _, _ = self.call('POST', '/__fixture/tokens', dict(token='grace-move-token', user='grace'), token=None)
        self.assertEqual(status, 200)
        _, confirm = self.confirmation()
        root = self.issue('OPS-2')['id']
        self.configure(slowMoveCommit=0.6)
        with ThreadPoolExecutor(1) as pool:
            pending = pool.submit(self.org, 'PATCH', f'/issues/{root}', confirm, token='grace-move-token')
            time.sleep(0.2)
            status, _, _ = self.call('POST', '/__fixture/members/revoke',
                                     dict(org=WORKS, user='grace', closeStreams=False), token=None)
            self.assertEqual(status, 200)
            code, value, _ = pending.result(5)
        self.assertEqual(code, 403, value)
        self.assertEqual(self.issue(root)['teamKey'], 'OPS', 'nothing moved')

    def test_a_replay_that_changes_the_confirmation_is_refused(self):
        _, confirm = self.confirmation()
        root = self.ok('PATCH', '/issues/OPS-2', confirm)['id']
        for changed in (dict(confirm, version=confirm['version'] + 1),
                        dict(confirm, teamId=OPS),
                        dict(confirm, reviewContext=dict(confirm['reviewContext'], issueContextVersion=9))):
            refused = self.org_problem(409, 'PATCH', f'/issues/{root}', changed)
            self.assertEqual(refused['code'], 'IssueMove.ReplayMismatch')
        self.assertTrue(self.ok('PATCH', f'/issues/{root}', confirm)['move']['replayed'])

    def test_the_move_settings_are_bounded(self):
        for settings in (dict(dropMoveReply=-1), dict(slowMoveCommit=11), dict(slowMoveCommit='1')):
            status, _, _ = self.call('POST', '/__fixture/config', settings, token=None)
            self.assertEqual(status, 400, settings)


class CapabilityTests(FixtureCase):
    def test_no_coming_feature_is_served_by_default(self):
        self.assertEqual(self.get('/desktop/capabilities'), dict(features=[]))
        issue = self.issue('OPS-1')
        gated = [('GET', '/me/initiatives', None), ('GET', '/fleets/fixture-fleet', None),
                 ('GET', '/issues/OPS-1/attachments', None),
                 ('POST', '/issues/OPS-1/attachments/prepare', {}),
                 ('POST', '/issues/OPS-1/attachments/from-upload', {}),
                 ('DELETE', f'/issues/OPS-1/attachments/pending/{uuid.uuid4()}', None),
                 ('DELETE', f'/issues/OPS-1/attachments/{uuid.uuid4()}?version=1', None),
                 ('POST', '/issues/OPS-1/move-preview', dict(destinationTeamId=WEB, version=issue['version'])),
                 ('POST', '/chat/uploads/prepare', {}), ('POST', '/chat/uploads/from-upload', {}),
                 ('DELETE', f'/chat/uploads/pending/{uuid.uuid4()}', None),
                 ('GET', '/chat/uploads/UPL-AAAAAAAAAAAAAAAAAAAAAA', None),
                 ('DELETE', '/chat/uploads/UPL-AAAAAAAAAAAAAAAAAAAAAA?version=1', None),
                 ('POST', '/chat/uploads/UPL-AAAAAAAAAAAAAAAAAAAAAA/grants', None)]
        for method, rest, body in gated:
            self.org_problem(404, method, rest, body)
        move = dict(teamId=WEB, version=issue['version'], reviewContext={}, movePreviewToken='0' * 64)
        self.org_problem(400, 'PATCH', '/issues/OPS-1', move)  # movePreviewToken is unknown until served.
        status, _, _ = self.call('GET', '/api/v1/updates?project=OPS', headers={'x-org-id': WORKS})
        self.assertEqual(status, 404)

    def test_the_command_line_offers_only_implemented_features(self):
        self.assertEqual(NETT.DESKTOP_FEATURES,
                         ('initiativeChannels', 'attachments', 'chatUploads', 'issueMoves', 'issueMoveReplay',
                          'updates'))
        result = subprocess.run([sys.executable, '-I', str(SERVER_PATH), '--port', '0', '--serve', 'repositories'],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)


class ServedCapabilityTests(FixtureCase):
    options = dict(served=NETT.DESKTOP_FEATURES)

    def test_every_eligible_organisation_serves_the_chosen_features(self):
        self.assertEqual(self.get('/desktop/capabilities'),
                         dict(features=['attachments', 'chatUploads', 'initiativeChannels', 'issueMoveReplay',
                                        'issueMoves', 'updates']))
        self.assertEqual(self.get('/desktop/capabilities', org=PERSONAL), dict(features=[]),
                         'a personal organisation without issue tracking is eligible for none')

    def test_a_hook_overrides_one_organisations_list_and_its_routes(self):
        status, value, _ = self.call('POST', '/__fixture/capabilities',
                                     dict(org=SECOND, features=['updates', 'issueSubscribers', 'someFutureFlag']),
                                     token=None)
        self.assertEqual((status, value['features']), (200, ['updates', 'issueSubscribers', 'someFutureFlag']))
        self.assertEqual(self.get('/desktop/capabilities', org=SECOND)['features'],
                         ['updates', 'issueSubscribers', 'someFutureFlag'])
        self.org_problem(404, 'GET', '/me/initiatives', org=SECOND)
        self.assertEqual(len(self.get('/me/initiatives')['items']), 4, 'other organisations keep the default')
        self.call('POST', '/__fixture/capabilities', dict(org=SECOND, features=None), token=None)
        self.assertEqual(len(self.get('/desktop/capabilities', org=SECOND)['features']), 6)
        for body in (dict(org=SECOND, features='updates'), dict(org=SECOND, features=['a', 'a']),
                     dict(org=SECOND, features=[''])):
            self.problem(400, 'POST', '/__fixture/capabilities', body, token=None)
        self.problem(404, 'POST', '/__fixture/capabilities', dict(org=NETT.uid('nope'), features=[]), token=None)

    def test_discovery_has_paths_stakeholders_and_null_fleets_without_archived_branches(self):
        items = self.get('/me/initiatives')['items']
        self.assertEqual([i['name'] for i in items], ['Desktop', 'Empty', 'Platform', 'Research'])
        rows = {i['name']: i for i in items}
        self.assertEqual(set(rows['Platform']), {'id', 'name', 'path', 'channels', 'stakeholders', 'fleetId', 'fleet',
                                                 'version'})
        self.assertEqual(rows['Desktop']['path'], ['Platform', 'Desktop'])
        self.assertEqual(rows['Platform']['path'], ['Platform'])
        self.assertEqual((rows['Research']['fleetId'], rows['Research']['fleet']), (None, None))
        self.assertEqual(rows['Platform']['stakeholders'][0]['role'], 'Coordinator')
        self.assertEqual(set(rows['Platform']['stakeholders'][0]),
                         {'id', 'name', 'role', 'slackHandle', 'teamsHandle', 'version'})
        fleet = rows['Platform']['fleet']
        self.assertEqual(rows['Platform']['fleetId'], fleet['id'])
        self.assertEqual((fleet['ircHost'], fleet['ircPort'], fleet['ircTls']), ('irc.example.test', 6697, True))
        self.assertEqual(self.get(f"/fleets/{fleet['id']}")['slug'], 'fixture-fleet')
        self.assertEqual(self.get('/fleets/fixture-fleet')['id'], fleet['id'])
        first = self.get('/me/initiatives?limit=3')
        self.assertEqual([i['name'] for i in first['items']], ['Desktop', 'Empty', 'Platform'])
        rest = self.get('/me/initiatives?limit=3&cursor=' + first['nextCursor'])
        self.assertEqual(([i['name'] for i in rest['items']], rest['nextCursor']), (['Research'], None))
        self.org_problem(400, 'GET', '/me/initiatives?cursor=' + self.get('/initiatives?limit=1')['nextCursor'])



def hook(case, path, body):
    status, value, _ = case.call('POST', f'/__fixture/{path}', body, token=None)
    case.assertEqual(status, 200, value)
    return value


GRACE_TOKEN = 'grace-fixture-token'


class OrgHeaderTests(FixtureCase):
    def raw(self, path, headers):
        """A GET with exactly these headers, repeats included."""
        connection = http.client.HTTPConnection(urlsplit(self.server.origin).netloc, timeout=5)
        self.addCleanup(connection.close)
        connection.putrequest('GET', path)
        connection.putheader('Authorization', f'Bearer {TOKEN}')
        for name, value in headers:
            connection.putheader(name, value)
        connection.endheaders()
        response = connection.getresponse()
        return response.status, json.loads(response.read())

    def test_every_organisation_route_needs_exactly_one_matching_header(self):
        for rest in ('/issues', '/members', '/desktop/capabilities', '/issues/OPS-1/comments'):
            self.org_problem(403, 'GET', rest, org_header=False)
            self.org_problem(403, 'GET', rest, headers={'x-org-id': SECOND})
            self.org_problem(400, 'GET', rest, headers={'x-org-id': 'not-a-uuid'})
            status, body = self.raw(f'/api/v1/orgs/{WORKS}{rest}', [('x-org-id', WORKS), ('x-org-id', WORKS)])
            self.assertEqual((status, body['status']), (400, 400), rest)
            self.assertEqual(self.org('GET', rest, headers={'x-org-id': WORKS.upper()})[0], 200, rest)
        # A stranger still gets the neutral 404, whatever the header says.
        self.org_problem(404, 'GET', '/issues', org=OUTSIDE)
        self.org_problem(404, 'POST', '/issues/OPS-1/comments', dict(body='x'), org_header=False, org=OUTSIDE)


class CallerTests(FixtureCase):
    def setUp(self):
        super().setUp()
        hook(self, 'tokens', dict(token=GRACE_TOKEN, user='grace'))

    def test_a_registered_token_acts_as_its_person(self):
        context = self.call('GET', '/api/v1/auth/context', token=GRACE_TOKEN)[1]
        self.assertEqual((context['userId'], context['user']['firstName']), (NETT.GRACE['userId'], 'Grace'))
        self.assertEqual(sorted(org['name'] for org in context['organisations']),
                         ['Fixture Works', 'Outside Fixture Org', 'Second Fixture Org'])
        self.assertEqual(self.org('GET', '/issues', org=OUTSIDE, token=GRACE_TOKEN)[0], 200)
        self.org_problem(404, 'GET', '/members', org=PERSONAL, token=GRACE_TOKEN)
        posted = self.ok('POST', '/issues/OPS-1/comments', dict(body='From Grace'), status=201, token=GRACE_TOKEN)
        self.assertEqual(posted['actualAuthor']['userId'], NETT.GRACE['userId'])
        mine = self.get('/issues?assignee=me&limit=100', token=GRACE_TOKEN)['items']
        self.assertTrue(mine and all(item['assignee']['userId'] == NETT.GRACE['userId'] for item in mine))

    def test_a_revoked_membership_is_refused_while_another_person_keeps_access(self):
        self.assertEqual(self.org('GET', '/issues/OPS-1', token=GRACE_TOKEN)[0], 200)
        hook(self, 'members/revoke', dict(org=WORKS, user='grace'))
        self.org_problem(403, 'GET', '/issues/OPS-1', token=GRACE_TOKEN)
        self.org_problem(403, 'POST', '/files/grants', dict(publicFileIds=['pf_x']), token=GRACE_TOKEN)
        self.assertEqual(self.org('GET', '/issues/OPS-1')[0], 200, 'Ada keeps access')
        names = [org['name'] for org in self.call('GET', '/api/v1/auth/context', token=GRACE_TOKEN)[1]['organisations']]
        self.assertNotIn('Fixture Works', names)
        hook(self, 'members/restore', dict(org=WORKS, user='grace'))
        self.assertEqual(self.org('GET', '/issues/OPS-1', token=GRACE_TOKEN)[0], 200)

    def test_a_revoked_token_is_refused_everywhere(self):
        hook(self, 'tokens/revoke', dict(token=GRACE_TOKEN))
        self.problem(401, 'GET', '/api/v1/auth/context', token=GRACE_TOKEN)
        self.org_problem(401, 'GET', '/issues', token=GRACE_TOKEN)

    def test_planning_projects_can_be_hidden_from_one_person(self):
        initiative = next(i['id'] for i in self.get('/initiatives')['items'] if i['name'] == 'Platform')
        names = lambda rest, token=TOKEN: [p['name'] for p in self.get(rest, token=token)['items']]
        self.assertIn('Launch', names(f'/initiatives/{initiative}/projects', GRACE_TOKEN))
        hook(self, 'projects/hide', dict(org=WORKS, project='Launch', user='grace'))
        self.assertNotIn('Launch', names(f'/initiatives/{initiative}/projects', GRACE_TOKEN))
        self.assertNotIn('Launch', names('/projects', GRACE_TOKEN))
        self.assertIn('Launch', names(f'/initiatives/{initiative}/projects'))
        self.assertIn('Launch', names('/projects'))


class FailureHookTests(FileCase):
    options = dict(served=('attachments',))

    def test_a_configured_failure_answers_once_with_its_retry_after(self):
        path = f'/api/v1/orgs/{WORKS}/issues/OPS-1'
        self.configure(failNext=[dict(method='GET', path=path, status=503, retryAfter=45, times=1),
                                 dict(method='POST', path=path + '/comments', status=429, retryAfter=2)])
        code, value, headers = self.call('GET', path)
        self.assertEqual((code, headers['Retry-After'], value['status']), (503, '45', 503))
        self.assertEqual(self.call('GET', path)[0], 200)
        code, _, headers = self.call('POST', path + '/comments', dict(body='Busy'))
        self.assertEqual((code, headers['Retry-After']), (429, '2'))
        self.ok('POST', '/issues/OPS-1/comments', dict(body='Not busy'), status=201)
        self.assertEqual(self.call('POST', '/__fixture/config', dict(failNext=[dict(method='GET')]), token=None)[0], 400)

    def test_a_slow_body_arrives_after_its_headers(self):
        self.configure(slowBody=dict(path='/api/v1/auth/context', seconds=0.8, times=1))
        connection = http.client.HTTPConnection(urlsplit(self.server.origin).netloc, timeout=5)
        self.addCleanup(connection.close)
        started = time.monotonic()
        connection.request('GET', '/api/v1/auth/context', headers={'Authorization': f'Bearer {TOKEN}'})
        response = connection.getresponse()
        headers_at = time.monotonic() - started
        body = json.loads(response.read())
        body_at = time.monotonic() - started
        self.assertEqual((response.status, body['userId']), (200, NETT.ADA['userId']))
        self.assertLess(headers_at, 0.6)
        self.assertGreaterEqual(body_at, 0.75)
        started = time.monotonic()
        self.call('GET', '/api/v1/auth/context')
        self.assertLess(time.monotonic() - started, 0.6, 'only once')
        # A path ending in * slows every path under it.
        self.configure(slowBody=dict(path='/api/v1/auth/*', seconds=0.8, times=1))
        started = time.monotonic()
        self.call('GET', '/api/v1/auth/context')
        self.assertGreaterEqual(time.monotonic() - started, 0.75)

    def test_a_held_body_logs_the_sent_headers_and_can_be_released(self):
        path = '/api/v1/auth/context'
        self.configure(slowBody=dict(path=path, seconds=10, waitForRelease=True))
        with ThreadPoolExecutor() as pool:
            waiting = pool.submit(self.call, 'GET', path)
            try:
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    log = self.call('GET', '/__fixture/requests', token=None)[1]
                    if log and log[-1].get('slowBodyHeadersStatus') == 200:
                        break
                    time.sleep(0.02)
                else:
                    self.fail('the fixture did not record the sent headers')
                self.assertFalse(waiting.done())
            finally:
                self.configure(slowBody=None)
            self.assertEqual(waiting.result(timeout=2)[0], 200)

    def test_a_dropped_upload_acknowledgement_still_stores_the_bytes(self):
        data = b'# Restore notes\n'
        upload = self.ok('POST', '/issues/OPS-1/attachments/prepare', self.declaration(data))
        self.configure(dropPutResponse=1)
        with self.assertRaises((http.client.RemoteDisconnected, ConnectionError, http.client.BadStatusLine,
                                OSError)):
            self.put(upload, data)
        attachment = self.ok('POST', '/issues/OPS-1/attachments/from-upload', dict(uploadId=upload['uploadId']),
                             status=201)
        self.assertEqual(attachment['sizeBytes'], len(data))
        self.assertEqual(self.put(upload, data)[0], 409, 'the bytes went once')


    def test_a_dropped_answer_is_served_but_never_arrives(self):
        path = f'/api/v1/orgs/{WORKS}/issues/OPS-1/attachments/from-upload'
        data = b'# Completed, unanswered\n'
        upload = self.ok('POST', '/issues/OPS-1/attachments/prepare', self.declaration(data))
        self.assertIn(self.put(upload, data)[0], (200, 201, 204))
        self.configure(dropNext=[dict(method='POST', path=path, times=1)])
        with self.assertRaises((http.client.RemoteDisconnected, ConnectionError, http.client.BadStatusLine,
                                OSError)):
            self.call('POST', path, dict(uploadId=upload['uploadId']))
        # The completion happened: repeating it replays the same record.
        replay = self.ok('POST', '/issues/OPS-1/attachments/from-upload', dict(uploadId=upload['uploadId']),
                         status=200)
        self.assertEqual(replay['sizeBytes'], len(data))
        self.assertEqual(self.call('POST', '/__fixture/config', dict(dropNext=[dict(method='POST')]),
                                   token=None)[0], 400)

    def test_an_expired_reservation_refuses_its_request_key(self):
        data = b'# Reserved, then lapsed\n'
        declaration = self.declaration(data)
        self.ok('POST', '/issues/OPS-1/attachments/prepare', declaration)
        status, value, _ = self.call('POST', '/__fixture/transfers/expire', {}, token=None)
        self.assertEqual((status, value['expired']), (200, 1))
        refused = self.org_problem(409, 'POST', '/issues/OPS-1/attachments/prepare', declaration)
        self.assertIn('different or finished', refused['detail'])

    def test_only_the_numeric_origin_is_answered(self):
        connection = http.client.HTTPConnection(urlsplit(self.server.origin).netloc, timeout=5)
        self.addCleanup(connection.close)
        connection.putrequest('GET', '/__fixture/requests', skip_host=True)
        connection.putheader('Host', f'localhost:{self.server.server_port}')
        connection.endheaders()
        self.assertEqual(connection.getresponse().status, 421)

    def test_a_cross_origin_form_cannot_clear_the_request_log(self):
        self.assertEqual(self.call('GET', '/api/v1/me')[0], 200)
        for origin in ('http://127.0.0.1:1', 'null'):
            self.problem(403, 'POST', '/__fixture/requests/clear', form={'clear': 'true'},
                         token=None, headers={'Origin': origin})
        self.assertEqual(len(self.call('GET', '/__fixture/requests', token=None)[1]), 1)
        status, result, _ = self.call('POST', '/__fixture/requests/clear', token=None,
                                      headers={'Origin': self.server.origin})
        self.assertEqual((status, result['cleared']), (200, 1))

    def test_a_trickling_upload_expires_without_blocking_other_requests(self):
        body, data = self.upload(data=b'x' * 100, draftId=str(uuid.uuid4()))
        prepared = self.ok('POST', '/issues/media/prepare', body)
        url = urlsplit(prepared['upload']['uploadUrl'])
        slow = http.client.HTTPConnection(url.netloc, timeout=3)
        self.addCleanup(slow.close)
        stop = threading.Event()

        def trickle():
            try:
                while not stop.wait(0.08):
                    slow.send(b'x')
            except OSError:
                pass  # The deadline closes the connection.

        with patch.object(NETT.Handler, 'body_timeout', 1), ThreadPoolExecutor() as pool:
            slow.putrequest('PUT', f'{url.path}?{url.query}')
            slow.putheader('Content-Length', str(len(data)))
            slow.putheader('Content-Type', body['contentType'])
            slow.endheaders(b'x')
            sending = pool.submit(trickle)
            try:
                # The log is taken at arrival. It must stay readable while the
                # incomplete request is still receiving bytes.
                with closing(http.client.HTTPConnection(url.netloc, timeout=0.7)) as probe:
                    for _ in range(10):
                        probe.request('GET', '/__fixture/requests')
                        reply = probe.getresponse()
                        if any(r['path'] == url.path for r in json.loads(reply.read())):
                            break
                        time.sleep(0.02)
                    else:
                        self.fail('the fixture did not record the incoming upload')
                self.assertEqual(slow.getresponse().status, 408)
            finally:
                stop.set()
            sending.result()
        self.assertEqual(self.transfer(prepared, data)[0], 204, 'a partial body did not commit')

    def test_request_bodies_received_at_once_share_one_budget(self):
        self.server.intake = NETT.BodyIntake(150)
        netloc = urlsplit(self.server.origin).netloc
        path = f'/api/v1/orgs/{WORKS}/issues/media/prepare'

        def post(connection, size, first=b''):
            connection.putrequest('POST', path)
            for name, value in (('Authorization', f'Bearer {TOKEN}'), ('x-org-id', WORKS),
                                ('Content-Type', 'application/json'), ('Content-Length', str(size))):
                connection.putheader(name, value)
            connection.endheaders(first or None)

        def held(size):
            deadline = time.monotonic() + 3
            while self.server.intake.held != size:
                self.assertLess(time.monotonic(), deadline, f'{self.server.intake.held} bytes held')
                time.sleep(0.01)

        slow = http.client.HTTPConnection(netloc, timeout=3)
        self.addCleanup(slow.close)
        # A body that has its place but has not all arrived yet.
        post(slow, 100, b'{')
        held(100)
        with patch.object(NETT.Handler, 'body_timeout', 0.3), \
                closing(http.client.HTTPConnection(netloc, timeout=3)) as second:
            # Another that would not fit beside it waits for room, then is
            # refused before any of its bytes are read.
            post(second, 100)
            started = time.monotonic()
            reply = second.getresponse()
            self.assertEqual((reply.status, reply.getheader('Retry-After')), (503, '1'))
            self.assertGreaterEqual(time.monotonic() - started, 0.25)
            reply.read()
        held(100)
        # Once the first is answered, its place goes and the next fits.
        slow.send(b' ' * 98 + b'}')
        slow.getresponse().read()
        held(0)
        status = self.org('POST', '/issues/media/prepare', raw=b'{' + b' ' * 98 + b'}',
                          headers={'Content-Type': 'application/json'})[0]
        self.assertNotEqual(status, 503)
        held(0)

    def test_a_request_body_is_not_held_once_its_place_in_the_intake_goes(self):
        held = []
        respond = NETT.Handler.respond

        def recording(handler, status, value):
            held.append((len(handler.body), self.server.intake.held))
            return respond(handler, status, value)

        body, _ = self.upload(data=b'x' * 100, draftId=str(uuid.uuid4()))
        with patch.object(NETT.Handler, 'respond', recording):
            self.ok('POST', '/issues/media/prepare', body)
        # By the time the answer is written, which for a stream lasts
        # minutes, neither the bytes nor their place are held.
        self.assertEqual(held, [(0, 0)])

    def test_the_request_log_says_when_each_request_arrived(self):
        hook(self, 'requests/clear', {})
        self.call('GET', '/api/v1/auth/context')
        time.sleep(0.2)
        self.call('GET', '/api/v1/auth/context')
        first, second = self.server.requests[-2:]
        self.assertGreaterEqual(second['at'] - first['at'], 0.19)


class StreamReleaseTests(StreamCase):
    options = dict(served=('updates',))

    def test_a_refused_stream_request_frees_its_place(self):
        netloc = urlsplit(self.server.origin).netloc
        for _ in range(NETT.MAX_STREAMS_PER_USER + 1):
            connection = http.client.HTTPConnection(netloc, timeout=5)
            self.addCleanup(connection.close)
            connection.request('GET', '/api/v1/updates?project=OPS&unsupported=1',
                               headers={'Authorization': f'Bearer {TOKEN}', 'x-org-id': WORKS})
            response = connection.getresponse()
            self.assertEqual(response.status, 400)
            response.read()
        self.assertEqual(self.open_stream().status, 200, 'refused requests hold no stream')

    def test_a_failed_header_write_releases_the_subscriber(self):
        self.server.max_streams_per_user = 1
        with patch.object(NETT.Handler, 'end_headers', side_effect=BrokenPipeError):
            with self.assertRaises(http.client.RemoteDisconnected):
                self.open_stream()
        with self.server.lock:
            self.assertEqual(self.server.subscribers, [])
        self.ready(self.open_stream())

    def test_a_stalled_batch_uses_a_short_socket_timeout_and_releases_its_slot(self):
        self.server.max_streams_per_user = 1
        response = self.open_stream()
        self.ready(response)
        sendall = socket.socket.sendall
        timeouts = []

        def stall_batch(connection, frame, *args):
            if frame.startswith(b'event: issueChanged'):
                timeouts.append(connection.gettimeout())
                raise socket.timeout
            return sendall(connection, frame, *args)

        with patch.object(socket.socket, 'sendall', stall_batch):
            hook(self, 'touch', dict(org=WORKS, issue='OPS-1'))
            self.assertEqual(self.ends_quickly(response), b'')
        self.assertEqual(timeouts, [NETT.STREAM_WRITE_SECONDS])
        self.ready(self.open_stream())


class StreamRefusalTests(StreamCase):
    options = dict(stream_seconds=6, keepalive_seconds=0.3, served=('updates',))

    def test_a_stream_can_be_refused_with_401_or_403_and_no_retry_after(self):
        for status in (401, 403):
            hook(self, 'config', dict(streamRefusal=dict(status=status, retryAfter=0, times=1)))
            response = self.refused(status)
            self.assertIsNone(response.getheader('Retry-After'))
        self.ready(self.open_stream())

    def test_an_aborted_stream_resets_the_connection(self):
        response = self.open_stream()
        self.ready(response)
        hook(self, 'updates/close', dict(abort=True))
        ended_cleanly = False
        with self.assertRaises((ConnectionResetError, http.client.IncompleteRead)):
            started = time.monotonic()
            while time.monotonic() - started < 3:
                if not response.read(1):
                    ended_cleanly = True
                    break
            if ended_cleanly:
                self.fail('the stream ended instead of being reset')


class StreamAuthorityTests(StreamCase):
    options = dict(stream_seconds=6, keepalive_seconds=0.3, served=('updates',))

    def setUp(self):
        super().setUp()
        hook(self, 'tokens', dict(token=GRACE_TOKEN, user='grace'))

    def test_queued_events_are_withheld_once_membership_is_revoked(self):
        response = self.open_stream(token=GRACE_TOKEN)
        self.ready(response)
        subscriber = next(s for s in self.server.subscribers if s.token == GRACE_TOKEN)
        with subscriber.ready:  # Queue a hint, then lose membership before the stream flushes it.
            subscriber.push(dict(projectKey='OPS', issueId=NETT.uid('x'), identifier='OPS-1', version=9))
            hook(self, 'members/revoke', dict(org=WORKS, user='grace', closeStreams=False))
        self.assertNotIn(b'issueChanged', self.ends_quickly(response))
        self.refused(403, token=GRACE_TOKEN)

    def test_revocation_waits_for_delivery_already_authorised(self):
        self.server.keepalive_seconds = 5
        response = self.open_stream(token=GRACE_TOKEN)
        self.ready(response)
        subscriber = next(s for s in self.server.subscribers if s.token == GRACE_TOKEN)
        authorised, resume, revoked = threading.Event(), threading.Event(), threading.Event()
        check, close = self.server.authorised, subscriber.close

        def pause_after_authorising(stream):
            allowed = check(stream)
            if stream is subscriber and allowed:
                authorised.set()
                resume.wait()
            return allowed

        def close_stream(reason):
            close(reason)
            revoked.set()

        with patch.object(self.server, 'authorised', pause_after_authorising), \
                patch.object(subscriber, 'close', close_stream), ThreadPoolExecutor() as pool:
            try:
                hook(self, 'touch', dict(org=WORKS, issue='OPS-1'))
                self.assertTrue(authorised.wait(2), 'the stream did not authorise the queued event')
                request = pool.submit(hook, self, 'members/revoke', dict(org=WORKS, user='grace'))
                self.assertTrue(revoked.wait(2), 'the revoke hook did not close the stream')
                self.org_problem(403, 'GET', '/issues/OPS-1', token=GRACE_TOKEN)
                with self.assertRaises(FutureTimeout, msg='revocation answered with a write in flight'):
                    request.result(timeout=0.15)
            finally:
                resume.set()
            self.assertEqual(request.result(timeout=2)['closed'], 1)
        self.assertNotIn(b'issueChanged', self.ends_quickly(response))

    def test_an_admitted_person_keeps_receiving_while_a_denied_one_is_closed(self):
        ada, grace = self.open_stream(), self.open_stream(token=GRACE_TOKEN)
        self.ready(ada)
        self.ready(grace)
        hook(self, 'members/revoke', dict(org=WORKS, user='grace', closeStreams=False))
        self.call('POST', '/__fixture/touch', dict(org=WORKS, issue='OPS-1'), token=None)
        found = self.records(ada, lambda found: any(name == 'issueChanged' for name, _ in found))
        self.assertEqual([data['identifier'] for name, data in found if name == 'issueChanged'], ['OPS-1'])
        self.assertNotIn(b'issueChanged', self.ends_quickly(grace))

    def test_a_revoked_token_closes_its_stream_and_cannot_reopen_it(self):
        response = self.open_stream(token=GRACE_TOKEN)
        self.ready(response)
        self.assertEqual(hook(self, 'tokens/revoke', dict(token=GRACE_TOKEN))['closed'], 1)
        self.assertNotIn(b'issueChanged', self.ends_quickly(response))
        self.refused(401, token=GRACE_TOKEN)


@unittest.skipUnless(OPENAPI, 'Pass --openapi PATH or set NETT_OPENAPI to check the NETT contract.')
class OpenApiTests(FixtureCase):
    @classmethod
    def setUpClass(cls):
        cls.contract = json.loads(Path(OPENAPI).read_text())

    def api_routes(self):
        """Every API route NETT already serves; the update stream is the change it is to make."""
        return {(route.method, route.template) for route in NETT.ROUTES
                if route.template.startswith('/api/v1/')} - NETT.NETT_GAPS

    def test_every_api_route_exists_in_the_contract(self):
        paths = self.contract['paths']
        missing = sorted(f'{method} {template}' for method, template in self.api_routes()
                         if method.lower() not in paths.get(template, {}))
        self.assertEqual(missing, [])

    def sample(self, method, template, path, body=None, status=200, headers=None):
        code, value, _ = self.call(method, path, body, headers=headers)
        self.assertEqual(code, status, (method, path, value))
        response = self.contract['paths'][template][method.lower()]['responses'][str(status)]
        schema = response['content']['application/json']['schema']
        self.assertEqual(list(undeclared(value, schema, self.contract['components']['schemas'])), [],
                         f'{method} {template}')
        self.sampled.add((method, template))
        return value

    def test_sample_responses_use_only_declared_properties(self):
        self.sampled = set()
        org = '/api/v1/orgs/{orgId}'
        base = f'/api/v1/orgs/{WORKS}'
        self.sample('GET', '/api/v1/me', '/api/v1/me')
        self.sample('GET', '/api/v1/auth/context', '/api/v1/auth/context', headers={'x-org-id': WORKS})
        self.sample('GET', '/api/v1/auth/orgs', '/api/v1/auth/orgs')
        self.sample('GET', org + '/issue-teams', base + '/issue-teams')
        self.sample('GET', org + '/issue-teams/{teamRef}', base + '/issue-teams/OPS')
        self.sample('GET', org + '/issue-teams/{teamId}/workflow-states', f'{base}/issue-teams/{OPS}/workflow-states')
        self.sample('GET', org + '/issues', base + '/issues?includeArchived=true')
        self.sample('GET', org + '/issues/groups', base + '/issues/groups?groupBy=state&team=OPS&limitPerGroup=1')
        for ref in ('OPS-1', 'OPS-2', 'OPS-3', 'WEB-3'):
            self.sample('GET', org + '/issues/{issueRef}', f'{base}/issues/{ref}')
        issue_id = self.sample('GET', org + '/issues/{issueRef}', base + '/issues/OPS-1')['id']
        self.sample('POST', org + '/issues', base + '/issues',
                    dict(teamId=OPS, title='Contract check', labelIds=[BUG], projectId=LAUNCH), status=201)
        self.sample('PATCH', org + '/issues/{issueRef}', base + '/issues/OPS-1', dict(version=1, priority=1))
        self.sample('GET', org + '/issues/{issueRef}/comments', base + '/issues/OPS-1/comments')
        self.sample('GET', org + '/issues/{issueRef}/comments/{rootId}/replies',
                    f'{base}/issues/OPS-1/comments/{FIRST_ROOT}/replies')
        self.sample('POST', org + '/issues/{issueRef}/comments', base + '/issues/OPS-1/comments',
                    dict(body='Contract check'), status=201)
        self.sample('GET', org + '/issues/{issueRef}/relations', base + '/issues/OPS-1/relations')
        self.sample('GET', org + '/issues/{issueRef}/activity', base + '/issues/OPS-1/activity')
        self.sample('GET', org + '/issues/{issueRef}/pull-requests', base + '/issues/OPS-1/pull-requests')
        self.sample('GET', org + '/issues/{issueRef}/images', base + '/issues/OPS-1/images?limit=1')
        public = self.sample('GET', org + '/issues/media/{mediaId}', f'{base}/issues/media/{IMAGE}')['media']['publicFileId']
        self.sample('POST', org + '/files/grants', base + '/files/grants', dict(publicFileIds=[public]),
                    headers={'x-org-id': WORKS})
        draft = str(uuid.uuid4())
        body, data = self.upload(issueId=issue_id, draftId=draft)
        prepared = self.sample('POST', org + '/issues/media/prepare', base + '/issues/media/prepare', body)
        self.transfer(prepared, data)
        self.sample('POST', org + '/issues/media/from-upload', base + '/issues/media/from-upload',
                    dict(uploadId=prepared['uploadId']), status=202)
        self.sample('GET', org + '/issues/media/uploads/{uploadId}', f"{base}/issues/media/uploads/{prepared['uploadId']}")
        self.sample('POST', org + '/issues/media/from-upload', base + '/issues/media/from-upload',
                    dict(uploadId=prepared['uploadId']))
        self.sample('POST', org + '/issues/{issueRef}/comments', base + '/issues/OPS-1/comments',
                    dict(body='With media', mediaDraftId=draft), status=201)
        body, data = self.upload('issueDescriptionDraft', draftId=str(uuid.uuid4()))
        doomed = self.sample('POST', org + '/issues/media/prepare', base + '/issues/media/prepare', body)
        self.sample('PUT', org + '/issues/media/{mediaId}/upload', f"{base}/issues/media/{doomed['mediaId']}/upload",
                    dict(version=1, cancel=True))
        lists = f'{base}/issues/{issue_id}/lists'
        self.sample('GET', org + '/issues/{issueId}/lists', lists)
        items = org + '/issues/{issueId}/lists/{listId}/items'
        self.sample('GET', items, f'{lists}/{CHECKLIST}/items')
        item = self.sample('POST', items, f'{lists}/{CHECKLIST}/items', dict(text='Contract check'), status=201)['item']
        self.sample('PUT', items + '/{itemId}', f"{lists}/{CHECKLIST}/items/{item['id']}", dict(version=1, completed=True))
        self.sample('GET', org + '/members', base + '/members?status=all')
        self.sample('GET', org + '/labels', base + '/labels?includeInactive=true')
        self.sample('GET', org + '/issue-cycles', base + '/issue-cycles?team=OPS')
        self.sample('GET', org + '/projects', base + '/projects')
        self.sample('GET', org + '/projects/{projectId}/milestones', f'{base}/projects/{LAUNCH}/milestones')
        self.sample('GET', org + '/mention-candidates', base + '/mention-candidates?kind=issue')
        listed = self.sample('GET', org + '/initiatives', base + '/initiatives?includeArchived=true')['items']
        platform = next(i['id'] for i in listed if i['name'] == 'Platform')
        self.sample('GET', org + '/initiatives/{initiativeId}', f'{base}/initiatives/{platform}')
        self.sample('GET', org + '/initiatives/{initiativeId}/projects', f'{base}/initiatives/{platform}/projects')
        decisions = self.sample('GET', org + '/decisions', base + '/decisions?status=all')['items']
        for decision in decisions:
            self.sample('GET', org + '/decisions/{decisionId}', f"{base}/decisions/{decision['id']}")
        self.assertEqual(sorted(self.api_routes() - self.sampled), [], 'every API route has a sample')


if __name__ == '__main__':
    unittest.main()
