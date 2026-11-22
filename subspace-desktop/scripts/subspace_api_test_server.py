#!/usr/bin/env python3
"""Loopback-only, memory-only Subspace API fixture. Never calls upstream services."""

import argparse
import base64
import copy
import hashlib
import hmac
import json
import queue
import re
import threading
import uuid
import struct
import zlib
from email.parser import BytesParser
from email.policy import default as email_policy
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlsplit

STATE_TYPES = ('triage', 'backlog', 'unstarted', 'started', 'completed', 'canceled', 'duplicate')
TERMINAL = ('completed', 'canceled', 'duplicate')
STATE_NAMES = ('Triage', 'Backlog', 'Todo', 'In progress', 'Done', 'Cancelled', 'Duplicate')
DECISIONS_PATH = '/api/v1/decisions'
DECISION_SCENARIOS = ('normal', 'missing', 'forbidden', 'unavailable')


def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, 'https://fixture.example.test/' + name))


def instant():
    return datetime.now(timezone.utc).isoformat(timespec='microseconds').replace('+00:00', 'Z')


def person(name):
    return dict(userId=uid(name), displayName=name, isFormerMember=False, kind='human')


ADA = person('Ada Fixture')
GRACE = person('Grace Fixture')
AGENT = dict(person('Orbit Fixture'), kind='agent')


def fixture_png():
    def chunk(kind, data):
        return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', 240, 120, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(fixture_pixels(240, 120))) + chunk(b'IEND', b''))


def fixture_pixels(width, height):
    """A diagonal blue-to-amber gradient with a light band, so a preview shows real cropping."""
    rows = []
    for y in range(height):
        row = bytearray(b'\x00')
        for x in range(width):
            t = (x + y) / (width + height)
            band = 60 if abs(x - 2 * y) < 12 else 0
            row += bytes((min(255, int(40 + 200 * t) + band), min(255, int(90 + 80 * t) + band),
                          min(255, int(200 - 150 * t) + band)))
        rows.append(bytes(row))
    return b''.join(rows)


# A small Markdown attachment for the issue page's attachment viewer.
FIXTURE_PLAN = b"""# Fixture plan

Notes for the fixture review.

## Steps

- Read the issue and its attachments.
- Post a text comment.
- Close the viewer with Escape.

## Links

See the [fixture guide](https://example.invalid/fixture-guide).
"""


class MediaBytes:
    def __init__(self, data, mime, name, image):
        self.data, self.mime, self.name, self.image = data, mime, name, image


def descriptor(record):
    return dict(mediaId=record['id'], kind=record['kind'], fileName=record['fileName'],
                contentType=record['contentType'], sizeBytes=len(record['bytes']), url=record['url'],
                sha256=record['sha256'], version=1, publicFileId='', expiresAt=None)


def image_metadata(record):
    return {key: record[key] for key in ('id', 'issueId', 'fileName', 'contentType', 'sizeBytes',
                                        'url', 'sha256', 'createdAt', 'uploadedBy')}


def actor(author):
    return dict(type=author['kind'], userId=author['userId'], person=author, label=None)


def states(key):
    return [dict(id=uid(key + kind), projectId=uid(key), name=name, type=kind,
                 position=position, isDefault=kind == 'backlog')
            for position, (kind, name) in enumerate(zip(STATE_TYPES, STATE_NAMES))]


def pull_request(index, now):
    state = ('open', 'merged', 'closed')[index % 3]
    return dict(id=uid(f'pull-request-{index}'), repository=dict(owner='fixture', name='workspace',
                fullName='fixture/workspace', url='https://github.com/fixture/workspace'),
                number=index + 1, url=f'https://github.com/fixture/workspace/pull/{index + 1}',
                title=f'Fixture change {index + 1}', authorLogin='octo-dev', state=state, isDraft=index == 0,
                headBranch=f'fixture/SUB-1-{index}', baseBranch='main', referenceKind='closes',
                referenceSources=['title', 'branch'], referenceActive=index != 24,
                completion='completed' if state == 'merged' else 'none', mergedAt=now if state == 'merged' else None,
                closedAt=now if state != 'open' else None, githubUpdatedAt=now, lastObservedAt=now)


def pull_request_summary(items):
    active = [pr for pr in items if pr['referenceActive']]
    if not active:
        return None
    counts = {state: sum(pr['state'] == state for pr in active) for state in ('open', 'closed', 'merged')}
    return dict(total=len(active), **counts,
                closingPending=sum(pr['state'] == 'open' and pr['referenceKind'] == 'closes' for pr in active),
                lastObservedAt=max(pr['lastObservedAt'] for pr in active))


def project(key, name, now):
    return dict(id=uid(key), key=key, name=name, archived=False, archivedAt=None,
                archivedByUserId=None, createdAt=now, updatedAt=now, createdByUserId=ADA['userId'],
                defaultStateId=uid(key + 'backlog'), description='Fictional fixture project.',
                estimateScale='fibonacci', cycles=None, health=None, initiativeId=None,
                latestUpdateAt=None, latestUpdateId=None, leadAvailable=True, leadId=ADA['userId'],
                leadName=ADA['displayName'], openIssueCounts=None, priority=0,
                startDate=None, status='started', targetDate=None, version=1)


def issue(ref, title, kind, now):
    key = ref.split('-')[0]
    state = dict(id=uid(key + kind), name=STATE_NAMES[STATE_TYPES.index(kind)], type=kind)
    return dict(id=uid(ref), identifier=ref, projectId=uid(key), projectKey=key, projectRank=int(ref.split('-')[1]),
                subIssueRank=None, title=title, summary='Explain the work clearly so people can decide what to do without reading the whole discussion.', type='issue', state=state, assignee=ADA, owner=ADA,
                labels=[dict(id=uid('label'), name='Fixture', color='#6C8EEF', state='active')],
                priority=2, estimate=3, dueDate=None, parentId=None, blocked=False, progress=None,
                archivedAt=None, createdAt=now, updatedAt=now, version=1, commentCount=0,
                todoCounts=dict(total=0, done=0), lastActivityAt=now, lastActivity=None,
                cycle=None, cycleId=None, milestoneId=None, relayedCrew=None, provenance=None, pullRequestSummary=None)


def comment(issue_id, body, parent, now, author=ADA):
    return dict(id=str(uuid.uuid4()), issueId=issue_id, parentCommentId=parent, body=body,
                version=1, actualAuthor=author, createdAt=now, displayCreatedAt=now,
                editedAt=None, deletedAt=None, resolved=False, resolvedBy=None, resolvedAt=None,
                replyCount=0, hasReplies=False, media=[], mentions=[], relayedCrew=None, provenance=None)


def activity(row, now, kind='issue.created', posted=None):
    return dict(id=str(uuid.uuid4()), issueId=row['id'], changeId=str(uuid.uuid4()),
                kind=kind, occurredAt=now, displayAt=now, actor=actor(ADA), dataVersion=1,
                data={} if posted is None else dict(commentId=posted['id'], parentCommentId=posted['parentCommentId']),
                comment=posted)


def decision_summary(row):
    return {key: row[key] for key in ('id', 'title', 'status')}


def decision_record(key, title, status, at, decided, project_row, linked_issues, text, maker=ADA):
    links = [{field: issue_row[field] for field in ('id', 'identifier', 'title')}
             for issue_row in linked_issues]
    agent = maker['kind'] == 'agent'
    provenance = dict(madeByType=maker['kind'], madeByUserId=None if agent else maker['userId'],
                      madeByAgentName=maker['displayName'] if agent else None,
                      madeByAgentRank=maker.get('agent', {}).get('rank', 'crew') if agent else None,
                      madeByAgentModel='fixture-model' if agent else None,
                      evidenceKind='documentOrUrl', evidenceReference='https://example.invalid/fixture-review',
                      evidenceExcerpt='A fictional review note for Desktop capture.')
    row = dict(id=uid('decision-' + key), title=title, status=status, decidedOn=decided,
               projectId=project_row['id'], projectKey=project_row['key'],
               project=dict(id=project_row['id'], name=project_row['name'], archived=False),
               supersedesId=None, supersededById=None, supersedes=None, supersededBy=None,
               issues=dict(items=links, count=len(links)), tags=['review', 'workflow'],
               recordedByUserId=ADA['userId'], version=1, revisionNumber=1, createdAt=at, updatedAt=at,
               **provenance)
    accepted = status in ('accepted', 'superseded')
    detail = dict(decision=text, context='The fixture team is reviewing its workspace handover.',
                  why='Reviewers can find the current choice and its history in one place.',
                  implications='Use this approach for the next fixture review.',
                  alternatives='Keep the existing approach and review it again later.',
                  acceptedAt=at if accepted else None, acceptedByUserId=ADA['userId'] if accepted else None,
                  rejectedAt=None, rejectedByUserId=None, provenance=provenance,
                  recordedBy=dict(userId=ADA['userId'], displayName=ADA['displayName'],
                                  available=True, kind='human'))
    return row, detail


def seed_decisions(project_row, tracker, discussion, now):
    instant_now = parse_instant(now)

    def minutes_ago(minutes):
        return (instant_now - timedelta(minutes=minutes)).isoformat().replace('+00:00', 'Z')

    def days_ago(days):
        return (instant_now - timedelta(days=days)).date().isoformat()

    # Distinct decided-on dates, so the list order (newest decided first)
    # differs from both the logging order and the ID order.
    old = decision_record(
        'separate-queues', 'Keep separate review queues', 'superseded', minutes_ago(40), days_ago(9),
        project_row, [tracker], 'Review each part of the fixture workspace in its own queue.')
    accepted = decision_record(
        'shared-queue', 'Use one review queue', 'accepted', minutes_ago(25), days_ago(4),
        project_row, [tracker, discussion], 'Use one queue for all reviews of the fixture workspace.')
    proposed = decision_record(
        'handover-note', 'Add a short handover note', 'proposed', minutes_ago(10), days_ago(1),
        project_row, [tracker], 'Write a short handover note when a fixture review closes.', maker=AGENT)
    old[0].update(supersededById=accepted[0]['id'], supersededBy=decision_summary(accepted[0]),
                  updatedAt=accepted[0]['createdAt'], revisionNumber=2, version=2)
    accepted[0].update(supersedesId=old[0]['id'], supersedes=decision_summary(old[0]))
    return {row['id']: (row, detail) for row, detail in (old, accepted, proposed)}


def decision_references(records, events):
    ids = dict.fromkeys(event['data']['decisionId'] for event in events if event['kind'] == 'decision.logged')
    return [decision_summary(records[decision_id][0]) for decision_id in ids if decision_id in records]


def seed_issues(now):
    rows = [issue('SUB-1', 'Deliver the fixture workspace', 'started', now),
            issue('SUB-2', 'Read the issue detail', 'started', now),
            issue('SUB-3', 'Connect the fixture', 'completed', now),
            issue('SUB-4', 'Review the discussion', 'backlog', now),
            issue('OPS-1', 'Check local operations', 'unstarted', now),
            issue('SUB-5', 'Archived fixture task', 'completed', now)]
    summaries = [
        'Keep fixture delivery visible across its tasks so the team can find unfinished work.',
        'Show the full issue context so people can understand and update the work from one page.',
        'Connect the local fixture service so development does not need a live account.',
        None,
        'Expose local operational checks so failures can be found before delivery.',
        'Keep completed fixture work available so its history is not lost.',
    ]
    for row, summary in zip(rows, summaries):
        row['summary'] = summary
    rows[0].update(type='tracker', estimate=None,
                   progress=dict(done=1, total=2, canceled=0, duplicate=0, hasMore=False,
                                 sample=[dict(id=r['id'], stateType=r['state']['type']) for r in rows[1:3]]))
    for index, row in enumerate(rows[1:3]):
        row.update(parentId=rows[0]['id'], subIssueRank=index + 1)
    rows[0].update(pullRequestSummary=dict(total=5, open=1, closed=0, merged=4,
                                         closingPending=0, checksState='success', lastObservedAt=now),
                   dueDate=(datetime.now(timezone.utc) + timedelta(days=4)).date().isoformat())
    tile_agent = dict(AGENT, displayName='protostar',
                      agent=dict(rank='captain', fleet='fixture', ship='fixture'))
    rows[1].update(assignee=tile_agent, owner=GRACE,
                   dueDate=datetime.now(timezone.utc).date().isoformat(),
                   lastActivity=dict(kind='comment.created',
                                     occurredAt=(datetime.now(timezone.utc) - timedelta(minutes=4)).isoformat(),
                                     actor=actor(tile_agent)))
    # A long generated label with no spaces, as end-to-end runs create, to
    # show truncation: one of several on OPS-1, the only one on SUB-2.
    long_label = dict(id=uid('label-e2e'),
                      name='e2e-f0a573fcdfe94c24a3ec9b79afb9f91d-planning-views-regression-suite',
                      color='#4C7899', state='active')
    rows[1]['labels'] = [long_label]
    rows[4].update(assignee=None, priority=0, blocked=True, labels=rows[4]['labels'] + [long_label])
    rows[5]['archivedAt'] = now
    rows[3]['summary'] = None
    return rows


# --fleet: the demo world's projects (subspace-api/scripts/demo/data.ts),
# populated as a busy team's tracker for screenshots and web Space evidence,
# beside the neutral FLT project. The Fleet API fixture's aurora fleet works on
# these issues under the same numbers, titles and states. Every person,
# captain and repository is fictional.
FLEET_PEOPLE = dict(mira='Mira Fenwick', tavi='Tavi Quill', sela='Sela Bracken')
FLEET_CAPTAINS = {'lyra': ('aurora-lyra', 'aurora'), 'orion': ('aurora-orion', 'aurora'),
                  'castor': ('aurora-castor', 'aurora'), 'vega': ('aurora-vega', 'aurora'),
                  'rigel': ('borealis-rigel', 'borealis')}
# --fleet: the aurora fleet's initiative, so Channels knows its roles: on the
# fleet's connection (any loopback one in a capture) the API's agents give
# each rank by IRC nick: aurora-admiral is the rear admiral (named
# admiral-vance, so no name rule could guess it), aurora-lyra, aurora-orion,
# aurora-castor and aurora-vega are captains, and aurora-wren is Lyra's crew.
# The fleet records #aurora-starbase and #aurora-command; each captain's ship
# channel is the standard #<nick>. Never selected, so it joins nothing.
FLEET_AGENTS = [dict(userId='fixture-agent-admiral', name='admiral-vance', rank='rearAdmiral', shipId=None,
                     ship=None, ircNick='aurora-admiral', shipChannel=None, available=True)] + [
    dict(userId=f'fixture-agent-{code}', name=code.title(), rank='captain', shipId=f'fixture-ship-{code}',
         ship=ship, ircNick=nick, shipChannel=None, available=True)
    for code, ship, nick in (('castor', 'Comet', 'aurora-castor'), ('lyra', 'Lattice', 'aurora-lyra'),
                             ('orion', 'Plover', 'aurora-orion'), ('vega', 'Vane', 'aurora-vega'))] + [
    dict(userId='fixture-agent-wren', name='Wren', rank='crew', shipId='fixture-ship-lyra', ship='Lattice',
         ircNick='aurora-wren', shipChannel=None, available=True)]
FLEET_INITIATIVE = dict(
    id='5d1f0c2e-5b7a-4f0e-9a51-2a8f3c6d7e10', name='Fallow Reach', channels='#aurora-starbase, #aurora-lyra',
    fleetId='aurora', fleet=dict(id='aurora', slug='aurora', name='Aurora', ircHost='irc.example.test', ircPort=6697,
                                 ircTls=True, ircWebsocketUrl=None, rearAdmiralName='admiral-vance',
                                 captainNames=['Castor', 'Lyra', 'Orion', 'Vega'], vmHost=None,
                                 starbaseChannel='#aurora-starbase', commandChannel='#aurora-command',
                                 agents=FLEET_AGENTS,
                                 ships=[dict(id=agent['shipId'], name=agent['ship'], channel=None)
                                        for agent in FLEET_AGENTS if agent['rank'] == 'captain']))
FLEET_LABELS = dict(bug='#e5484d', feature='#5b5bd6', performance='#f76b15', docs='#30a46c',
                    design='#ab4aba', reliability='#008c95')
# Per project: key, name, lead, then issues as (title, state type, label,
# priority, parent number, assignee, extras). Extras: e estimate, d due in
# days, c comments, p (open, merged) pull requests.
FLEET_PROJECTS = (
    ('NAV', 'Navigation console', 'mira', (
        ('Render star chart tiles off the UI thread', 'started', 'performance', 2, None, 'castor', dict(d=3, c=6, p=(1, 2))),
        ('Course correction preview ignores relay drift', 'unstarted', 'bug', 1, None, 'mira', dict(e=5, d=6, c=3)),
        ('Add waypoint search to the console', 'backlog', 'feature', 3, None, None, dict(e=8)),
        ('Document the telemetry frame format', 'completed', 'docs', 4, None, 'sela', dict(e=2, c=2, p=(0, 1))),
        ('Retry telemetry uploads with backoff', 'triage', 'bug', 2, None, None, dict(c=1, p=(1, 0))),
        ('Capture a chart-pan baseline on the pilot laptop', 'completed', 'performance', 3, 1, 'tavi', dict(e=2, c=4)),
        ('Cancel tile decodes that leave the viewport', 'started', 'performance', 2, 1, 'lyra', dict(e=3, d=2, c=5, p=(1, 0))),
        ('Label unconfirmed burns in the route preview', 'unstarted', 'design', 2, 2, 'sela', dict(e=3, d=8, p=(1, 0))),
        ('Retire the legacy route wizard', 'canceled', 'feature', 4, None, None, dict(c=2)),
        ('Route editor jumps when panning across a seam', 'duplicate', 'bug', 3, None, None, dict(c=1)),
        ('Tab from a chart marker to the burn summary', 'started', 'design', 2, None, 'lyra', dict(e=3, d=5, c=2, p=(1, 0))),
        ('Make the training route available offline', 'backlog', 'reliability', 4, None, 'tavi', dict(e=5)),
        ('Pan inertia overshoots on trackpads', 'triage', 'bug', 3, None, None, dict(c=2)),
        ('Show sector names on the minimap', 'unstarted', 'feature', 3, None, 'lyra', dict(e=2, d=10)),
        ('Remember the last chart zoom per route', 'unstarted', 'feature', 4, None, None, dict(e=1)),
        ('Telemetry parser rejects empty checksums', 'completed', 'bug', 2, None, 'lyra', dict(e=1, c=3, p=(0, 1))),
        ('Export the route as a printable briefing', 'canceled', 'feature', 4, None, None, dict()),
        ('Chart stalls when crossing sector seams', 'duplicate', 'performance', 3, None, None, dict(c=1)),
        ('Recentre the chart on the ship with one key', 'started', 'design', 3, None, 'sela', dict(e=2, d=1, c=1)),
        ('Burn summary announces the fuel reserve twice', 'triage', 'bug', 2, None, None, dict(c=1)),
    )),
    ('MAP', 'Sector atlas', 'sela', (
        ('Show the survey corridor at every zoom level', 'started', 'design', 2, None, 'orion', dict(e=5, d=4, c=4, p=(1, 0))),
        ('Pick atlas label density by zoom band', 'unstarted', 'design', 2, 1, 'orion', dict(e=3, d=7)),
        ('Record the high-contrast map palette', 'completed', 'docs', 3, 1, 'sela', dict(e=1, c=2)),
        ('Detect outdated sector packs at startup', 'unstarted', 'reliability', 2, None, None, dict(e=3)),
        ('Cache the last viewed sector', 'backlog', 'feature', 3, None, None, dict(e=2)),
        ('Investigate clipped labels on narrow windows', 'triage', 'bug', 2, None, None, dict(c=3)),
        ('Keyboard shortcuts for layer toggles', 'backlog', 'feature', 4, None, None, dict(e=3)),
        ('Move the scale bar above the annotation tray', 'completed', 'design', 3, None, 'sela', dict(e=1, p=(0, 1))),
        ('Reject route manifests with mismatched atlas versions', 'started', 'reliability', 2, None, 'orion', dict(e=3, d=2, c=2, p=(1, 0))),
        ('Drop the separate night palette preview', 'canceled', 'design', 4, None, None, dict()),
        ('Compare tile cache hit rates after warm-up', 'completed', 'performance', 3, None, 'tavi', dict(e=2, c=1)),
        ('Surface route provenance in the atlas', 'backlog', 'feature', 4, None, None, dict(e=5)),
        ('Labels collide at the eastern crossing', 'duplicate', 'bug', 3, None, None, dict(c=1)),
    )),
    ('WARP', 'Warp diagnostics', 'tavi', (
        ('Alert when plasma temperature trends upwards', 'started', 'feature', 2, None, 'rigel', dict(e=5, d=3, c=7, p=(1, 1))),
        ('Diagnostics report times out on long runs', 'unstarted', 'bug', 1, None, 'tavi', dict(e=3, d=-1, c=2)),
        ('Export simulator runs as CSV', 'backlog', 'feature', 3, None, None, dict(e=3)),
        ('Suppress alert chatter during calibration', 'completed', 'reliability', 2, None, 'rigel', dict(e=3, c=3, p=(0, 1))),
        ('Record the trace used in alert acceptance tests', 'completed', 'docs', 3, 1, 'tavi', dict(e=1)),
        ('Show the time window used by each alert', 'started', 'design', 2, 1, 'sela', dict(e=2, d=5, c=1)),
        ('Page diagnostics by sample range', 'started', 'performance', 2, 2, 'rigel', dict(e=5, d=6, c=2, p=(1, 0))),
        ('Distinguish sensor silence from stable temperature', 'triage', 'bug', 1, None, None, dict(c=2)),
        ('Remove the manual alarm reset command', 'canceled', 'feature', 4, None, None, dict()),
        ('Add an empty-state message for new test runs', 'completed', 'design', 4, None, 'sela', dict(e=1)),
        ('Describe the simulator replay controls', 'backlog', 'docs', 4, None, None, dict(e=2)),
        ('Verify alarms after a mid-run reconnect', 'unstarted', 'reliability', 2, None, 'rigel', dict(e=3, d=9)),
        ('Alert fires twice on the 09:42 replay', 'duplicate', 'bug', 3, None, None, dict(c=1)),
    )),
    ('RELAY', 'Relay handover', 'mira', (
        ('Recover queued frames after a relay swap', 'started', 'reliability', 1, None, 'orion', dict(e=8, d=2, c=8, p=(1, 1))),
        ('Add sequence counters to the handover dashboard', 'completed', 'feature', 2, 1, 'orion', dict(e=2, c=2, p=(0, 1))),
        ('Replay a duplicate acknowledgement in the simulator', 'started', 'bug', 2, 1, 'orion', dict(e=3, d=1, c=4, p=(1, 0))),
        ('Cap the retry queue on slow relays', 'unstarted', 'performance', 2, None, None, dict(e=3, d=12)),
        ('Publish the pilot handover runbook', 'completed', 'docs', 3, None, 'vega', dict(e=2, c=1)),
        ('Show when the backup relay last responded', 'started', 'design', 3, None, 'castor', dict(e=2, d=4, c=1)),
        ('Retry a stale DNS lookup after stand-down', 'triage', 'bug', 2, None, None, dict(c=1)),
        ('Prototype peer-to-peer relay discovery', 'canceled', 'feature', 4, None, None, dict(c=3)),
        ('Limit simultaneous handover candidates', 'backlog', 'reliability', 3, None, None, dict(e=3)),
        ('Write an operator checklist for clean rollback', 'unstarted', 'docs', 2, None, 'mira', dict(e=2, d=5)),
        ('Track reconnect time at the 95th percentile', 'backlog', 'performance', 3, None, None, dict(e=3)),
        ('Replay edge-case sequence gaps during rollout', 'completed', 'reliability', 2, None, 'orion', dict(e=3, c=2)),
    )),
)
FLEET_ISSUES = tuple((f'{key}-{number}', title, kind, assignee)
                     for key, _, _, rows in FLEET_PROJECTS
                     for number, (title, kind, _, _, _, assignee, _) in enumerate(rows, 1)) + (
    ('FLT-198', 'Serve the Fleet API over a private network', 'completed', None),
    ('FLT-205', 'Stream fleet events to web Spaces', 'started', None),
    ('FLT-207', 'Resume event streams after a drop', 'unstarted', None),
    ('FLT-212', 'Warp transitions between fleet tiers', 'started', None),
    ('FLT-230', 'Offline mode for the fleet map', 'backlog', None),
)
FLT_ASSIGNEES = {'FLT-198': 'Rosa Fixture', 'FLT-205': 'Jo Fixture', 'FLT-212': 'Bo Sample'}
# Decisions: project, key, title, status, days ago, issue numbers, maker
# (person or captain), tags, why; then supersessions as (old key, new key).
FLEET_DECISIONS = (
    ('NAV', 'two-workers', 'Keep two tile workers on the pilot console', 'superseded', 16, (1, 6), 'mira',
     ('performance',), 'Flight operations runs the pilot on a shared laptop.'),
    ('NAV', 'viewport-pool', 'Size the tile pool to the current viewport', 'accepted', 9, (1, 7), 'lyra',
     ('performance',), 'A bounded adaptive pool reduced panning stalls without holding obsolete frames.'),
    ('NAV', 'drift-overlay', 'Show drift as a separate route overlay', 'proposed', 4, (2, 8), 'sela',
     ('design', 'navigation'), 'Operators need to compare the planned and corrected path in one view.'),
    ('NAV', 'wizard', 'Keep the legacy route wizard beside the editor', 'rejected', 12, (9,), 'tavi',
     ('workflow',), 'The pilot route editor loads every supported waypoint format.'),
    ('WARP', 'any-crossing', 'Alert on any crossing of the warning band', 'superseded', 20, (1,), 'tavi',
     ('alerts',), 'The simplest rule to explain during the first rehearsal.'),
    ('WARP', 'sustained-rise', 'Require a sustained rise before alerting', 'accepted', 11, (1, 5, 6), 'rigel',
     ('alerts', 'reliability'), 'A single crossing caused excess alerts in the calibration replay.'),
    ('WARP', 'sample-pages', 'Page long diagnostics reports by sample range', 'proposed', 2, (2, 7), 'rigel',
     ('performance',), 'Reports over six hours of samples exceed the request timeout.'),
    ('MAP', 'route-layer', 'Pin the route layer across zoom bands', 'accepted', 7, (1, 2), 'sela',
     ('design',), 'The survey crew uses the corridor to orient themselves before reading place names.'),
    ('MAP', 'night-preview', 'Offer a separate night palette preview', 'rejected', 8, (10,), 'orion',
     ('design',), 'The theme picker already offers the approved dark map.'),
    ('RELAY', 'immediate-replay', 'Replay pending frames as soon as the backup relay connects', 'rejected', 6, (1, 3), 'orion',
     ('handover',), 'Waiting for the acknowledgement window avoids duplicate traffic at the receivers.'),
    ('RELAY', 'hold-rollout', 'Hold the handover rehearsal at 25% traffic', 'proposed', 1, (1, 10), 'orion',
     ('handover', 'rollout'), 'One retransmission appeared in the simulated far-side relay.'),
    ('RELAY', 'manual-handover', 'Hand over relays manually during rehearsals', 'superseded', 14, (1,), 'mira',
     ('handover',), 'Operators wanted to watch every swap during the first rehearsal.'),
    ('RELAY', 'window-handover', 'Hand over after the acknowledgement window closes', 'accepted', 5, (1, 2, 3), 'mira',
     ('handover', 'reliability'), 'It keeps confirmed frames from being replayed after a swap.'),
)
FLEET_SUPERSESSIONS = (('two-workers', 'viewport-pool'), ('any-crossing', 'sustained-rise'),
                       ('manual-handover', 'window-handover'))
FLEET_DESCRIPTIONS = {
    'NAV-1': '## Fallow Reach chart pans\n\nPanning the chart stalls while tiles decode on the UI thread. '
             'Move decoding to a worker pool and keep the pointer path under 4 ms.\n\n'
             '- Warm pans hold at **41 ms** on the pilot replay.\n'
             '- The first cross-sector pan still peaks at 87 ms; the cold pass is the release gate.\n\n'
             '```sh\natlas replay --route fallow-reach --cache cold\n```\n',
}
FLEET_COMMENTS = {
    'NAV-1': (('mira', 'Can you rerun the pan with a cold tile cache before the pilot review?'),
              ('castor', 'Cold pass re-run on the pilot laptop: 87 ms peak on the first seam, 41 ms warm. '
                         'Trace attached to the pull request.'),
              ('tavi', 'Matches the baseline from NAV-6. Leaving the cold pass as the gate.')),
    'RELAY-1': (('mira', 'Hold the rollout at 25% until the rollback run completes.'),
                ('orion', 'Rollback run queued for the Fallow Reach rehearsal; no lost frames so far.')),
}
FLEET_RELATIONS = {'NAV-1': ('NAV-18', 'MAP-11'), 'NAV-18': ('NAV-1',)}
# The GitHub fixture's repositories, and the numbers of its pull requests.
FLEET_REPOSITORIES = dict(NAV='acme/web', MAP='acme/design-system', WARP='acme/api', RELAY='acme/api')
FLEET_PULLS = {'NAV-1': 491, 'NAV-5': 1183, 'NAV-7': 492, 'NAV-8': 106, 'NAV-11': 489, 'MAP-1': 493,
               'WARP-7': 1186, 'RELAY-1': 1188, 'RELAY-3': 1187}


FLEET_FOLLOW_UPS = ('Record the trace in the pilot review notes', 'Add the benchmark to CI',
                    'Split the change behind a setting')


def fleet_person(code):
    if code in FLEET_PEOPLE:
        return person(FLEET_PEOPLE[code])
    name, fleet = FLEET_CAPTAINS[code]
    return dict(person(name), kind='agent', agent=dict(rank='captain', fleet=fleet, ship=name))


def fleet_pull_requests(row, open_count, merged, now):
    repository = FLEET_REPOSITORIES[row['projectKey']]
    if row['identifier'] in ('NAV-5', 'MAP-1'):
        repository = {'NAV-5': 'acme/api', 'MAP-1': 'acme/web'}[row['identifier']]
    owner, name = repository.split('/')
    number = FLEET_PULLS.get(row['identifier'], 300 + int(row['identifier'].split('-')[1]) * 3)
    rows = []
    for index in range(open_count + merged):
        state = 'open' if index < open_count else 'merged'
        rows.append(dict(id=uid(f"pr-{row['identifier']}-{index}"), repository=dict(
            owner=owner, name=name, fullName=repository, url=f'https://github.com/{repository}'),
            number=number - 7 * index, url=f'https://github.com/{repository}/pull/{number - 7 * index}',
            title=f"{row['title']} ({row['identifier']})" if index == 0 else f"{FLEET_FOLLOW_UPS[index % 3]} ({row['identifier']})",
            authorLogin='octo-dev',
            state=state, isDraft=False, headBranch=f"{row['identifier'].lower()}-{number - 7 * index}", baseBranch='main',
            referenceKind='closes', referenceSources=['title', 'branch'], referenceActive=True,
            completion='completed' if state == 'merged' else 'none', mergedAt=now if state == 'merged' else None,
            closedAt=now if state == 'merged' else None, githubUpdatedAt=now, lastObservedAt=now))
    return rows


def add_fleet(fixture, now):
    today = datetime.now(timezone.utc).date()
    labels = {name: dict(id=uid('label-' + name), name=name, color=color, state='active')
              for name, color in FLEET_LABELS.items()}
    for key, name, lead, rows in FLEET_PROJECTS:
        lead_person = person(FLEET_PEOPLE[lead])
        fixture.projects.append(dict(project(key, name, now), leadId=lead_person['userId'], leadName=lead_person['displayName']))
        for number, (title, kind, label, priority, parent, assignee, extra) in enumerate(rows, 1):
            row = issue(f'{key}-{number}', title, kind, now)
            row.update(assignee=fleet_person(assignee) if assignee else None, owner=person(FLEET_PEOPLE[lead]),
                       labels=[labels[label]], priority=priority, estimate=extra.get('e'),
                       commentCount=extra.get('c', 0),
                       dueDate=(today + timedelta(days=extra['d'])).isoformat() if 'd' in extra else None,
                       summary=f'Fictional {name} work on the Fallow Reach rehearsal.',
                       parentId=uid(f'{key}-{parent}') if parent else None, subIssueRank=number if parent else None)
            if 'p' in extra:
                fixture.pull_requests[row['id']] = fleet_pull_requests(row, *extra['p'], now)
                row['pullRequestSummary'] = pull_request_summary(fixture.pull_requests[row['id']])
            fixture.issues.append(row)
            fixture.creators[row['id']] = person(FLEET_PEOPLE[lead])
            fixture.events[row['id']] = [activity(row, now)]
        trackers = {r['parentId'] for r in fixture.issues if r['parentId'] and r['projectKey'] == key}
        for row in fixture.issues:
            if row['id'] in trackers:
                children = [r for r in fixture.issues if r['parentId'] == row['id']]
                row.update(type='tracker', estimate=None, progress=dict(
                    done=sum(r['state']['type'] == 'completed' for r in children), total=len(children),
                    canceled=0, duplicate=0, hasMore=False,
                    sample=[dict(id=r['id'], stateType=r['state']['type']) for r in children]))
    project_row = {p['key']: p for p in fixture.projects}
    fixture.projects.append(project('FLT', 'Fleet fixture', now))
    for ref, title, kind, _ in FLEET_ISSUES:
        if ref.startswith('FLT'):
            row = issue(ref, title, kind, now)
            assignee = FLT_ASSIGNEES.get(ref)
            row.update(assignee=person(assignee) if assignee else None,
                       summary='Fictional fleet work for web Space evidence.')
            fixture.issues.append(row)
            fixture.creators[row['id']] = ADA
            fixture.events[row['id']] = [activity(row, now)]
    for ref, body in FLEET_DESCRIPTIONS.items():
        fixture.descriptions[uid(ref)] = body
    nav = fixture.find_issue('NAV-1')
    fixture.add_media(nav, 'Cold-cache pan trace.png', 'image/png', fixture_png(), True)
    fixture.add_media(nav, 'Pilot review notes.md', 'text/markdown',
                      b'# Pilot review\n\nWarm pans: 41 ms. First cross-sector pan: 87 ms.\n', False)
    nav.update(milestoneId=uid('NAVmilestone'), lastActivityAt=now,
               summary='Panning the chart stalls while tiles decode; move decoding to a worker pool.')
    mira, castor = fleet_person('mira'), fleet_person('castor')
    fixture.todos[nav['id']] = [
        dict(id=uid(f'nav-1-todo-{index}'), issueId=nav['id'], text=text, done=done, position=index,
             createdBy=mira, createdAt=now, doneBy=castor if done else None, doneAt=now if done else None,
             updatedBy=castor if done else None, updatedAt=now, version=1)
        for index, (text, done) in enumerate((('Move tile decoding to a worker pool', True),
                                              ('Capture the warm-pan baseline (NAV-6)', True),
                                              ('Cancel decodes that leave the viewport (NAV-7)', False),
                                              ('Re-run the cold-cache pan on the pilot laptop', False)), 1)]
    nav['todoCounts'] = dict(total=4, done=2)
    for ref, notes in FLEET_COMMENTS.items():
        row = fixture.find_issue(ref)
        for code, body in notes:
            posted = comment(row['id'], body, None, now, fleet_person(code))
            fixture.comments.append(posted)
            fixture.events[row['id']].insert(0, activity(row, now, 'comment.created', posted))
        row['commentCount'] = len(notes)
        row['lastActivity'] = dict(kind='comment.created', occurredAt=now, actor=actor(fleet_person(notes[-1][0])))
    add_fleet_decisions(fixture, project_row, now)


def add_fleet_decisions(fixture, project_row, now):
    instant_now = parse_instant(now)
    records = {}
    leads = {key: lead for key, _, lead, _ in FLEET_PROJECTS}
    for key, slug, title, status, days, numbers, maker, tags, why in FLEET_DECISIONS:
        linked = [fixture.find_issue(f'{key}-{number}') for number in numbers]
        at = (instant_now - timedelta(days=days, hours=2)).isoformat().replace('+00:00', 'Z')
        decided = (instant_now - timedelta(days=days)).date().isoformat()
        row, detail = decision_record(slug, title, status, at, decided, project_row[key], linked,
                                      title + '.', maker=fleet_person(maker))
        row.update(tags=list(tags), evidenceExcerpt=why,
                   recordedByUserId=person(FLEET_PEOPLE[leads[key]])['userId'])
        detail['why'] = why
        detail['provenance'] = dict(detail['provenance'], evidenceExcerpt=why)
        records[slug] = (row, detail, maker)
    for old, new in FLEET_SUPERSESSIONS:
        old_row, new_row = records[old][0], records[new][0]
        old_row.update(supersededById=new_row['id'], supersededBy=decision_summary(new_row), revisionNumber=2, version=2)
        new_row.update(supersedesId=old_row['id'], supersedes=decision_summary(old_row))
    for row, detail, maker in records.values():
        fixture.decisions[row['id']] = (row, detail)
        for link in row['issues']['items']:
            event = activity(link, row['createdAt'], 'decision.logged')
            event.update(actor=actor(fleet_person(maker)), data=dict(decisionId=row['id']))
            fixture.events[link['id']].insert(0, event)


class IssueWrites:
    def create_issue(self, payload):
        validate_issue_payload(payload, True)
        project_row = next((p for p in self.projects if p['id'] == payload.get('projectId')), None)
        if project_row is None:
            raise Problem(400, 'Choose a project.')
        if project_row['archivedAt']:
            raise Problem(409, 'The project is archived.')
        key = project_row['key']
        sequence = max(int(r['identifier'].split('-')[1]) for r in self.issues if r['projectKey'] == key) + 1
        row = issue(f'{key}-{sequence}', payload['title'].strip(), 'backlog', instant())
        row.update(assignee=None, labels=[], priority=0, estimate=None)
        self.apply_issue(row, payload)
        self.issues.append(row)
        self.creators[row['id']] = ADA
        self.events[row['id']] = [activity(row, row['createdAt'])]
        self.refresh_trackers()
        return self.detail(row)

    def patch_issue(self, row, payload):
        validate_issue_payload(payload, False)
        if row['archivedAt'] or payload.get('version') != row['version']:
            raise Problem(409, 'The issue changed. Read the latest issue.', dict(currentVersion=row['version']))
        project_row = next(p for p in self.projects if p['id'] == row['projectId'])
        if project_row['archivedAt']:
            raise Problem(409, 'The project is archived.')
        if payload.get('type', row['type']) != 'tracker' and row['type'] == 'tracker':
            if any(r['parentId'] == row['id'] for r in self.issues):
                raise Problem(409, 'A tracker with children cannot be converted.')
        self.apply_issue(row, payload)
        row['version'] += 1
        row['updatedAt'] = instant()
        self.refresh_trackers()
        return self.detail(row)

    def apply_issue(self, row, payload):
        kind = payload.get('type', row['type'])
        if kind == 'tracker' and ('stateId' in payload or payload.get('estimate') is not None or payload.get('cycleId') is not None):
            raise Problem(400, 'Trackers derive state from children and accept no estimate.')
        values = issue_properties(self, row, payload)
        for key in ('title', 'summary', 'priority', 'estimate', 'dueDate', 'type'):
            if key in payload:
                values[key] = payload[key]
        if kind == 'tracker':
            values['estimate'] = None
            values.update(cycleId=None, cycle=None)
        row.update(values)
        if 'description' in payload:
            self.descriptions[row['id']] = payload['description']

    def refresh_trackers(self):
        for row in self.issues:
            if row['type'] != 'tracker':
                continue
            children = [r for r in self.issues if r['parentId'] == row['id'] and not r['archivedAt']]
            types = [r['state']['type'] for r in children]
            kind = tracker_state(types)
            row['state'] = next({k: s[k] for k in ('id', 'name', 'type')}
                                for s in states(row['projectKey']) if s['type'] == kind)
            row['progress'] = dict(done=types.count('completed'), total=len(types), canceled=types.count('canceled'),
                                   duplicate=types.count('duplicate'), hasMore=len(types) > 20,
                                   sample=[dict(id=r['id'], stateType=r['state']['type']) for r in children[:20]])


def tracker_state(types):
    if not types:
        return 'backlog'
    if all(t in TERMINAL for t in types):
        return 'completed' if 'completed' in types else 'canceled'
    if any(t in (*TERMINAL, 'started') for t in types):
        return 'started'
    return min(types, key=STATE_TYPES.index)


def validate_issue_payload(payload, create):
    allowed = {'title', 'summary', 'description', 'stateId', 'assigneeId', 'priority', 'estimate',
               'dueDate', 'labelIds', 'parentId', 'type', 'milestoneId', 'cycleId', 'projectId' if create else 'version'}
    if not isinstance(payload, dict) or not payload or set(payload) - allowed:
        raise Problem(400, 'Supply editable issue fields only.')
    title = payload.get('title')
    if create or 'title' in payload:
        if not isinstance(title, str) or not title.strip() or any(c in title for c in '\r\n\0') or len(title.encode('utf-16-le')) // 2 > 200:
            raise Problem(400, 'Enter a single-line title of 1–200 characters.')
    summary = payload.get('summary')
    if create or 'summary' in payload:
        if not isinstance(summary, str) or not summary.strip() or len(summary) > 300 or any(c in summary for c in '\r\n\0\u2028\u2029'):
            raise Problem(400, 'Enter a single-line summary of 1–300 characters. A summary cannot be cleared once set.')
    description = payload.get('description')
    if description is not None and (not isinstance(description, str) or len(description.encode('utf-16-le')) // 2 > 50000):
        raise Problem(400, 'Description must be at most 50000 characters.')
    if 'priority' in payload and payload['priority'] not in range(5):
        raise Problem(400, 'Invalid priority.')
    if 'type' in payload and payload['type'] not in ('issue', 'tracker'):
        raise Problem(400, 'Invalid issue type.')
    if payload.get('dueDate') is not None:
        try:
            datetime.strptime(payload['dueDate'], '%Y-%m-%d')
        except (ValueError, TypeError) as error:
            raise Problem(400, 'Invalid due date.') from error


def planning_rows(key, cycles=False):
    if cycles:
        return [dict(id=uid(key + 'cycle'), projectId=uid(key), number=1, state='current')]
    return [dict(id=uid(key + 'milestone'), name='First delivery', projectId=uid(key), description='Fixture milestone description.')]


def planning_properties(row, payload):
    values = {}
    for field, cycles in (('milestoneId', False), ('cycleId', True)):
        if field not in payload:
            continue
        selected = next((p for p in planning_rows(row['projectKey'], cycles) if p['id'] == payload[field]), None)
        if payload[field] is not None and selected is None:
            raise Problem(400, 'Choose a planning value in this project, or clear it.')
        values[field] = payload[field]
        if cycles:
            values['cycle'] = {k: selected[k] for k in ('id', 'number', 'state')} if selected else None
    return values


def issue_properties(fixture, row, payload):
    values = planning_properties(row, payload)
    if 'stateId' in payload:
        state = next((s for s in states(row['projectKey']) if s['id'] == payload['stateId']), None)
        if state is None:
            raise Problem(400, 'Choose a state from this project.')
        values['state'] = {k: state[k] for k in ('id', 'name', 'type')}
    if 'assigneeId' in payload:
        people = {p['userId']: p for p in (ADA, GRACE, AGENT)}
        if payload['assigneeId'] is not None and payload['assigneeId'] not in people:
            raise Problem(400, 'Choose an active human or agent.')
        values['assignee'] = people.get(payload['assigneeId'])
    if 'labelIds' in payload:
        ids = payload['labelIds']
        if not isinstance(ids, list) or len(ids) > 100 or any(i != uid('label') for i in ids):
            raise Problem(400, 'Choose active labels.')
        values['labels'] = [dict(id=i, name='Fixture', color='#6C8EEF', state='active') for i in ids]
    if 'parentId' in payload:
        parent = fixture.find_issue(payload['parentId']) if payload['parentId'] else None
        if payload['parentId'] and (not parent or parent['type'] != 'tracker' or parent['projectId'] != row['projectId']
                                    or parent['parentId'] or parent['id'] == row['id'] or payload.get('type', row['type']) == 'tracker'):
            raise Problem(400, 'Choose a top-level tracker in this project.')
        values['parentId'] = parent['id'] if parent else None
    if payload.get('estimate') is not None:
        scale = next(p['estimateScale'] for p in fixture.projects if p['id'] == row['projectId'])
        points = {'fibonacci': (1, 2, 3, 5, 8), 'exponential': (1, 2, 4, 8, 16),
                  'linear': (1, 2, 3, 4, 5), 'tShirt': (1, 2, 3, 5, 8)}.get(scale, ())
        if payload['estimate'] not in points:
            raise Problem(400, 'Choose an estimate on the project scale.')
    return values


def issue_write_route(handler, method, path, query):
    fixture = handler.server.fixture
    if method == 'GET' and path == '/api/v1/users':
        now = instant()
        people = [dict(id=p['userId'], displayName=p['displayName'], kind=p['kind'], email=None,
                       createdAt=now, updatedAt=now, lastActiveAt=now)
                  for p in (ADA, GRACE, AGENT) + (tuple(map(person, FLEET_PEOPLE.values())) if fixture.fleet else ())]
        if 'kind' in query:
            people = [p for p in people if p['kind'] in query['kind']]
        return 200, page(people, query, 'users')
    if method == 'GET' and path == '/api/v1/labels':
        labels = [dict(id=uid('label'), name='Fixture', color='#6C8EEF', state='active',
                       createdAt=instant(), updatedAt=instant())]
        if 'state' in query:
            labels = [l for l in labels if l['state'] in query['state']]
        return 200, page(labels, query, 'labels')
    match = re.fullmatch(r'/api/v1/projects/([^/]+)', path)
    if method == 'GET' and match:
        project_row = next((p for p in fixture.projects if match[1] in (p['key'], p['id'])), None)
        if project_row is None:
            raise Problem(404, 'Project not found.')
        return 200, project_row
    if method == 'POST' and path == '/api/v1/issues':
        return 201, fixture.create_issue(handler.read_json())
    return None



def validate_chat_upload(name, mime, data):
    import io, zipfile
    types = {'zip': 'application/zip', 'pdf': 'application/pdf',
             'docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
             'xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'}
    supported = {'text/plain', 'text/markdown', 'image/png', 'image/jpeg', 'image/gif', 'image/webp', *types.values()}
    if mime not in supported:
        raise Problem(400, 'Unsupported chat file type.')
    if mime in types.values() and types.get(name.rsplit('.', 1)[-1].lower()) != mime:
        raise Problem(400, 'Match the filename extension to its type.')
    if mime == 'application/pdf' and not data.startswith(b'%PDF-'):
        raise Problem(400, 'Invalid PDF signature.')
    if mime in (types['zip'], types['docx'], types['xlsx']):
        try:
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                entries = archive.infolist()
                if len(entries) > 4096 or sum(e.file_size for e in entries) > 250000000 or any(e.flag_bits & 1 for e in entries):
                    raise Problem(400, 'Archive exceeds fixture limits.')
                if archive.testzip() is not None:
                    raise Problem(400, 'Invalid archive checksum.')
                part = 'word/document.xml' if mime == types['docx'] else 'xl/workbook.xml'
                if mime != types['zip'] and ('[Content_Types].xml' not in archive.namelist() or part not in archive.namelist()):
                    raise Problem(400, 'Invalid Office package.')
        except (zipfile.BadZipFile, RuntimeError, NotImplementedError) as error:
            raise Problem(400, 'Invalid archive.') from error

class Fixture(IssueWrites):
    def __init__(self, fleet=False):
        now = instant()
        self.projects = [
            dict(project('SUB', 'Subspace fixture', now), initiativeId='ca2aabe7-68bd-4e82-9b76-d577ddf987ac'),
            dict(project('OPS', 'Operations fixture', now), initiativeId='47d093d1-2498-4943-b0fd-b816d3b83425'),
        ]
        created = (datetime.now(timezone.utc) - timedelta(hours=1)).isoformat().replace('+00:00', 'Z')
        self.issues = seed_issues(created)
        self.pull_requests = {self.issues[0]['id']: [pull_request(index, now) for index in range(25)]}
        self.pull_requests_available = True
        for row in self.issues:
            row['pullRequestSummary'] = pull_request_summary(self.pull_requests.get(row['id'], []))
        self.aliases = {}
        self.chat_uploads = {}
        import base64, io, zipfile
        bundle = io.BytesIO()
        with zipfile.ZipFile(bundle, 'w', zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('src/hello.rs', 'fn main() { println!("Hello, captain"); }\n')
        for name, mime, data in [('review-brief.md', 'text/markdown', '\n'.join(f'{n}. Fixture review point {n}' for n in range(1, 26)).encode()),
                                 ('workspace-sketch.png', 'image/png', fixture_png()),
                                 ('captain-code.zip', 'application/zip', bundle.getvalue())]:
            upload_id = uuid.UUID(uid(name))
            key = 'UPL-' + base64.urlsafe_b64encode(upload_id.bytes).decode().rstrip('=')
            created_at = datetime.now(timezone.utc)
            meta = dict(id=str(upload_id), reference=key, fileName=name, contentType=mime, sizeBytes=len(data),
                sha256=hashlib.sha256(data).hexdigest(), uploadedByUserId=ADA['userId'], createdAt=created_at.isoformat(),
                expiresAt=(created_at + timedelta(days=30)).isoformat(), url=f'/api/v1/chat/uploads/{key}/content')
            self.chat_uploads[key] = (meta, data)
        self.descriptions = {}
        self.creators = {r["id"]: ADA for r in self.issues}
        self.media = {}
        for number in range(3):
            self.add_media(self.issues[0], f'Fixture sketch {number + 1}.png', 'image/png', fixture_png(), True)
        self.add_media(self.issues[0], 'Fixture notes.txt', 'text/plain', b'Fixture notes\n', False)
        self.add_media(self.issues[0], 'Fixture plan.md', 'text/markdown', FIXTURE_PLAN, False)
        root = comment(self.issues[0]['id'], 'Ready for the fixture review.', None, now)
        media = list(self.media.values())
        root['body'] += f'\n\n![Review sketch](nett-media:{media[1]["id"]})\n\n[Notes](nett-media:{media[3]["id"]})'
        root['media'] = [descriptor(media[1]), descriptor(media[3])]
        root.update(replyCount=1, hasReplies=True)
        self.comments = [root, comment(root['issueId'], 'The connection check passed.', root['id'], now, GRACE)]
        self.issues[0]['commentCount'] = 2
        self.events = {r['id']: [activity(r, created)] for r in self.issues}
        self.decisions = seed_decisions(self.projects[0], self.issues[0], self.issues[3], now)
        for decision, _ in self.decisions.values():
            maker = AGENT if decision['madeByType'] == 'agent' else ADA
            for link in decision['issues']['items']:
                event = activity(link, decision['createdAt'], 'decision.logged')
                event.update(actor=actor(maker), data=dict(decisionId=decision['id']))
                self.events[link['id']].insert(0, event)
        self.events[root['issueId']].insert(0, activity(self.issues[0], now, 'comment.created', root))
        self.issues[0]['lastActivityAt'] = now
        self.issues[0]['lastActivity'] = dict(kind='comment.created', occurredAt=now, actor=actor(ADA))
        self.fleet = fleet
        self.todos = {}
        if fleet:
            add_fleet(self, created)

    def find_issue(self, ref):
        return next((r for r in self.issues if ref in (r['identifier'], r['id']) or self.aliases.get(ref) == r['id']), None)

    def preview_move(self, row, payload):
        destination = next((p for p in self.projects if p['id'] == payload.get('destinationProjectId')), None)
        if destination is None or destination['id'] == row['projectId']:
            raise Problem(400, 'Choose another project.')
        if row['version'] != payload.get('version'):
            raise Problem(409, 'The issue changed. Refresh the preview.')
        source = next(p for p in self.projects if p['id'] == row['projectId'])
        moving = [row] + [r for r in self.issues if r['parentId'] == row['id']]
        number = max((int(r['identifier'].split('-')[1]) for r in self.issues if r['projectId'] == destination['id']), default=0)
        items = []
        for offset, item in enumerate(moving, 1):
            state = next(state for state in states(destination['key']) if state['type'] == item['state']['type'])
            items.append(dict(id=item['id'], version=item['version'], oldIdentifier=item['identifier'],
                newIdentifier=f"{destination['key']}-{number + offset}", stateId=state['id'], stateName=state['name'],
                cycleCleared=item['cycleId'] is not None, milestoneCleared=item['milestoneId'] is not None,
                estimateCleared=item['estimate'] is not None, parentDetached=bool(item['parentId'] and item is row),
                archived=item['archivedAt'] is not None))
        summary = lambda p: {k: p[k] for k in ('id', 'key', 'name')}
        token = hashlib.sha256(json.dumps(items, sort_keys=True).encode()).hexdigest()
        return dict(token=token, source=summary(source), destination=summary(destination), issues=items)

    def move_issue(self, row, payload):
        preview = self.preview_move(row, payload)
        if preview['token'] != payload.get('token'):
            raise Problem(409, 'The preview changed. Review the new identifiers.')
        for item in preview['issues']:
            current = self.find_issue(item['id'])
            self.aliases[current['identifier']] = current['id']
            current.update(identifier=item['newIdentifier'], projectId=preview['destination']['id'],
                projectKey=preview['destination']['key'], version=current['version'] + 1,
                estimate=None, milestoneId=None, cycleId=None, cycle=None)
            if item['parentDetached']:
                current.update(parentId=None, subIssueRank=None)
            state = next(state for state in states(current['projectKey']) if state['id'] == item['stateId'])
            current['state'] = {k: state[k] for k in ('id', 'name', 'type')}
        return dict(issue=self.detail(row), moved=preview['issues'])

    def relations(self, row):
        if row['projectKey'] not in ('SUB', 'OPS'):
            related = [self.find_issue(ref) for ref in FLEET_RELATIONS.get(row['identifier'], ())]
            return [dict(target, kind='relatedTo') for target in related]
        target = self.issues[4] if row['projectKey'] == 'SUB' else self.issues[1]
        return [dict(target, kind='relatedTo')]

    def detail(self, row):
        children = [r for r in self.issues if r['parentId'] == row['id']]
        parent = next((r for r in self.issues if r['id'] == row['parentId']), None)
        image = next((m for m in self.media.values() if m['issueId'] == row['id'] and m['kind'] == 'image' and not m['deleted']), None)
        body = self.descriptions.get(row['id'])
        if body is None:
            body = '## Fixture workspace\n\nRead issues and post text comments.\n'
            if image:
                body += f'\n![Workspace sketch](nett-media:{image["id"]})\n'
        return dict(row, description=body,
                    createdBy=self.creators.get(row['id']), startedAt=row['createdAt'] if row['state']['type'] == 'started' else None,
                    completedAt=row['createdAt'] if row['state']['type'] == 'completed' else None,
                    canceledAt=None, archivedBy=ADA if row['archivedAt'] else None, todos=self.todos.get(row['id'], []), mentions=[],
                    parent=reference(parent), duplicateOf=None,
                    children=dict(items=children[:20], nextCursor=None),
                    relations=dict(items=self.relations(row), nextCursor=None),
                    subscription=dict(explicitChoice=False, subscribed=False))

    def post(self, row, payload):
        parent_id = validate_comment(payload)
        root = find_root(self.comments, row['id'], parent_id)
        if row['archivedAt']:
            raise Problem(409, 'The issue is archived.')
        posted = comment(row['id'], payload['body'].replace('\r\n', '\n').replace('\r', '\n'), parent_id, instant())
        ids = re.findall(r'nett-media:([0-9a-f-]{36})', posted['body'])
        posted['media'] = [descriptor(m) for m in self.media.values()
                           if m['id'] in ids and m['issueId'] == row['id'] and not m['deleted']]
        self.comments.append(posted)
        if root:
            root['replyCount'] += 1
            root['hasReplies'] = True
        row['commentCount'] += 1
        row['lastActivityAt'] = posted['createdAt']
        row['lastActivity'] = dict(kind='comment.created', occurredAt=posted['createdAt'], actor=actor(ADA))
        self.events[row['id']].insert(0, activity(row, posted['createdAt'], 'comment.created', posted))
        return posted

    def add_media(self, row, name, mime, data, image):
        if row['archivedAt']:
            raise Problem(409, 'The issue is archived.')
        media_id = str(uuid.uuid4())
        kind, route = ('image', 'media') if image else ('file', 'attachments')
        record = dict(id=media_id, issueId=row['id'], fileName=name, contentType=mime,
                      sizeBytes=len(data), url=f'/api/v1/issues/{route}/{media_id}',
                      sha256=hashlib.sha256(data).hexdigest(), createdAt=instant(), uploadedBy=ADA,
                      bytes=data, kind=kind, deleted=False)
        self.media[media_id] = record
        return image_metadata(record) if image else descriptor(record)


def reference(row):
    if row is None:
        return None
    return {k: row[k] for k in ('id', 'identifier', 'title', 'summary', 'type', 'state')} | dict(archived=bool(row['archivedAt']))


class Problem(Exception):
    def __init__(self, status, detail, extensions=None):
        super().__init__(detail)
        self.status = status
        self.extensions = extensions or {}


def validate_comment(payload):
    if not isinstance(payload, dict) or set(payload) - {'body', 'parentCommentId'}:
        raise Problem(400, 'Supply body and optional parentCommentId only.')
    body = payload.get('body')
    if not isinstance(body, str) or not body.strip() or len(body.encode('utf-16-le')) // 2 > 50000:
        raise Problem(400, 'body must contain 1–50000 characters.')
    parent = payload.get('parentCommentId')
    if parent is not None and not isinstance(parent, str):
        raise Problem(400, 'parentCommentId must be a root UUID or null.')
    return parent


def find_root(comments, issue_id, parent):
    if parent is None:
        return None
    root = next((c for c in comments if c['id'] == parent and c['issueId'] == issue_id), None)
    if not root or root['parentCommentId'] or root['deletedAt']:
        raise Problem(400, 'Reply to an undeleted root on this issue.')
    return root


def number(query, name, default, low=1, high=200):
    try:
        value = int(query.get(name, [str(default)])[0])
    except ValueError as error:
        raise Problem(400, f'{name} must be an integer.') from error
    if not low <= value <= high:
        raise Problem(400, f'{name} must be between {low} and {high}.')
    return value


def parse_instant(value):
    if not re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{1,7})?(?:Z|[+-]\d\d:\d\d)', value):
        raise Problem(400, 'activitySince must include an offset.')
    try:
        return datetime.fromisoformat(value.replace('Z', '+00:00'))
    except ValueError as error:
        raise Problem(400, 'Invalid activitySince.') from error


def due_matches(row, value):
    today = datetime.now(timezone.utc).date()
    due = row['dueDate']
    if value == 'none':
        return due is None
    if due is None or row['state']['type'] in TERMINAL:
        return False
    if value == 'overdue':
        return due < today.isoformat()
    return today.isoformat() <= due <= (today + timedelta(days=6 - today.weekday())).isoformat()


def matches(row, query, parent_id):
    assignee = row['assignee']['userId'] if row['assignee'] else 'none'
    wanted = [ADA['userId'] if v == 'me' else v for v in query.get('assignee', [])]
    checks = [('project', (row['projectKey'], row['projectId'])), ('stateId', (row['state']['id'],)),
              ('stateType', (row['state']['type'],)), ('type', (row['type'],)),
              ('priority', (str(row['priority']),))]
    if any(name in query and not set(query[name]).intersection(values) for name, values in checks):
        return False
    if wanted and assignee not in wanted:
        return False
    if 'creator' in query and not set(query['creator']).intersection(('me', ADA['userId'])):
        return False
    if parent_id is not False and row['parentId'] != parent_id:
        return False
    text = query.get('q', [''])[0].casefold()
    return text in (row['identifier'] + ' ' + row['title'] + ' Fixture workspace Read issues and post text comments.').casefold()


def filtered(fixture, query):
    validate_filters(query)
    parent = query.get('parent', [None])[0]
    parent_row = fixture.find_issue(parent) if parent and parent != 'none' else None
    parent_id = False if parent is None else (parent_row['id'] if parent_row else None)
    if parent and parent != 'none' and not parent_row:
        return []
    rows = [r for r in fixture.issues if matches(r, query, parent_id)]
    if query.get('includeArchived', ['false'])[0] != 'true':
        rows = [r for r in rows if r['archivedAt'] is None]
    if 'activitySince' in query:
        since = parse_instant(query['activitySince'][0])
        rows = [r for r in rows if parse_instant(r['lastActivityAt']) >= since]
    if 'due' in query:
        rows = [r for r in rows if due_matches(r, query['due'][0])]
    return ordered(rows, query)


def validate_filters(query):
    choices = {'stateType': STATE_TYPES, 'type': ('issue', 'tracker'),
               'priority': ('0', '1', '2', '3', '4'), 'due': ('overdue', 'thisWeek', 'none'),
               'includeArchived': ('true', 'false')}
    for name, values in choices.items():
        if any(v not in values for v in query.get(name, [])):
            raise Problem(400, f'Invalid {name}.')


def ordered(rows, query):
    order = query.get('order', ['created'])[0]
    if order == 'manual':
        if 'project' not in query and 'parent' not in query:
            raise Problem(400, 'Manual order requires a project or parent.')
        return sorted(rows, key=lambda r: (r['subIssueRank'] or r['projectRank'], r['id']))
    if order == 'priority':
        return sorted(rows, key=lambda r: (r['priority'] or 5, r['dueDate'] or '9999', r['createdAt'], r['id']))
    if order not in ('created', 'activity'):
        raise Problem(400, 'Unknown order.')
    field = 'lastActivityAt' if order == 'activity' else 'createdAt'
    return sorted(rows, key=lambda r: (r[field], r['id']), reverse=True)


def page(items, query, scope, maximum=200):
    limit = number(query, 'limit', 50, high=maximum)
    fingerprint = hashlib.sha256(json.dumps([scope, sorted((k, v) for k, v in query.items()
                                                        if k not in ('cursor', 'limit'))]).encode()).hexdigest()[:16]
    start = 0
    if 'cursor' in query:
        try:
            stamp, offset = base64.urlsafe_b64decode(query['cursor'][0]).decode().split(':')
            start = int(offset)
            if stamp != fingerprint or not 0 <= start <= len(items):
                raise ValueError('Invalid cursor')
        except (ValueError, UnicodeError) as error:
            raise Problem(400, 'Cursor does not belong to this list and filters.') from error
    end = start + limit
    cursor = base64.urlsafe_b64encode(f'{fingerprint}:{end}'.encode()).decode() if end < len(items) else None
    return dict(items=items[start:end], nextCursor=cursor)


def decision_uuid(value):
    try:
        return str(uuid.UUID(value))
    except ValueError:
        return None


def decision_list(fixture, query):
    def value(key):
        return query.get(key, [''])[0].strip()

    issue_ref = value('issue')
    issue_id = decision_uuid(issue_ref)
    if issue_ref and not issue_id and not re.fullmatch(r'[\w-]+-[1-9]\d*', issue_ref):
        raise Problem(400, 'Use an issue ID or an identifier such as SUB-1.')
    issue_row = fixture.find_issue(issue_id or issue_ref.upper()) if issue_ref else None
    project_ref = value('project').upper()
    statuses = set(query.get('status', []))
    maker = value('madeByType')
    if statuses - {'proposed', 'accepted', 'rejected', 'superseded'} or maker not in ('', 'human', 'agent'):
        raise Problem(400, 'Invalid decision status or maker type.')
    tag = value('tag')
    search = value('q').casefold()
    rows = []
    for row, detail in fixture.decisions.values():
        if issue_ref and (issue_row is None or not any(link['id'] == issue_row['id'] for link in row['issues']['items'])):
            continue
        if project_ref and project_ref not in (row['projectKey'], row['projectId'].upper()):
            continue
        if (statuses and row['status'] not in statuses) or (maker and row['madeByType'] != maker):
            continue
        if tag and tag not in row['tags']:
            continue
        if search and not any(search in text.casefold() for text in (
                row['title'], *(detail[key] for key in ('decision', 'context', 'why', 'implications', 'alternatives')))):
            continue
        rows.append(row)
    # Subspace API's order: newest decided-on date first, then ID.
    rows.sort(key=lambda row: row['id'])
    rows.sort(key=lambda row: row['decidedOn'], reverse=True)
    return page(rows, query, 'decisions', maximum=100)


def groups(fixture, query):
    by = query.get('groupBy', [''])[0]
    if by not in ('state', 'stateType') or (by == 'state' and 'project' not in query):
        raise Problem(400, 'groupBy requires state with project, or stateType.')
    if 'limit' in query or 'cursor' in query:
        raise Problem(400, 'Groups use limitPerGroup, not limit or cursor.')
    limit = number(query, 'limitPerGroup', 25, low=0, high=25)
    rows = filtered(fixture, query)
    key = query.get('project', [''])[0]
    project_row = next((p for p in fixture.projects if key in (p['id'], p['key'])), None)
    if by == 'state' and project_row is None:
        raise Problem(404, 'Project not found.')
    result = []
    for kind in STATE_TYPES:
        members = [r for r in rows if r['state']['type'] == kind]
        continuation = {k: v for k, v in query.items() if k not in ('groupBy', 'limitPerGroup')}
        state_id = uid(project_row['key'] + kind) if by == 'state' else None
        continuation['stateId' if state_id else 'stateType'] = [state_id or kind]
        first = page(members, dict(continuation, limit=[str(max(limit, 1))]), 'issues')
        if limit == 0 or kind in TERMINAL:
            first = dict(items=[], nextCursor=None)
        result.append(dict(stateId=state_id, stateType=kind, count=len(members), **first))
    return dict(groupBy=by, groups=result)


UPDATES_PATH = '/api/v1/updates'
HEARTBEAT_SECONDS = 10
MAX_PROJECTS = 64


def update_project_reference(value):
    value = value.strip().upper()
    try:
        identifier = uuid.UUID(value)
    except ValueError:
        return value if re.fullmatch(r'[A-Z][A-Z0-9]{1,4}', value) else None
    return str(identifier).upper() if identifier.int else None


class UpdateFeed:
    """A queue per HTTP stream, filtered by its authorised project scope."""

    def __init__(self, enabled):
        self.enabled = enabled
        self.lock = threading.Lock()
        self.streams = {}

    def subscribe(self, projects):
        events = queue.SimpleQueue()
        with self.lock:
            self.streams[events] = projects
        return events

    def unsubscribe(self, events):
        with self.lock:
            self.streams.pop(events, None)

    def changed(self, fixture, value, action, previous_project=None):
        """Broadcasts a hint for an issue written through the fixture; a move
        also reaches the project it left."""
        if not isinstance(value, dict) or not {'id', 'identifier', 'version', 'projectKey'} <= value.keys():
            return
        projects = {value['projectKey'].upper(), (previous_project or value['projectKey']).upper()}
        parent = fixture.find_issue(value['parentId']) if value.get('parentId') else None
        event = dict(projectKey=value['projectKey'], issueId=value['id'], identifier=value['identifier'],
                     parentIdentifier=parent['identifier'] if parent else None, version=value['version'],
                     auditId=str(uuid.uuid4()), action=action, actorType='human', occurredAt=instant())
        with self.lock:
            for stream, subscribed in self.streams.items():
                if projects & subscribed:
                    stream.put(event)


def write_hint(fixture, method, path, status, value):
    """The hints a successful write sends, like the API's updates stream: each is
    the issue, its dotted action and the project it left on a move.
    Collaboration writes keep the issue's version."""
    if method not in ('POST', 'PATCH', 'DELETE') or status >= 300:
        return []
    path = unquote(path)
    if method == 'POST' and path == '/api/v1/issues':
        return [(value, 'issue.created', None)]
    match = re.fullmatch(r'/api/v1/issues/(media|attachments)/([^/]+)(/delete)?', path)
    if match:
        record = fixture.media.get(match[2])
        return [(fixture.find_issue(record['issueId']), 'attachment.removed', None)] if record and value else []
    match = re.fullmatch(r'/api/v1/issues/([^/]+)(/.*)?', path)
    row = fixture.find_issue(match[1]) if match else None
    suffix = (match[2] or '') if match else None
    if row is None:
        return []
    if method == 'PATCH' and suffix == '':
        return [(value, 'issue.changed', None)]
    if method == 'POST' and suffix == '/move':
        # Moves notify the old and the new project with the new identifier.
        return [(fixture.find_issue(item['id']), 'issue.changed', item['oldIdentifier'].rsplit('-', 1)[0])
                for item in value['moved']]
    if method == 'POST' and suffix == '/comments':
        return [(row, 'comment.created', None)]
    if method == 'POST' and suffix in ('/images', '/attachments'):
        return [(row, 'attachment.added', None)]
    return []


class FixtureServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, port, access_jwt, fail_chat_uploads=None, updates_stream=True, change_after=None,
                 decision_scenario='normal', fleet=False):
        if not access_jwt:
            raise ValueError('A non-empty fixture Access JWT is required.')
        self.access_jwt = access_jwt
        # Opt-in failure injection: (file-name prefix, HTTP status) or None.
        self.fail_chat_uploads = fail_chat_uploads
        self.fixture = Fixture(fleet)
        self.lock = threading.RLock()
        self.updates = UpdateFeed(updates_stream)
        self.change_after = change_after
        self.decision_scenario = decision_scenario
        super().__init__(('127.0.0.1', port), Handler)

    def change_later(self, seconds, identifier='SUB-1'):
        """Edits an issue as another person would, after `seconds`, for live-update captures."""
        def change():
            with self.lock:
                row = self.fixture.find_issue(identifier)
                value = self.fixture.patch_issue(row, dict(version=row['version'],
                                                           title=row['title'] + ' (changed elsewhere)'))
                self.updates.changed(self.fixture, value, 'issue.changed')
        timer = threading.Timer(seconds, change)
        timer.daemon = True
        timer.start()

    @property
    def origin(self):
        return f'http://127.0.0.1:{self.server_port}'


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass  # Never log headers, bodies or JWTs.

    def do_GET(self):
        self.handle_request('GET')

    def do_POST(self):
        self.handle_request('POST')

    def do_PATCH(self):
        self.handle_request('PATCH')
    def do_DELETE(self):
        self.handle_request('DELETE')

    def handle_request(self, method):
        try:
            if not hmac.compare_digest(self.headers.get('Authorization', '').encode(),
                                       f'Bearer {self.server.access_jwt}'.encode()):
                raise Problem(401, 'A valid bearer token is required.')
            if method == 'GET' and urlsplit(self.path).path == UPDATES_PATH:
                return self.stream_updates()
            with self.server.lock:
                status, value = self.route(method)
                self.respond(status, value)
                for hint in write_hint(self.server.fixture, method, urlsplit(self.path).path, status, value):
                    self.server.updates.changed(self.server.fixture, *hint)
        except Problem as error:
            self.respond(error.status, dict(type=f'https://httpstatuses.com/{error.status}',
                                           title=self.responses[error.status][0], status=error.status,
                                           detail=str(error), instance=urlsplit(self.path).path,
                                           errors={}, correlationId='fixture', **error.extensions))

    def stream_updates(self):
        feed = self.server.updates
        if not feed.enabled:
            raise Problem(404, 'Route not found.')
        keys = parse_qs(urlsplit(self.path).query, keep_blank_values=True).get('project', [])
        keys = [update_project_reference(key) for key in keys]
        if not 1 <= len(keys) <= MAX_PROJECTS or any(key is None for key in keys):
            raise Problem(400, 'Specify between 1 and 64 valid project references.')
        with self.server.lock:
            references = {value.upper(): row['key'].upper() for row in self.server.fixture.projects
                          if not row.get('archivedAt') for value in (row['id'], row['key'])}
            try:
                projects = {references[key] for key in keys}
            except KeyError as error:
                raise Problem(404, 'Project unavailable.') from error
            events = feed.subscribe(projects)
        try:
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Cache-Control', 'no-cache')
            self.send_header('X-Content-Type-Options', 'nosniff')
            self.send_header('Connection', 'close')
            self.end_headers()
            self.close_connection = True
            self.send_event('ready')
            while True:
                try:
                    self.send_event('issueChanged', events.get(timeout=HEARTBEAT_SECONDS))
                except queue.Empty:
                    self.send_event('heartbeat')
        except OSError:
            pass  # The fixture client closed the stream.
        finally:
            feed.unsubscribe(events)

    def send_event(self, name, value=None):
        payload = json.dumps(value, separators=(',', ':'))
        self.wfile.write(f'event: {name}\ndata: {payload}\n\n'.encode())
        self.wfile.flush()

    def route(self, method):
        url = urlsplit(self.path)
        path = unquote(url.path)
        query = parse_qs(url.query, keep_blank_values=True)
        fixture = self.server.fixture
        write_route = issue_write_route(self, method, path, query)
        if write_route is not None:
            return write_route
        chat = self.chat_route(method, path)
        if chat is not None:
            return chat
        if method == 'GET' and (path == DECISIONS_PATH or path.startswith(DECISIONS_PATH + '/')):
            return self.decision_route(path, query)
        if method == 'GET' and path == '/api/v1/me':
            return 200, dict(userId=ADA['userId'], kind='human', displayName=ADA['displayName'],
                             email='ada@example.test', agentName=None, agentRank=None, fleet=None, ship=None)
        if method == 'GET' and path == '/api/v1/me/initiatives':
            return 200, dict(items=[
                dict(id='ca2aabe7-68bd-4e82-9b76-d577ddf987ac', name='Design Studio', channels='#design, #releases'),
                dict(id='47d093d1-2498-4943-b0fd-b816d3b83425', name='Observatory', channels='#data, #forecast'),
            ] + ([FLEET_INITIATIVE] if fixture.fleet else []))
        if method == 'GET' and path == '/api/v1/projects':
            return 200, page(fixture.projects, query, 'projects')
        match = re.fullmatch(r'/api/v1/projects/([^/]+)/workflow-states', path)
        if method == 'GET' and match:
            project_row = next((p for p in fixture.projects if match[1] in (p['key'], p['id'])), None)
            if project_row is None:
                raise Problem(404, 'Project not found.')
            return 200, page(states(project_row['key']), query, path)
        match = re.fullmatch(r'/api/v1/projects/([^/]+)/milestones', path)
        if method == 'GET' and (match or path == '/api/v1/issue-cycles'):
            key = match[1] if match else query.get('project', [''])[0]
            project_row = next((p for p in fixture.projects if key in (p['key'], p['id'])), None)
            if project_row is None:
                raise Problem(404, 'Project not found.')
            rows = planning_rows(project_row['key'], cycles=match is None)
            if match and query.get('includeContent', ['true']) == ['false']:
                rows = [dict(r, description=None) for r in rows]
            if 'milestoneId' in query:
                rows = [r for r in rows if r['id'] == query['milestoneId'][0]]
            return 200, page(rows, query, path, maximum=50)
        if method == 'GET' and path == '/api/v1/issues':
            return 200, page(filtered(fixture, query), query, 'issues')
        if method == 'GET' and path == '/api/v1/issues/groups':
            return 200, groups(fixture, query)
        match = re.fullmatch(r'/api/v1/issues/(media|attachments)/([^/]+)(/delete)?', path)
        if match:
            return self.media_route(method, match)
        match = re.fullmatch(r'/api/v1/issues/([^/]+)(/.*)?', path)
        if not match:
            raise Problem(404, 'Route not found.')
        row = fixture.find_issue(match[1])
        if row is None:
            raise Problem(404, 'Issue not found.')
        return self.issue_route(method, row, match[2] or '', query)

    def decision_route(self, path, query):
        fixture = self.server.fixture
        scenario = self.server.decision_scenario
        if scenario == 'forbidden':
            raise Problem(403, 'Decisions are forbidden in this fixture scenario.')
        if scenario == 'unavailable':
            raise Problem(503, 'Decisions are unavailable in this fixture scenario.')
        if path == DECISIONS_PATH:
            return 200, decision_list(fixture, query)
        ref = path.removeprefix(DECISIONS_PATH + '/')
        if '/' in ref:
            raise Problem(404, 'Route not found.')
        decision_id = decision_uuid(ref)
        if decision_id is None:
            raise Problem(400, 'Use a decision UUID.')
        record = fixture.decisions.get(decision_id)
        # The list and activity can still refer to an older decision whose detail is missing.
        if record is None or (scenario == 'missing' and record[0]['status'] == 'superseded'):
            raise Problem(404, 'Decision not found.')
        row, detail = record
        return 200, dict(row, **detail)

    def issue_route(self, method, row, suffix, query):
        fixture = self.server.fixture
        if method == 'POST' and suffix == '/move-preview':
            return 200, fixture.preview_move(row, self.read_json())
        if method == 'POST' and suffix == '/move':
            return 200, fixture.move_issue(row, self.read_json())
        if method == 'PATCH' and suffix == '':
            return 200, fixture.patch_issue(row, self.read_json())
        if method == 'GET' and suffix == '':
            if row['identifier'] == 'SUB-1' and self.server.change_after is not None:
                seconds, self.server.change_after = self.server.change_after, None
                self.server.change_later(seconds)
            return 200, fixture.detail(row)
        if method == 'GET' and suffix == '/pull-requests':
            if not fixture.pull_requests_available:
                raise Problem(404, 'Route not found.')
            rows = fixture.pull_requests.get(row['id'], [])
            if query.get('includeInactive', ['false'])[0] != 'true':
                rows = [pr for pr in rows if pr['referenceActive']]
            rows = sorted(rows, key=lambda pr: (pr['githubUpdatedAt'], pr['id']), reverse=True)
            return 200, page(rows, dict({'limit': ['20']}, **query), row['id'] + suffix, maximum=100)
        if suffix in ('/images', '/attachments'):
            image = suffix == '/images'
            if method == 'POST':
                name, mime, data = self.read_multipart(image)
                return 201, fixture.add_media(row, name, mime, data, image)
            if method == 'GET':
                rows = [m for m in fixture.media.values() if m['issueId'] == row['id'] and not m['deleted']
                        and (not image or m['kind'] == 'image')]
                return 200, page([image_metadata(m) if image else descriptor(m) for m in rows], query, row['id'] + suffix, maximum=100)
        if method == 'GET' and suffix == '/relations':
            rows = fixture.relations(row)
            if 'kind' in query:
                rows = [r for r in rows if r['kind'] in query['kind']]
            return 200, page(rows, query, row['id'] + suffix, maximum=100)
        if method == 'GET' and suffix == '/activity':
            result = page(fixture.events[row['id']], query, row['id'] + suffix, maximum=100)
            decisions = decision_references(fixture.decisions, result['items'])
            return 200, dict(result, references=dict(people=[ADA, GRACE], projects=[], issues=[], labels=[], decisions=decisions))
        if method == 'POST' and suffix == '/comments':
            return 201, fixture.post(row, self.read_json())
        if method == 'GET' and suffix == '/comments':
            rows = [c for c in fixture.comments if c['issueId'] == row['id'] and c['parentCommentId'] is None]
            return 200, page(sorted(rows, key=lambda c: (c['displayCreatedAt'], c['id']), reverse=True), query, row['id'] + suffix)
        match = re.fullmatch(r'/comments/([^/]+)/replies', suffix)
        if method == 'GET' and match:
            find_root(fixture.comments, row['id'], match[1])
            rows = [c for c in fixture.comments if c['issueId'] == row['id'] and c['parentCommentId'] == match[1]]
            return 200, page(sorted(rows, key=lambda c: (c['displayCreatedAt'], c['id'])), query, row['id'] + suffix)
        raise Problem(404, 'Route not found.')

    def chat_route(self, method, path):
        uploads = self.server.fixture.chat_uploads
        if method == 'POST' and path == '/api/v1/chat/uploads':
            import base64
            payload = self.read_json()
            failing = self.server.fail_chat_uploads
            if failing and str(payload.get('fileName', '')).startswith(failing[0]):
                raise Problem(failing[1], 'Fixture refused this upload.')
            upload_id = str(uuid.UUID(payload['uploadId']))
            data = base64.b64decode(payload['base64'], validate=True)
            if not 0 < len(data) <= 25000000:
                raise Problem(400, 'Keep uploads within 25 MB.')
            validate_chat_upload(payload['fileName'], payload['contentType'], data)
            key = 'UPL-' + base64.urlsafe_b64encode(uuid.UUID(upload_id).bytes).decode().rstrip('=')
            created = datetime.now(timezone.utc)
            meta = dict(id=upload_id, reference=key, fileName=payload['fileName'], contentType=payload['contentType'],
                sizeBytes=len(data), sha256=hashlib.sha256(data).hexdigest(), uploadedByUserId=ADA['userId'],
                createdAt=created.isoformat(), expiresAt=(created + timedelta(days=30)).isoformat(),
                url=f'/api/v1/chat/uploads/{key}/content')
            if key not in uploads:
                uploads[key] = (meta, data)
            return 201, uploads[key][0]
        if method == 'POST' and path == '/api/v1/resolve':
            keys = self.read_json().get('references', [])
            return 200, dict(items=[dict(reference=key, kind='chatUpload', status='available' if key in uploads else 'missing',
                upload=uploads[key][0] if key in uploads else None) for key in keys])
        match = re.fullmatch(r'/api/v1/chat/uploads/([^/]+)(/content)?', path)
        if not match:
            return None
        record = uploads.get(match[1]) or next((r for r in uploads.values() if r[0]['id'] == match[1]), None)
        if record is None:
            raise Problem(404, 'Upload not found.')
        if method == 'DELETE':
            del uploads[record[0]['reference']]
            return 200, dict(deleted=True)
        if method == 'GET':
            return 200, MediaBytes(record[1], record[0]['contentType'], record[0]['fileName'], record[0]['contentType'].startswith('image/')) if match[2] else record[0]
        return None

    def media_route(self, method, match):
        record = self.server.fixture.media.get(match[2])
        if record is None:
            raise Problem(404, 'Media not found.')
        if (method == 'POST' and match[1] == 'media' and match[3] == '/delete') or (method == 'DELETE' and match[1] == 'attachments'):
            if self.server.fixture.find_issue(record['issueId'])['archivedAt']:
                raise Problem(409, 'The issue is archived.')
            changed = not record['deleted']
            record['deleted'] = True
            return 200, changed
        if method != 'GET' or record['deleted'] or (match[1] == 'media' and record['kind'] != 'image'):
            raise Problem(404, 'Media not found.')
        return 200, MediaBytes(record['bytes'], record['contentType'], record['fileName'], match[1] == 'media')

    def read_multipart(self, image):
        limit = 10 * 1024 * 1024 if image else 10_000_000
        try:
            size = int(self.headers.get('Content-Length', '0'))
        except ValueError as error:
            raise Problem(400, 'Invalid size.') from error
        if not 0 < size <= limit + 65536:
            raise Problem(413, 'Attachment too large.')
        mime = self.headers.get('Content-Type', '')
        message = BytesParser(policy=email_policy).parsebytes(f'Content-Type: {mime}\r\n\r\n'.encode() + self.rfile.read(size))
        parts = list(message.iter_parts())
        if len(parts) != 1 or parts[0].get_param('name', header='Content-Disposition') != 'file':
            raise Problem(400, 'Send one file field.')
        part = parts[0]
        data, name, mime = part.get_payload(decode=True), part.get_filename(), part.get_content_type()
        if not name or len(name) > 255 or any(c in name for c in '/\\\r\n\x00') or not data or len(data) > limit:
            raise Problem(400, 'Invalid file.')
        signatures = {'image/png': b'\x89PNG\r\n\x1a\n', 'image/jpeg': b'\xff\xd8\xff',
                      'image/gif': b'GIF8', 'image/webp': b'RIFF', 'application/pdf': b'%PDF-'}
        valid = data.startswith(signatures[mime]) if mime in signatures else False
        if not image and mime in ('text/plain', 'text/markdown'):
            try:
                valid = '\x00' not in data.decode('utf-8')
            except UnicodeError:
                valid = False
        if not image and mime in ('application/vnd.openxmlformats-officedocument.wordprocessingml.document',
                                 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
                                 'application/vnd.openxmlformats-officedocument.presentationml.presentation'):
            valid = data.startswith(b'PK\x03\x04')
        if not valid or (image and not mime.startswith('image/')):
            raise Problem(400, 'Unsupported file content.')
        if mime == 'image/png':
            if len(data) < 24:
                raise Problem(400, 'Invalid PNG header.')
            width, height = struct.unpack('!II', data[16:24])
            if width > 4096 or height > 4096 or width * height > 16_777_216:
                raise Problem(400, 'Image dimensions are too large.')
        return name, mime, data

    def read_json(self):
        try:
            size = int(self.headers.get('Content-Length', '0'))
            if not 0 < size <= (33398872 if self.path == '/api/v1/chat/uploads' else 300000):
                raise Problem(400, 'Invalid request body size.')
            return json.loads(self.rfile.read(size))
        except (ValueError, UnicodeError) as error:
            raise Problem(400, 'Invalid JSON body.') from error

    def respond(self, status, value):
        if isinstance(value, MediaBytes):
            self.send_response(status)
            self.send_header('Content-Type', value.mime)
            self.send_header('X-Content-Type-Options', 'nosniff')
            self.send_header('Content-Security-Policy', "default-src 'none'; sandbox")
            self.send_header('Content-Disposition', 'inline' if value.image else 'attachment')
            self.send_header('Cache-Control', 'private')
            self.send_header('Content-Length', str(len(value.data)))
            self.end_headers()
            self.wfile.write(value.data)
            return
        body = json.dumps(copy.deepcopy(value)).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/problem+json' if status >= 400 else 'application/json')
        self.send_header('X-Correlation-Id', 'fixture')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def failure_rule(value):
    prefix, _, status = value.partition(':')
    status = int(status or 413)
    if not prefix or status not in (409, 413, 500, 503):
        raise argparse.ArgumentTypeError('Use PREFIX or PREFIX:STATUS with 409, 413, 500 or 503.')
    return prefix, status


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--port', type=int, default=18161)
    parser.add_argument('--access-jwt', required=True, help='Dummy assertion required on every request; never printed.')
    parser.add_argument('--fail-chat-uploads', metavar='PREFIX[:STATUS]', type=failure_rule,
                        help='Refuse chat uploads whose file name starts with PREFIX (default status 413; 409 and 503 leave the upload unclear, so the app offers Check upload).')
    parser.add_argument('--no-updates-stream', action='store_true',
                        help='Answer the updates stream with 404, as an API without it does.')
    parser.add_argument('--change-after', type=float, metavar='SECONDS',
                        help='Retitle SUB-1 SECONDS after its first detail read.')
    parser.add_argument('--decision-scenario', choices=DECISION_SCENARIOS, default='normal',
                        help='Choose which fictional Decisions response to capture.')
    parser.add_argument('--fleet', action='store_true',
                        help='Add the fictional FLT, NAV, MAP, WARP and RELAY projects and their issues, for web Space evidence.')
    args = parser.parse_args()
    with FixtureServer(args.port, args.access_jwt, fail_chat_uploads=args.fail_chat_uploads,
                       updates_stream=not args.no_updates_stream, change_after=args.change_after,
                       decision_scenario=args.decision_scenario, fleet=args.fleet) as server:
        print(f'Subspace API fixture on {server.origin}', flush=True)
        print(f'SUBSPACE_API_URL={server.origin}/api', flush=True)
        server.serve_forever()


if __name__ == '__main__':
    main()
