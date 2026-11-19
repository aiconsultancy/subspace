#!/usr/bin/env python3
"""Offline HTTP contract checks. Works with python3 -I and uses only the stdlib."""

import importlib.util
import json
import sys
import uuid
from datetime import date, datetime
from pathlib import Path
import threading
import unittest
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

SPEC = importlib.util.spec_from_file_location('fixture', Path(__file__).with_name('subspace_api_test_server.py'))
FIXTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIXTURE)

ISSUE_FIELDS = set('archivedAt assignee owner blocked commentCount createdAt cycle cycleId dueDate estimate id '
                   'identifier labels lastActivity lastActivityAt milestoneId parentId priority progress projectId '
                   'projectKey projectRank provenance pullRequestSummary relayedCrew state subIssueRank summary title todoCounts type updatedAt version'.split())
COMMENT_FIELDS = set('actualAuthor body createdAt deletedAt displayCreatedAt editedAt hasReplies id issueId media mentions '
                     'parentCommentId provenance relayedCrew replyCount resolved resolvedAt resolvedBy version'.split())
DETAIL_FIELDS = set('archivedBy canceledAt children completedAt createdBy description duplicateOf mentions parent '
                    'relations startedAt subscription todos'.split())


class FixtureTests(unittest.TestCase):
    def setUp(self):
        self.server = FIXTURE.FixtureServer(0, 'fixture-access')
        self.worker = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.worker.start()
        self.addCleanup(self.close_server)

    def close_server(self):
        self.server.shutdown()
        self.server.server_close()
        self.worker.join()

    def request(self, path, body=None, jwt='fixture-access', method=None):
        headers = {'User-Agent': 'Subspace/fixture', 'Content-Type': 'application/json'}
        if jwt is not None:
            headers['Authorization'] = f'Bearer {jwt}'
        data = json.dumps(body).encode() if body is not None else None
        request = Request(self.server.origin + '/api/v1' + path, data=data, headers=headers, method=method)
        try:
            response = urlopen(request, timeout=3)
        except HTTPError as error:
            response = error
        with response:
            return response.status, json.load(response), response.headers

    def get(self, path):
        status, value, headers = self.request(path)
        self.assertEqual(status, 200, value)
        self.assertEqual(headers['Content-Type'], 'application/json')
        self.assertEqual(headers['X-Correlation-Id'], 'fixture')
        return value

    def updates(self, *projects):
        query = urlencode([('project', key) for key in projects])
        headers = {'Authorization': 'Bearer fixture-access', 'Accept': 'text/event-stream'}
        response = urlopen(Request(self.server.origin + '/api/v1/updates?' + query, headers=headers), timeout=3)
        self.addCleanup(response.close)
        self.assertEqual(response.status, 200)
        self.assertEqual(response.headers['Content-Type'], 'text/event-stream')
        self.assertEqual(response.headers['X-Content-Type-Options'], 'nosniff')
        self.assertEqual(self.next_event(response), ('ready', None))
        return response

    def next_event(self, response):
        name = response.readline().decode().removeprefix('event: ').strip()
        data = response.readline().decode().removeprefix('data: ')
        self.assertEqual(response.readline(), b'\n')
        return name, json.loads(data)

    def test_updates_stream_scopes_hints_and_sends_them_after_a_write(self):
        source = self.updates('SUB')
        target = self.updates('ops')
        issue = self.get('/issues/SUB-2')
        status, saved, _ = self.request('/issues/SUB-2', dict(version=issue['version'], title='Changed elsewhere'),
                                        method='PATCH')
        self.assertEqual(status, 200)
        kind, event = self.next_event(source)
        self.assertEqual(kind, 'issueChanged')
        self.assertEqual((event['identifier'], event['version'], event['projectKey']), ('SUB-2', saved['version'], 'SUB'))
        self.assertEqual(set(event), set('projectKey issueId identifier parentIdentifier version auditId action '
                                         'actorType occurredAt'.split()))
        self.assertEqual(event['action'], 'issue.changed')
        self.assertNotEqual(uuid.UUID(event['auditId']).int, 0)
        self.assertEqual(self.request('/issues/SUB-2/comments', {'body': 'Elsewhere'})[0], 201)
        _, event = self.next_event(source)
        self.assertEqual((event['action'], event['version']), ('comment.created', saved['version']))
        self.assertEqual(self.upload('/issues/SUB-2/attachments', 'notes.txt', 'text/plain', b'notes')[0], 201)
        _, event = self.next_event(source)
        self.assertEqual((event['action'], event['version']), ('attachment.added', saved['version']))
        self.assertEqual(self.get('/issues/SUB-2')['version'], saved['version'])
        root = self.get('/issues/SUB-1')
        project = self.get('/projects/OPS')
        move = dict(version=root['version'], destinationProjectId=project['id'])
        preview = self.request('/issues/SUB-1/move-preview', move)[1]
        self.assertEqual(self.request('/issues/SUB-1/move', dict(move, token=preview['token']))[0], 200)
        moved = [self.next_event(source)[1] for _ in preview['issues']]
        self.assertEqual([(e['identifier'], e['projectKey'], e['action']) for e in moved],
                         [(i['newIdentifier'], 'OPS', 'issue.changed') for i in preview['issues']])
        self.assertEqual([self.next_event(target)[1] for _ in moved], moved)

    def test_a_later_change_elsewhere_bumps_the_version_and_hints(self):
        response = self.updates('SUB', 'OPS')
        self.server.change_after = 0.1
        before = self.get('/issues/SUB-1')
        _, event = self.next_event(response)
        after = self.get('/issues/SUB-1')
        self.assertEqual((event['version'], event['action']), (before['version'] + 1, 'issue.changed'))
        self.assertEqual(after['title'], before['title'] + ' (changed elsewhere)')

    def test_updates_stream_can_be_absent_and_checks_authentication_and_projects(self):
        self.assertEqual(self.request('/updates?project=SUB', jwt=None)[0], 401)
        self.assertEqual(self.request('/updates')[0], 400)
        self.assertEqual(self.request('/updates?project=SUB%23')[0], 400)
        self.assertEqual(self.request('/updates?project=SUB&project=ZZZ')[0], 404)
        self.server.updates.enabled = False
        self.assertEqual(self.request('/updates?project=SUB')[0], 404)

    def test_move_preview_confirms_tracker_children_and_old_identifiers_still_resolve(self):
        root = self.get('/issues/SUB-1')
        target = self.get('/projects/OPS')
        status, preview, _ = self.request('/issues/SUB-1/move-preview', dict(version=root['version'], destinationProjectId=target['id']))
        self.assertEqual(status, 200)
        self.assertEqual([i['newIdentifier'] for i in preview['issues']], ['OPS-2', 'OPS-3', 'OPS-4'])
        status, result, _ = self.request('/issues/SUB-1/move', dict(version=root['version'], destinationProjectId=target['id'], token=preview['token']))
        self.assertEqual(status, 200)
        self.assertEqual(result['issue']['identifier'], 'OPS-2')
        self.assertEqual(self.get('/issues/SUB-2')['identifier'], 'OPS-3')
        self.assertEqual(len(result['moved']), 3)

    def test_chat_upload_round_trip_and_short_reference_resolution(self):
        import base64
        data = '\n'.join(f'Fixture line {n}' for n in range(1, 26)).encode()
        status, upload, _ = self.request('/chat/uploads', dict(uploadId=str(uuid.uuid4()), fileName='brief.md',
            contentType='text/markdown', base64=base64.b64encode(data).decode()))
        self.assertEqual(status, 201)
        self.assertEqual(len(upload['reference']), 26)
        self.assertEqual(upload['sizeBytes'], len(data))
        status, cards, _ = self.request('/resolve', dict(references=[upload['reference']]))
        self.assertEqual(status, 200)
        self.assertEqual(cards['items'][0]['upload'], upload)
        req = Request(self.server.origin + upload['url'], headers={'Authorization': 'Bearer fixture-access'})
        with urlopen(req) as response:
            self.assertEqual(response.read(), data)

    def test_chat_upload_failure_injection_is_opt_in_and_by_name(self):
        import base64
        body = lambda name: dict(uploadId=str(uuid.uuid4()), fileName=name, contentType='text/plain',
                                 base64=base64.b64encode(b'fixture').decode())
        status, _, _ = self.request('/chat/uploads', body('fail-one.txt'))
        self.assertEqual(status, 201, 'off by default')
        self.server.fail_chat_uploads = FIXTURE.failure_rule('fail-')
        status, problem, _ = self.request('/chat/uploads', body('fail-two.txt'))
        self.assertEqual(status, 413)
        self.assertNotIn('two', json.dumps(problem))
        status, _, _ = self.request('/chat/uploads', body('kept.txt'))
        self.assertEqual(status, 201)
        self.server.fail_chat_uploads = FIXTURE.failure_rule('fail-:503')
        status, _, _ = self.request('/chat/uploads', body('fail-three.txt'))
        self.assertEqual(status, 503)
        with self.assertRaises(Exception):
            FIXTURE.failure_rule('fail-:200')

    def test_planning_choices_assign_clear_and_reject_cross_project_links(self):
        milestone = self.get('/projects/SUB/milestones?includeContent=false')['items'][0]
        self.assertIsNone(milestone['description'])
        cycle = self.get('/issue-cycles?project=SUB')['items'][0]
        issue = self.get('/issues/SUB-2')
        status, changed, _ = self.request('/issues/SUB-2',
            dict(version=issue['version'], milestoneId=milestone['id'], cycleId=cycle['id']), method='PATCH')
        self.assertEqual(status, 200)
        self.assertEqual(changed['milestoneId'], milestone['id'])
        self.assertEqual(changed['cycle']['id'], cycle['id'])
        status, cleared, _ = self.request('/issues/SUB-2',
            dict(version=changed['version'], milestoneId=None, cycleId=None), method='PATCH')
        self.assertEqual(status, 200)
        self.assertIsNone(cleared['milestoneId'])
        self.assertIsNone(cleared['cycle'])
        self.assertEqual(self.request('/issues/OPS-1',
            dict(version=1, milestoneId=milestone['id']), method='PATCH')[0], 400)
        self.assertEqual(self.request('/issues/SUB-1',
            dict(version=1, cycleId=cycle['id']), method='PATCH')[0], 400)

    def test_pull_requests_summary_cursor_pages_and_active_links(self):
        summary = self.get('/issues/SUB-1')['pullRequestSummary']
        self.assertEqual(summary, dict(total=24, open=8, closed=8, merged=8, closingPending=8,
                                       lastObservedAt=summary['lastObservedAt']))
        self.assertIsNone(self.get('/issues/SUB-2')['pullRequestSummary'])
        first = self.get('/issues/SUB-1/pull-requests')
        self.assertEqual(len(first['items']), 20)
        second = self.get('/issues/SUB-1/pull-requests?' + urlencode({'cursor': first['nextCursor']}))
        self.assertEqual(len(second['items']), 4)
        self.assertIsNone(second['nextCursor'])
        self.assertEqual(len({pr['id'] for pr in first['items'] + second['items']}), 24)
        all_rows = self.get('/issues/SUB-1/pull-requests?includeInactive=true&limit=100')['items']
        self.assertEqual(len(all_rows), 25)
        expected = set('id repository number url title authorLogin state isDraft headBranch baseBranch referenceKind '
                       'referenceSources referenceActive completion mergedAt closedAt githubUpdatedAt lastObservedAt'.split())
        self.assertEqual(set(all_rows[0]), expected)
        self.assertEqual(set(all_rows[0]['repository']), {'owner', 'name', 'fullName', 'url'})
        self.assertEqual(sum(not pr['referenceActive'] for pr in all_rows), 1)

    def test_pull_requests_route_limits_scopes_and_undeployed_server(self):
        for limit in (0, 101):
            self.assertEqual(self.request(f'/issues/SUB-1/pull-requests?limit={limit}')[0], 400)
        first = self.get('/issues/SUB-1/pull-requests?limit=1')
        cursor = urlencode({'cursor': first['nextCursor']})
        self.assertEqual(self.request('/issues/SUB-2/pull-requests?' + cursor)[0], 400)
        self.assertEqual(self.request('/issues/SUB-1/pull-requests?includeInactive=true&' + cursor)[0], 400)
        self.assertEqual(self.get('/issues/SUB-2/pull-requests')['items'], [])
        self.server.fixture.pull_requests_available = False
        self.assertEqual(self.request('/issues/SUB-1/pull-requests')[0], 404)
        self.assertEqual(self.request('/issues/SUB-1')[0], 200)

    def media_request(self, path, data=None, method=None, mime='application/json'):
        request = Request(self.server.origin + '/api/v1' + path, data=data, method=method,
                          headers={'User-Agent': 'Subspace/fixture', 'Authorization': 'Bearer fixture-access', 'Content-Type': mime})
        try:
            response = urlopen(request, timeout=3)
        except HTTPError as error:
            response = error
        with response:
            return response.status, response.read(), response.headers

    def upload(self, route, name, mime, data):
        boundary = 'fixture-boundary'
        body = (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\nContent-Type: {mime}\r\n\r\n'.encode()
                + data + f'\r\n--{boundary}--\r\n'.encode())
        status, body, _ = self.media_request(route, body, 'POST', f'multipart/form-data; boundary={boundary}')
        return status, json.loads(body)

    def test_media_metadata_paging_inline_descriptors_and_private_image_bytes(self):
        images = self.get('/issues/SUB-1/images?limit=1')
        self.assertEqual(len(images['items']), 1)
        self.assertTrue(images['nextCursor'])
        next_page = self.get('/issues/SUB-1/images?' + urlencode({'limit': 1, 'cursor': images['nextCursor']}))
        self.assertNotEqual(images['items'][0]['id'], next_page['items'][0]['id'])
        attachments = self.get('/issues/SUB-1/attachments')['items']
        self.assertEqual(len(attachments), 5)
        plan = next(m for m in attachments if m['fileName'] == 'Fixture plan.md')
        status, data, headers = self.media_request('/issues/attachments/' + plan['mediaId'])
        self.assertEqual((status, data, headers['Content-Type']), (200, FIXTURE.FIXTURE_PLAN, 'text/markdown'))
        self.assertEqual(len(data), plan['sizeBytes'])
        self.assertEqual(headers['X-Content-Type-Options'], 'nosniff')
        self.assertTrue(all(set('mediaId kind fileName contentType sizeBytes url sha256'.split()) <= set(m) for m in attachments))
        self.assertNotIn('media', self.get('/issues/SUB-1'))
        comment = self.get('/issues/SUB-1/comments')['items'][0]
        self.assertEqual(len(comment['media']), 2)
        image = images['items'][0]
        status, data, headers = self.media_request('/issues/media/' + image['id'])
        self.assertEqual(status, 200)
        self.assertEqual(data, FIXTURE.fixture_png())
        self.assertEqual(headers['Content-Type'], 'image/png')
        self.assertEqual(headers['X-Content-Type-Options'], 'nosniff')

    def test_media_multipart_comments_and_distinct_cleanup_routes(self):
        status, image = self.upload('/issues/SUB-1/images', 'Fixture.png', 'image/png', FIXTURE.fixture_png())
        self.assertEqual(status, 201)
        status, file = self.upload('/issues/SUB-1/attachments', 'Fixture.md', 'text/markdown', b'# Fixture notes\n')
        self.assertEqual(status, 201)
        body = f'![Sketch](nett-media:{image["id"]})\n\n[Notes](nett-media:{file["mediaId"]})'
        status, posted, _ = self.request('/issues/SUB-1/comments', {'body': body})
        self.assertEqual(status, 201)
        self.assertEqual({m['mediaId'] for m in posted['media']}, {image['id'], file['mediaId']})
        status, removed, _ = self.media_request('/issues/media/' + image['id'] + '/delete', b'{}', 'POST')
        self.assertEqual((status, json.loads(removed)), (200, True))
        self.assertEqual(self.media_request('/issues/media/' + image['id'])[0], 404)
        status, removed, _ = self.media_request('/issues/attachments/' + file['mediaId'], b'{}', 'DELETE')
        self.assertEqual((status, json.loads(removed)), (200, True))
        status, removed, _ = self.media_request('/issues/attachments/' + file['mediaId'], b'{}', 'DELETE')
        self.assertEqual((status, json.loads(removed)), (200, False))

    def test_media_preflight_rejections_and_archive_boundary(self):
        self.assertEqual(self.upload('/issues/SUB-1/images', 'Fixture.svg', 'image/svg+xml', b'<svg/>')[0], 400)
        self.assertEqual(self.upload('/issues/SUB-1/images', 'Fixture.png', 'image/png', b'wrong bytes')[0], 400)
        self.assertEqual(self.upload('/issues/SUB-5/images', 'Fixture.png', 'image/png', FIXTURE.fixture_png())[0], 409)
        status, _, _ = self.media_request('/issues/SUB-1/images', b'bad', 'POST', 'application/json')
        self.assertEqual(status, 400)

    def test_loopback_and_required_authentication_for_every_route(self):
        self.assertEqual(self.server.server_address[0], '127.0.0.1')
        routes = ['/me', '/projects', '/projects/SUB/workflow-states', '/issues',
                  '/issues/groups?groupBy=stateType', '/issues/SUB-1', '/issues/SUB-1/relations',
                  '/issues/SUB-1/activity', '/issues/SUB-1/comments', '/issues/SUB-1/comments/fake/replies']
        for route in routes:
            for jwt in (None, 'wrong'):
                status, value, headers = self.request(route, jwt=jwt)
                self.assertEqual(status, 401)
                self.assertEqual(value['status'], 401)
                self.assertEqual(headers['Content-Type'], 'application/problem+json')
        self.assertEqual(self.request('/issues/SUB-1/comments', {'body': 'Refused'}, jwt=None)[0], 401)
        with self.assertRaises(ValueError):
            FIXTURE.FixtureServer(0, '')

    def test_identity_projects_and_workflows_match_wire_fields(self):
        me = self.get('/me')
        self.assertEqual(set(me), set('userId kind displayName email agentName agentRank fleet ship'.split()))
        self.assertEqual(me['displayName'], 'Ada Fixture')
        projects = self.get('/projects?includeArchived=true&limit=200')
        self.assertEqual(set(projects), {'items', 'nextCursor'})
        self.assertEqual([p['key'] for p in projects['items']], ['SUB', 'OPS'])
        for project in projects['items']:
            states = self.get('/projects/' + project['key'] + '/workflow-states?limit=200')['items']
            self.assertEqual([s['type'] for s in states], list(FIXTURE.STATE_TYPES))
            self.assertEqual(set(states[0]), {'id', 'projectId', 'name', 'type', 'position', 'isDefault'})
            self.assertTrue(all(s['projectId'] == project['id'] for s in states))
            self.assertEqual([s['id'] for s in states if s['isDefault']], [project['defaultStateId']])

    def test_tracker_children_relations_and_activity_wire_fields(self):
        rows = self.get('/issues')['items']
        self.assertTrue(all(set(r) == ISSUE_FIELDS for r in rows))
        self.assertTrue(all(r['lastActivityAt'] for r in rows))
        tracker = self.get('/issues/SUB-1')
        self.assertEqual(set(tracker), ISSUE_FIELDS | DETAIL_FIELDS)
        self.assertEqual(tracker['type'], 'tracker')
        self.assertEqual((tracker['progress']['done'], tracker['progress']['total']), (1, 2))
        self.assertEqual(len(tracker['children']['items']), 2)
        self.assertTrue(all(c['parentId'] == tracker['id'] for c in tracker['children']['items']))
        self.assertEqual(self.get('/issues/' + tracker['id'])['identifier'], 'SUB-1')
        self.assertEqual(self.get('/issues/SUB-2')['parent']['identifier'], 'SUB-1')
        relations = self.get('/issues/SUB-1/relations')['items']
        self.assertEqual(set(relations[0]), ISSUE_FIELDS | {'kind'})
        self.assertEqual(relations[0]['kind'], 'relatedTo')
        events = self.get('/issues/SUB-1/activity')
        self.assertEqual(set(events), {'items', 'nextCursor', 'references'})
        self.assertEqual(set(events['references']), {'people', 'projects', 'issues', 'labels', 'decisions'})
        self.assertEqual(set(events['items'][0]), set('actor changeId comment data dataVersion displayAt id issueId kind occurredAt'.split()))
        self.assertEqual(events['items'][0]['kind'], 'comment.created')

    def test_filters_and_orders(self):
        sub = self.get('/issues?project=SUB&parent=none')['items']
        self.assertEqual({r['identifier'] for r in sub}, {'SUB-1', 'SUB-4'})
        children = self.get('/issues?parent=SUB-1&order=manual')['items']
        self.assertEqual([r['identifier'] for r in children], ['SUB-2', 'SUB-3'])
        self.assertEqual(self.get('/issues?type=tracker&stateType=started')['items'][0]['identifier'], 'SUB-1')
        self.assertEqual(self.get('/issues?assignee=none')['items'][0]['identifier'], 'OPS-1')
        self.assertTrue(all(r['assignee']['displayName'] == 'Ada Fixture' for r in self.get('/issues?assignee=me')['items']))
        self.assertEqual(len(self.get('/issues?stateType=backlog&stateType=unstarted')['items']), 2)
        self.assertEqual(len(self.get('/issues?includeArchived=true')['items']), 6)
        self.assertEqual(self.get('/issues?q=SUB-4')['items'][0]['title'], 'Review the discussion')
        self.assertEqual(self.get('/issues?priority=0')['items'][0]['identifier'], 'OPS-1')
        state_id = self.get('/issues/SUB-2')['state']['id']
        self.assertEqual(len(self.get('/issues?stateId=' + state_id)['items']), 2)
        recent = self.get('/issues?order=activity')['items']
        since = urlencode({'activitySince': recent[0]['lastActivityAt']})
        self.assertEqual(len(self.get('/issues?' + since)['items']), 1)
        self.assertEqual(self.get('/issues?activitySince=2099-01-01T00:00:00Z')['items'], [])
        self.assertEqual(self.get('/issues?due=thisWeek')['items'][0]['identifier'], 'SUB-2')
        self.assertEqual(len(self.get('/issues?due=none')['items']), 3)
        priorities = [r['priority'] or 5 for r in self.get('/issues?order=priority')['items']]
        self.assertEqual(priorities, sorted(priorities))

    def test_decision_list_filters_and_pages(self):
        tracker = self.get('/issues/SUB-1')
        rows = self.get('/decisions?issue=SUB-1')['items']
        self.assertEqual({row['status'] for row in rows}, {'accepted', 'proposed', 'superseded'})
        self.assertEqual(len(rows), 3)
        # Newest decided first, as Subspace API sorts, on distinct dates that
        # differ from the ID order.
        dates = [row['decidedOn'] for row in rows]
        self.assertEqual(len(set(dates)), 3)
        self.assertEqual(dates, sorted(dates, reverse=True))
        self.assertNotEqual([row['id'] for row in rows], sorted(row['id'] for row in rows))
        self.assertEqual([row['status'] for row in rows], ['proposed', 'accepted', 'superseded'])
        self.assertEqual(self.get(f"/decisions?issue={tracker['id']}")['items'], rows)
        self.assertEqual(self.get('/decisions?issue=SUB-2'), {'items': [], 'nextCursor': None})
        self.assertEqual(self.get('/decisions?issue=SUB-4')['items'], [row for row in rows if row['status'] == 'accepted'])
        self.assertEqual(self.get('/decisions?project=OPS')['items'], [])
        self.assertEqual(self.get('/decisions?madeByType=agent&tag=review&q=handover')['items'],
                         [row for row in rows if row['status'] == 'proposed'])
        narrowed = self.get('/decisions?status=accepted&status=superseded')
        self.assertEqual({row['status'] for row in narrowed['items']}, {'accepted', 'superseded'})
        first = self.get('/decisions?issue=SUB-1&limit=1')
        cursor = urlencode({'cursor': first['nextCursor']})
        second = self.get('/decisions?issue=SUB-1&limit=2&' + cursor)
        self.assertEqual(first['items'], rows[:1])
        self.assertEqual(second['items'], rows[1:])
        self.assertEqual(first['items'] + second['items'], rows)
        self.assertIsNone(second['nextCursor'])
        self.assertEqual(self.request('/decisions?issue=SUB-4&' + cursor)[0], 400)
        for query in ('issue=invalid', 'limit=101', 'status=invalid', 'madeByType=invalid'):
            self.assertEqual(self.request('/decisions?' + query)[0], 400)

    def test_decision_detail_and_activity_resolve_the_same_records(self):
        rows = self.get('/decisions?issue=SUB-1')['items']
        current = next(row for row in rows if row['status'] == 'accepted')
        prior = self.get('/decisions/' + current['supersedesId'])
        detail = self.get('/decisions/' + current['id'])
        self.assertEqual({key: detail[key] for key in current}, current)
        self.assertNotIn('decision', current)
        self.assertEqual(detail['recordedBy']['userId'], detail['recordedByUserId'])
        self.assertEqual(detail['provenance']['madeByType'], detail['madeByType'])
        self.assertEqual(detail['supersedes'], {'id': prior['id'], 'title': prior['title'], 'status': 'superseded'})
        self.assertEqual(prior['supersededBy'], {'id': current['id'], 'title': current['title'], 'status': 'accepted'})
        self.assertEqual({link['identifier'] for link in detail['issues']['items']}, {'SUB-1', 'SUB-4'})
        self.assertEqual(self.get('/issues/SUB-1/activity?limit=1')['references']['decisions'], [])
        activity = self.get('/issues/SUB-1/activity')
        ids = {event['data']['decisionId'] for event in activity['items'] if event['kind'] == 'decision.logged'}
        self.assertEqual(ids, {str(uuid.UUID(row['id'])) for row in rows})
        self.assertCountEqual(activity['references']['decisions'],
                              [{key: row[key] for key in ('id', 'title', 'status')} for row in rows])
        self.assertEqual(self.get('/issues/SUB-2/activity')['references']['decisions'], [])

    def test_decision_scenarios_leave_other_issue_reads_available(self):
        current = self.get('/decisions?status=accepted')['items'][0]
        self.assertEqual(self.request('/decisions/not-a-uuid')[0], 400)
        self.assertEqual(self.request('/decisions/' + str(uuid.uuid4()))[0], 404)
        self.assertEqual(self.request('/decisions', jwt=None)[0], 401)
        self.server.decision_scenario = 'missing'
        self.assertEqual(self.request('/decisions/' + current['supersedesId'])[0], 404)
        self.assertEqual(self.get('/decisions/' + current['id'])['supersedes'], current['supersedes'])
        self.assertEqual(len(self.get('/decisions?issue=SUB-1')['items']), 3)
        for scenario, status in (('forbidden', 403), ('unavailable', 503)):
            self.server.decision_scenario = scenario
            self.assertEqual(self.request('/decisions?issue=SUB-1')[0], status)
            self.assertEqual(self.request('/decisions/' + current['id'])[0], status)
            self.assertEqual(self.get('/issues/SUB-1')['identifier'], 'SUB-1')

    def test_cursor_paging_and_group_continuation(self):
        first = self.get('/issues?project=SUB&limit=1')
        second = self.get('/issues?' + urlencode({'project': 'SUB', 'limit': 1, 'cursor': first['nextCursor']}))
        self.assertNotEqual(first['items'][0]['id'], second['items'][0]['id'])
        self.assertEqual(self.request('/projects?' + urlencode({'cursor': first['nextCursor']}))[0], 400)
        groups = self.get('/issues/groups?project=SUB&groupBy=state&limitPerGroup=1')
        started = next(g for g in groups['groups'] if g['stateType'] == 'started')
        self.assertEqual(set(started), {'stateId', 'stateType', 'count', 'items', 'nextCursor'})
        self.assertEqual(started['count'], 2)
        continuation = self.get('/issues?' + urlencode({'project': 'SUB', 'stateId': started['stateId'],
                                                       'limit': 1, 'cursor': started['nextCursor']}))
        self.assertNotEqual(continuation['items'][0]['id'], started['items'][0]['id'])
        completed = next(g for g in groups['groups'] if g['stateType'] == 'completed')
        self.assertEqual((completed['count'], completed['items']), (1, []))
        counts = self.get('/issues/groups?groupBy=stateType&limitPerGroup=0')['groups']
        self.assertTrue(all(not g['items'] and g['stateId'] is None for g in counts))
        projects = self.get('/projects?limit=1')
        self.assertEqual(len(self.get('/projects?' + urlencode({'cursor': projects['nextCursor']}))['items']), 1)

    def test_root_comments_replies_and_posting_are_memory_only(self):
        root = self.get('/issues/SUB-1/comments')['items'][0]
        self.assertEqual(set(root), COMMENT_FIELDS)
        self.assertEqual((root['replyCount'], root['hasReplies']), (1, True))
        reply = self.get(f"/issues/SUB-1/comments/{root['id']}/replies")['items'][0]
        self.assertEqual(reply['actualAuthor']['displayName'], 'Grace Fixture')
        self.assertEqual(reply['parentCommentId'], root['id'])
        before = self.get('/issues/SUB-1')
        status, posted, _ = self.request('/issues/SUB-1/comments', {'body': 'Fixture\r\npost', 'parentCommentId': None})
        self.assertEqual(status, 201)
        self.assertEqual(set(posted), COMMENT_FIELDS)
        self.assertEqual(posted['body'], 'Fixture\npost')
        self.assertEqual(posted['actualAuthor']['userId'], self.get('/me')['userId'])
        self.assertGreaterEqual(posted['createdAt'], before['lastActivityAt'])
        self.assertIn(posted['id'], [c['id'] for c in self.get('/issues/SUB-1/comments')['items']])
        status, response, _ = self.request('/issues/SUB-1/comments', {'body': 'A new reply', 'parentCommentId': root['id']})
        self.assertEqual(status, 201)
        self.assertEqual(response['parentCommentId'], root['id'])
        self.assertEqual(self.get('/issues/SUB-1')['commentCount'], before['commentCount'] + 2)
        self.assertEqual(self.get('/issues/SUB-1')['version'], before['version'])
        self.assertEqual(self.get('/issues/SUB-1/activity')['items'][0]['kind'], 'comment.created')
        isolated = FIXTURE.Fixture()
        self.assertEqual(len(isolated.comments), 2)

    def test_post_has_no_idempotency_key(self):
        first = self.request('/issues/SUB-1/comments', {'body': 'Same text'})[1]
        second = self.request('/issues/SUB-1/comments', {'body': 'Same text'})[1]
        self.assertNotEqual(first['id'], second['id'])
        self.assertEqual(first['body'], second['body'])

    def test_issue_picker_routes_and_sparse_writes(self):
        users = self.get('/users?limit=1')
        self.assertIsNotNone(users['nextCursor'])
        people = self.get('/users')['items']
        self.assertEqual({p['kind'] for p in people}, {'human', 'agent'})
        self.assertTrue(all(l['state'] == 'active' for l in self.get('/labels?state=active')['items']))
        self.assertEqual(self.get('/projects/SUB')['estimateScale'], 'fibonacci')
        payload = dict(projectId=FIXTURE.uid('SUB'), title='Create fixture', summary='Keep the fixture work clear so the team can decide what to do.',
                       description='@[Ada Fixture](member:' + FIXTURE.ADA['userId'] + ')',
                       assigneeId=FIXTURE.AGENT['userId'], estimate=5, priority=3,
                       dueDate='2026-12-01', labelIds=[FIXTURE.uid('label')], parentId='SUB-1')
        status, created, _ = self.request('/issues', payload)
        self.assertEqual(status, 201)
        self.assertEqual(created['assignee']['kind'], 'agent')
        self.assertEqual(created['parent']['identifier'], 'SUB-1')
        patched = self.request('/issues/' + created['id'], dict(version=1, title='Edited fixture',
                               estimate=None, dueDate=None, assigneeId=None, labelIds=[]), method='PATCH')
        self.assertEqual(patched[0], 200)
        self.assertEqual(patched[1]['description'], payload['description'])
        self.assertEqual(patched[1]['version'], 2)
        self.assertIsNone(patched[1]['estimate'])
        self.assertEqual(patched[1]['labels'], [])
        conflict = self.request('/issues/' + created['id'], dict(version=1, title='Stale'), method='PATCH')
        self.assertEqual(conflict[0], 409)
        self.assertEqual(conflict[1]['currentVersion'], 2)

    def test_summary_is_required_on_create_and_cannot_be_cleared_by_patch(self):
        payload = dict(projectId=FIXTURE.uid('SUB'), title='Summary contract')
        self.assertEqual(self.request('/issues', payload)[0], 400)
        for summary in (None, '', ' ', 'x' * 301, 'Two\nparagraphs'):
            self.assertEqual(self.request('/issues', dict(payload, summary=summary))[0], 400)
            result = self.request('/issues/SUB-2', dict(version=1, summary=summary), method='PATCH')
            self.assertEqual(result[0], 400)
            self.assertIn('summary', result[1]['detail'])
        self.assertIsNone(self.get('/issues/SUB-4')['summary'])
        status, created, _ = self.request('/issues', dict(payload, summary='Keep the work clear so the team can act.'))
        self.assertEqual(status, 201)
        self.assertEqual(created['summary'], 'Keep the work clear so the team can act.')

    def test_create_has_no_idempotency_and_checks_filter_creator_project_and_title(self):
        payload = dict(projectId=FIXTURE.uid('SUB'), title='Exact fixture', summary='Explain the work so matching issues are easy to identify.')
        first = self.request('/issues', payload)[1]
        second = self.request('/issues', payload)[1]
        self.assertNotEqual(first['id'], second['id'])
        found = self.get('/issues?project=SUB&creator=me&q=Exact+fixture&includeArchived=true')['items']
        self.assertEqual({r['id'] for r in found}, {first['id'], second['id']})
        self.assertEqual(self.get('/issues?creator=unknown')['items'], [])
        self.assertEqual(self.get('/issues?project=OPS&q=Exact+fixture')['items'], [])

    def test_trackers_refuse_manual_state_and_estimate(self):
        for payload in [dict(stateId=FIXTURE.uid('SUBstarted')), dict(estimate=3)]:
            self.assertEqual(self.request('/issues/SUB-1', dict(version=1, **payload), method='PATCH')[0], 400)
            self.assertEqual(self.request('/issues', dict(projectId=FIXTURE.uid('SUB'), title='Tracker', summary='Show overall progress so the team can find unfinished work.', type='tracker', **payload))[0], 400)
        status, tracker, _ = self.request('/issues', dict(projectId=FIXTURE.uid('SUB'), title='Tracker', summary='Show overall progress so the team can find unfinished work.', type='tracker'))
        self.assertEqual(status, 201)
        self.assertEqual(tracker['state']['type'], 'backlog')
        self.assertIsNone(tracker['estimate'])
        self.assertEqual(self.request('/issues/SUB-1', dict(version=1, type='issue'), method='PATCH')[0], 409)

    def test_invalid_queries_unknown_routes_and_post_validation(self):
        for path in ['/issues?limit=0', '/issues?limit=201', '/issues?cursor=bad', '/issues?order=manual',
                     '/issues?activitySince=2026-11-16', '/issues?due=invalid', '/issues?type=design', '/issues/groups?groupBy=state',
                     '/issues/groups?groupBy=stateType&limit=1', '/issues/groups?groupBy=stateType&limitPerGroup=26']:
            self.assertEqual(self.request(path)[0], 400, path)
        for path in ['/auth/orgs', '/issues/missing', '/projects/BAD/workflow-states']:
            self.assertEqual(self.request(path)[0], 404, path)
        for payload in [{'body': ''}, {'body': ' '}, {'body': 'x' * 50001}, {'body': 3},
                        {'body': 'No draft IDs', 'mediaDraftId': 'fake'}, {'body': 'Reply', 'parentCommentId': 'missing'}]:
            self.assertEqual(self.request('/issues/SUB-1/comments', payload)[0], 400)
        root = self.get('/issues/SUB-1/comments')['items'][0]
        reply = self.get(f"/issues/SUB-1/comments/{root['id']}/replies")['items'][0]
        for path, parent in [('/issues/SUB-1/comments', reply['id']), ('/issues/OPS-1/comments', root['id'])]:
            self.assertEqual(self.request(path, {'body': 'Invalid reply', 'parentCommentId': parent})[0], 400)
        self.assertEqual(self.request('/issues/SUB-5/comments', {'body': 'Archived'})[0], 409)
        self.assertEqual(self.request('/issues', {'title': 'Missing project'})[0], 400)


def schema_shape(schema, schemas):
    if '$ref' in schema:
        return schema_shape(schemas[schema['$ref'].split('/')[-1]], schemas)
    if 'allOf' in schema:
        properties = {}
        for part in schema['allOf']:
            properties.update(schema_shape(part, schemas).get('properties', {}))
        return dict(type='object', properties=properties)
    if 'oneOf' in schema:
        return dict(schema_shape(schema['oneOf'][0], schemas), nullable=schema.get('nullable', False))
    return schema


def check_shape(value, schema, schemas):
    shape = schema_shape(schema, schemas)
    if not shape or (value is None and shape.get('nullable')):
        return
    kind = shape.get('type')
    if kind == 'object':
        assert isinstance(value, dict), value
        properties = shape.get('properties', {})
        assert set(value) == set(properties), (set(value) - set(properties), set(properties) - set(value))
        for key, item in value.items():
            check_shape(item, properties[key], schemas)
    elif kind == 'array':
        assert isinstance(value, list), value
        for item in value:
            check_shape(item, shape['items'], schemas)
    elif kind in ('string', 'integer', 'boolean', 'number'):
        check_scalar(value, shape)
    if 'enum' in shape:
        assert value in shape['enum'], value


def check_scalar(value, shape):
    types = {'string': (str,), 'integer': (int,), 'boolean': (bool,), 'number': (int, float)}
    assert isinstance(value, types[shape['type']]), value
    if shape['type'] in ('integer', 'number'):
        assert not isinstance(value, bool), value
    if shape.get('format') == 'guid':
        assert str(uuid.UUID(value)) == value, value
    if shape.get('format') == 'date-time':
        assert datetime.fromisoformat(value.replace('Z', '+00:00')).tzinfo is not None, value
    if shape.get('format') == 'date':
        assert date.fromisoformat(value).isoformat() == value, value


def openapi_response_shapes(self):
    contract = json.loads(Path(self.openapi_path).read_text())
    cases = [('/me', '/me'), ('/projects?limit=200', '/projects'),
             ('/users', '/users'), ('/labels?state=active', '/labels'), ('/projects/SUB', '/projects/{projectRef}'),
             ('/projects/SUB/workflow-states', '/projects/{projectRef}/workflow-states'),
             ('/issues', '/issues'), ('/issues/groups?groupBy=stateType', '/issues/groups'),
             ('/issues/SUB-1', '/issues/{issueRef}'), ('/issues/SUB-2', '/issues/{issueRef}'),
             ('/issues/SUB-1/relations', '/issues/{issueRef}/relations'),
             ('/issues/SUB-1/activity', '/issues/{issueRef}/activity'),
             ('/issues/SUB-1/comments', '/issues/{issueRef}/comments')]
    root = self.get('/issues/SUB-1/comments')['items'][0]['id']
    cases.append((f'/issues/SUB-1/comments/{root}/replies', '/issues/{issueRef}/comments/{rootId}/replies'))
    for actual, template in cases:
        response = contract['paths']['/api/v1' + template]['get']['responses']['200']
        check_shape(self.get(actual), response['content']['application/json']['schema'], contract['components']['schemas'])
    response = contract['paths']['/api/v1/issues/{issueRef}/comments']['post']['responses']['201']
    status, posted, _ = self.request('/issues/SUB-1/comments', {'body': 'OpenAPI check'})
    self.assertEqual(status, 201)
    check_shape(posted, response['content']['application/json']['schema'], contract['components']['schemas'])

    for method, path, template, payload, code in [
        ('post', '/issues', '/issues', dict(projectId=FIXTURE.uid('SUB'), title='OpenAPI create', summary='Check the create contract so integrations send valid issues.'), '201'),
        ('patch', '/issues/SUB-2', '/issues/{issueRef}', dict(version=1, description='OpenAPI edit'), '200'),
    ]:
        status, value, _ = self.request(path, payload, method=method.upper())
        self.assertEqual(status, int(code))
        schema = contract['paths']['/api/v1' + template][method]['responses'][code]['content']['application/json']['schema']
        check_shape(value, schema, contract['components']['schemas'])


if __name__ == '__main__':
    if '--openapi' in sys.argv:
        index = sys.argv.index('--openapi')
        FixtureTests.openapi_path = sys.argv[index + 1]
        del sys.argv[index:index + 2]
        FixtureTests.test_openapi_response_shapes = openapi_response_shapes
    unittest.main()
