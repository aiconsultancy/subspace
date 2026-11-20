#!/usr/bin/env python3
"""Loopback-only, memory-only NETT API fixture. Never calls upstream services.

Usage: python3 scripts/nett_api_test_server.py [--host 127.0.0.1] [--port 18171]
           [--access-token TOKEN] [--single-org] [--auto-approve]
           [--serve FEATURE ...] [--stream-seconds S] [--keepalive-seconds S]
           [--access-token-seconds S]

Port 0 picks a free port. The only line on stdout is
`NETT fixture listening on http://127.0.0.1:<port>`.

It serves:
- the NETT REST routes the desktop NETT backend uses, under /api/v1, each
  needing `Authorization: Bearer <token>` where the token is --access-token,
  a token registered through /__fixture/tokens, or an access token the fake
  authority issued (valid for --access-token-seconds). Each request acts as
  its token's person (Ada unless a registered token names Grace or Linus),
  and every organisation route needs exactly one x-org-id naming the route's
  organisation: missing or different is 403, malformed or repeated 400;
- a fake WorkOS device-authorisation authority at
  /user_management/authorize/device, /user_management/authenticate (device
  code and refresh token grants, form-encoded or JSON) and the /device
  approval page (--auto-approve approves a code on its second poll);
- credential-free private file delivery at /files/<id>?grant=... and upload
  transfer at /u/<id>?grant=..., both refusing any Authorization, Cookie or
  x-org-id header; a transfer takes the declared bytes exactly once;
- the capability list GET /api/v1/orgs/{orgId}/desktop/capabilities, naming
  each --serve feature the organisation is eligible for (empty by default),
  and the routes of each served feature, per the agreed desktop contract:
  initiativeChannels (paged initiative discovery with paths, stakeholders
  and fleets, and fleet reads), attachments (standalone issue attachments:
  list, prepare, PUT, complete, cancel and versioned delete, downloaded
  through POST /files/grants), chatUploads (prepare, PUT, complete, read by
  reference, grant and versioned delete), issueMoves (team move preview and
  the PATCH that confirms it with movePreviewToken) and updates (the issue
  update stream, GET /api/v1/updates?project=KEY with exactly one x-org-id:
  a `ready` event with null data, `issueChanged` events, a keep-alive
  comment every 15 s, and a close after 3 minutes, at the bearer's expiry, on
  queue overflow or when a hook closes it; before every flush it rechecks
  the opening bearer and the person's membership, and closes without
  sending anything queued once either is gone). A route whose feature is
  not served answers a neutral 404;
- unauthenticated test hooks under /__fixture/: touch, requests,
  requests/clear, updates, updates/close, capabilities, config (completion
  checks, stream refusals, failNext, slowBody, dropPutResponse, dropNext,
  mediaReadyAtOnce),
  tokens,
  tokens/revoke, members/revoke, members/restore, projects/hide,
  teams/archive, issues/archive and transfers/expire.

The data is invented. Ada Fixture is the caller; Grace Fixture and the former
member Linus Fixture share "Fixture Works" with her. "Second Fixture Org" has
its own OPS team, so OPS-1 names a different issue in each organisation.
Ada also has a personal organisation without issue tracking. --single-org
leaves out both the second and the personal organisation, so the caller has
exactly one accessible organisation.
"""

import argparse
import base64
import collections
import hashlib
import hmac
import html
import ipaddress
import json
import re
import secrets
import socket
import struct
import sys
import threading
import time
import uuid
import zlib
from datetime import date, datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, quote, unquote, urlsplit

DEFAULT_PORT = 18171
DEFAULT_ACCESS_TOKEN = 'nett-fixture-access'
DEVICE_GRANT = 'urn:ietf:params:oauth:grant-type:device_code'
DEVICE_EXPIRES_SECONDS = 300
DEVICE_INTERVAL_SECONDS = 1
POLL_TOLERANCE_SECONDS = 0.2
ACCESS_TOKEN_SECONDS = 3600
GRANT_SECONDS = 300
UPLOAD_SECONDS = 900
MAX_JSON_BYTES = 1_000_000
MAX_UPLOAD_BYTES = 25_000_000
MAX_LOGGED_REQUESTS = 10_000
STREAM_SECONDS = 180
STREAM_WRITE_SECONDS = 1
KEEPALIVE_SECONDS = 15
MAX_STREAMS_PER_USER = 16
MAX_STREAMS = 256
STREAM_QUEUE = 64
MAX_STREAM_PROJECTS = 64
CHAT_UPLOAD_DAYS = 30
MOVE_PLAN_SECONDS = 600
# The largest wait a client can be told: any whole number of seconds it can
# hold, up to the largest unsigned 64-bit value.
MAX_RETRY_AFTER = 2**64 - 1
# Routes NETT does not serve yet: the change it is to make for native clients.
NETT_GAPS = {('GET', '/api/v1/updates'), ('GET', '/api/v1/orgs/{orgId}/desktop/capabilities'),
             ('GET', '/api/v1/orgs/{orgId}/me/initiatives'), ('GET', '/api/v1/orgs/{orgId}/fleets/{fleetId}'),
             ('GET', '/api/v1/orgs/{orgId}/issues/{issueRef}/attachments'),
             ('POST', '/api/v1/orgs/{orgId}/issues/{issueRef}/attachments/prepare'),
             ('POST', '/api/v1/orgs/{orgId}/issues/{issueRef}/attachments/from-upload'),
             ('DELETE', '/api/v1/orgs/{orgId}/issues/{issueRef}/attachments/pending/{uploadId}'),
             ('DELETE', '/api/v1/orgs/{orgId}/issues/{issueRef}/attachments/{mediaId}'),
             ('POST', '/api/v1/orgs/{orgId}/chat/uploads/prepare'),
             ('POST', '/api/v1/orgs/{orgId}/chat/uploads/from-upload'),
             ('DELETE', '/api/v1/orgs/{orgId}/chat/uploads/pending/{uploadId}'),
             ('GET', '/api/v1/orgs/{orgId}/chat/uploads/{reference}'),
             ('DELETE', '/api/v1/orgs/{orgId}/chat/uploads/{reference}'),
             ('POST', '/api/v1/orgs/{orgId}/chat/uploads/{reference}/grants'),
             ('POST', '/api/v1/orgs/{orgId}/issues/{issueRef}/move-preview')}
# The coming desktop features the fixture implements, and what an organisation
# needs before NETT would advertise each: a business organisation, or the
# issue-tracking pack.
DESKTOP_FEATURES = ('initiativeChannels', 'attachments', 'chatUploads', 'issueMoves', 'updates')
FEATURE_NEEDS = dict(initiativeChannels='business', chatUploads='business', attachments='issues',
                     issueMoves='issues', updates='issues')
FILE_TYPES = ('text/markdown', 'text/plain', 'image/png', 'image/jpeg', 'image/gif', 'image/webp', 'application/zip',
              'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
              'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet')
TEAM_KEY = re.compile(r'[A-Za-z][A-Za-z0-9]{0,9}')
ACTOR_TYPES = ('human', 'agent', 'integration')
FIXTURE_FLEET = dict(slug='fixture-fleet', name='Fixture Fleet', ircHost='irc.example.test', ircPort=6697,
                     ircTls=True, ircWebsocketUrl=None, rearAdmiralName='Admiral Fixture',
                     captainNames=['Captain One', 'Captain Two'], vmHost=None)
USER_CODE_LETTERS = 'BCDFGHJKLMNPQRSTVWXZ'
STATE_TYPES = ('triage', 'backlog', 'unstarted', 'started', 'completed', 'canceled', 'duplicate')
STATE_NAMES = ('Triage', 'Backlog', 'Todo', 'In progress', 'Done', 'Cancelled', 'Duplicate')
TERMINAL = ('completed', 'canceled', 'duplicate')
ESTIMATES = {'notUsed': (), 'fibonacci': (0, 1, 2, 3, 5, 8, 13, 21), 'exponential': (0, 1, 2, 4, 8, 16, 32),
             'linear': (0, 1, 2, 3, 4, 5), 'tShirt': (0, 1, 2, 3, 5, 8)}
RELATION_KINDS = ('blocks', 'blockedBy', 'relatedTo', 'duplicateOf', 'duplicatedBy')
MEDIA_TARGETS = ('issueDescription', 'issueDescriptionDraft', 'issueComment', 'issueCommentDraft')
MEDIA_TYPES = ('image/png', 'image/jpeg', 'image/gif', 'image/webp', 'text/plain', 'text/markdown', 'application/pdf')
DECISION_STATUSES = ('proposed', 'accepted', 'rejected', 'superseded')
CURRENT_DECISIONS = ('proposed', 'accepted')
LABEL_USES = ('connections', 'issues', 'scheduling')
UUID_TEXT = re.compile(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}')
IDENTIFIER = re.compile(r'([A-Za-z][A-Za-z0-9]{0,9})-([1-9]\d{0,8})')
INSTANT = re.compile(r'(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?(Z|[+-]\d\d:\d\d)', re.IGNORECASE)
MEDIA_LINK = re.compile(r'!\[([^\]]*)\]\(nett-media:([0-9a-f-]{36})\)')
WORKOS_USER = dict(id='user_fixture_ada', email='ada@example.com', first_name='Ada', last_name='Fixture')
OWNER_PERMISSIONS = ('issues.view', 'issues.manage', 'decisions.view', 'decisions.manage',
                     'members.view', 'members.manage', 'projects.view', 'projects.manage')
MEMBER_PERMISSIONS = ('issues.view', 'issues.manage', 'decisions.view', 'members.view', 'projects.view')

CREATE_FIELDS = {'teamId', 'title', 'description', 'stateId', 'assigneeId', 'priority', 'estimate', 'labelIds',
                 'projectId', 'milestoneId', 'cycleId', 'dueDate', 'parentId', 'type', 'mediaDraftId'}
CREATE_DECLARED = {'eventId', 'relations', 'number', 'provenance', 'teamPosition', 'subIssuePosition'}
UPDATE_FIELDS = {'version', 'title', 'description', 'stateId', 'assigneeId', 'priority', 'estimate', 'labelIds',
                 'projectId', 'milestoneId', 'cycleId', 'dueDate', 'parentId', 'type'}
UPDATE_DECLARED = {'reviewContext', 'teamId', 'eventId', 'relations', 'archived', 'teamPosition', 'subIssuePosition'}

# A small Markdown attachment for the description of Fixture Works OPS-1.
FIXTURE_RUNBOOK = b"""# Backup runbook

Fictional notes for the fixture issue.

- Check the nightly job log.
- Restore one file to prove the backup.
- Write down how long the restore took.
"""


def uid(name):
    """A stable UUID for a fixture record, so reruns and tests agree."""
    return str(uuid.uuid5(uuid.NAMESPACE_URL, 'https://nett-fixture.example.test/' + name))


def utc_now():
    return datetime.now(timezone.utc)


def stamp(moment):
    return moment.astimezone(timezone.utc).isoformat(timespec='milliseconds').replace('+00:00', 'Z')


def parse_stamp(value):
    return datetime.fromisoformat(value.replace('Z', '+00:00'))


def b64url(data):
    return base64.urlsafe_b64encode(data.encode() if isinstance(data, str) else data).decode().rstrip('=')


def parse_uuid(value):
    """The canonical lower-case form of a UUID string, or None."""
    if not isinstance(value, str) or not UUID_TEXT.fullmatch(value.strip()):
        return None
    return str(uuid.UUID(value.strip()))


def parse_instant(name, value):
    """An ISO-8601 instant that carries an explicit offset; anything else is a 400."""
    match = INSTANT.fullmatch(value)
    if not match:
        raise Problem(400, f'{name} must be an ISO-8601 instant with an explicit offset.')
    fraction = (match[2] or '')[:6]
    offset = '+00:00' if match[3].upper() == 'Z' else match[3]
    try:
        return datetime.fromisoformat(match[1] + ('.' + fraction.ljust(6, '0') if fraction else '') + offset)
    except ValueError as error:
        raise Problem(400, f'{name} is not a valid instant.') from error


def fixture_png(width, height, warm=False):
    """A small valid PNG: a gradient, so a preview shows real pixels."""
    def chunk(kind, data):
        return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            red, blue = 40 + 180 * x // width, 200 - 150 * x // width
            rows += bytes((blue, 90 + 2 * y % 120, red) if warm else (red, 90 + 2 * y % 120, blue))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(bytes(rows))) + chunk(b'IEND', b''))


def account(key, first, last):
    return dict(userId=uid('user/' + key), firstName=first, lastName=last, email=f'{key}@example.com')


ADA = account('ada', 'Ada', 'Fixture')
GRACE = account('grace', 'Grace', 'Fixture')
LINUS = account('linus', 'Linus', 'Fixture')
PEOPLE = {person['userId']: person for person in (ADA, GRACE, LINUS)}
# The people a registered token can act as, by key.
PEOPLE_BY_KEY = dict(ada=ADA, grace=GRACE, linus=LINUS)
FORMER = {LINUS['userId']}
CALLER = ADA['userId']


def display(user_id):
    person = PEOPLE[user_id]
    return f"{person['firstName']} {person['lastName']}"


def person_dto(user_id):
    """An IssuePersonDto, or None for nobody."""
    if user_id is None:
        return None
    return dict(userId=user_id, displayName=display(user_id), isFormerMember=user_id in FORMER)


def actor(user_id):
    return dict(type='human', userId=user_id, person=person_dto(user_id), label=None)


class Problem(Exception):
    """An RFC 9457 problem, sent as application/problem+json."""

    def __init__(self, status, detail, headers=(), **extensions):
        super().__init__(detail)
        self.status, self.detail, self.headers, self.extensions = status, detail, tuple(headers), extensions


class Raw:
    """A non-JSON answer: an HTML page, file bytes or an empty 204."""

    def __init__(self, body=b'', content_type=None, headers=()):
        self.body, self.content_type, self.headers = body, content_type, tuple(headers)


class Accepted:
    """A JSON answer with a whole-second Retry-After, such as a 202 while checking."""

    def __init__(self, value, retry_after):
        self.value, self.retry_after = value, retry_after


class Subscriber:
    """One update stream: its organisation, team keys, the bearer's expiry
    (epoch seconds, or None for the static token) and a bounded queue. Like
    NETT, it closes the stream when the queue overflows instead of dropping
    hints, and closes before sending anything queued."""

    def __init__(self, org_id, keys, user_id, expires=None, token=None):
        self.org_id, self.keys, self.user_id, self.expires = org_id, keys, user_id, expires
        self.token = token
        self.queue = collections.deque()
        self.ready = threading.Condition()
        self.delivery = threading.Lock()
        self.closed = None
        self.abort = False

    def push(self, event):
        with self.ready:
            if self.closed:
                return
            if len(self.queue) >= STREAM_QUEUE:
                self.closed = 'overflow'
            else:
                self.queue.append(event)
            self.ready.notify()

    def close(self, reason, abort=False):
        with self.ready:
            self.closed = self.closed or reason
            self.abort = self.abort or abort
            self.ready.notify()


class Drop:
    """No answer at all: the connection closes, as when an acknowledgement is lost."""


class Stream:
    """An answer that streams server-sent events until it closes."""

    def __init__(self, subscriber):
        self.subscriber = subscriber


def closed_body(payload, supported, declared, schema):
    """Refuses a body that is not an object or has a property the schema does
    not declare. Declared properties the fixture does not model must be null."""
    if not isinstance(payload, dict):
        raise Problem(400, f'{schema} must be a JSON object.')
    unknown = sorted(set(payload) - supported - declared)
    if unknown:
        raise Problem(400, f'{schema} has no property {unknown[0]}.')
    unsupported = sorted(name for name in set(payload) & declared if payload[name] is not None)
    if unsupported:
        raise Problem(400, f'The fixture does not support {unsupported[0]}.')


def is_integer(value):
    return isinstance(value, int) and not isinstance(value, bool)


def is_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def fingerprint(scope, values):
    """Binds a cursor to its operation, filters and order (never its limit)."""
    canonical = sorted((name, sorted(items)) for name, items in values.items()
                       if name not in ('cursor', 'limit', 'around'))
    return hashlib.sha256(json.dumps([scope, canonical]).encode()).hexdigest()[:16]


def make_cursor(scope, values, offset):
    return b64url(json.dumps([fingerprint(scope, values), offset], separators=(',', ':')))


def read_cursor(query, scope, total):
    value = query.one('cursor')
    if value is None:
        return 0
    try:
        bound, offset = json.loads(base64.urlsafe_b64decode(value + '=' * (-len(value) % 4)))
        if bound != fingerprint(scope, query.values) or not is_integer(offset) or not 0 <= offset <= total:
            raise ValueError('foreign cursor')
    except (ValueError, TypeError, UnicodeError) as error:
        raise Problem(400, 'The cursor does not belong to this list, its filters and its order.') from error
    return offset


def page(ctx, rows, scope, default=50, maximum=100):
    """One page of `rows` and the cursor for the next, if any."""
    limit = ctx.query.integer('limit', default, 1, maximum)
    start = read_cursor(ctx.query, scope, len(rows))
    end = start + limit
    return rows[start:end], (make_cursor(scope, ctx.query.values, end) if end < len(rows) else None)


class Query:
    """Decodes query parameters and validates their names and values."""

    def __init__(self, raw):
        self.values = parse_qs(raw, keep_blank_values=True)

    def all(self, name):
        return self.values.get(name, [])

    def one(self, name, default=None):
        values = self.all(name)
        if len(values) > 1:
            raise Problem(400, f'{name} may be given once.')
        return values[0] if values else default

    def integer(self, name, default, low, high):
        value = self.one(name)
        if value is None:
            return default
        if not re.fullmatch(r'-?\d{1,9}', value):
            raise Problem(400, f'{name} must be an integer.')
        if not low <= int(value) <= high:
            raise Problem(400, f'{name} must be between {low} and {high}.')
        return int(value)

    def flag(self, name, default=False):
        value = self.one(name)
        if value is None:
            return default
        if value.lower() not in ('true', 'false'):
            raise Problem(400, f'{name} must be true or false.')
        return value.lower() == 'true'

    def uuids(self, name):
        values = set()
        for value in self.all(name):
            parsed = parse_uuid(value)
            if parsed is None:
                raise Problem(400, f'{name} must be a UUID.')
            values.add(parsed)
        return values

    def check_names(self, allowed):
        unknown = sorted(self.values.keys() - allowed)
        if unknown:
            raise Problem(400, f'The fixture does not support the query parameter {unknown[0]}.')


class Fixture:
    """The in-memory NETT data. Callers hold the server lock."""

    def __init__(self, single_org=False):
        self.origin = 'http://127.0.0.1'
        started = utc_now()
        self.started = started.replace(microsecond=started.microsecond // 1000 * 1000)
        self.last_instant = self.started
        self.orgs, self.teams, self.states, self.labels = {}, {}, {}, {}
        self.projects, self.initiatives, self.milestones, self.cycles = {}, {}, {}, {}
        self.issues, self.comments, self.events, self.pull_requests = {}, {}, {}, {}
        self.issue_numbers = collections.defaultdict(int)
        self.media, self.files, self.grants = {}, {}, {}
        self.uploads, self.upload_keys = {}, {}
        # Standalone attachments and chat uploads: prepared transfers, then records.
        self.transfers, self.transfer_keys, self.spent_keys = {}, {}, set()
        self.attachments, self.chat_uploads = {}, {}
        # The person making the current request, set per request under the
        # server lock, and planning projects hidden from one person.
        self.caller = CALLER
        self.hidden_projects = set()
        self.move_plans = {}
        self.decisions, self.lists, self.list_items = {}, {}, {}
        self.on_change = None
        self.seed(single_org)

    # Clock

    def now(self):
        """A UTC instant later than any this fixture has handed out."""
        moment = utc_now()
        moment = moment.replace(microsecond=moment.microsecond // 1000 * 1000)
        if moment <= self.last_instant:
            moment = self.last_instant + timedelta(milliseconds=1)
        self.last_instant = moment
        return stamp(moment)

    def ago(self, **delta):
        return stamp(self.started - timedelta(**delta))

    def day(self, offset):
        return (self.started.date() + timedelta(days=offset)).isoformat()

    # Seed

    def seed(self, single_org):
        works = self.add_org('works', 'Fixture Works', 'business', ['issue-tracking'],
                             [(ADA, 'owner', 'active'), (GRACE, 'admin', 'active'), (LINUS, 'member', 'removed')])
        ops = self.add_team(works, 'OPS', 'Operations', 'fibonacci', True, self.ago(days=300))
        web = self.add_team(works, 'WEB', 'Web', 'notUsed', False, self.ago(days=290))
        bug = self.add_label(works, 'bug', '#D73A4A', ['issues'])
        docs = self.add_label(works, 'docs', '#0075CA', ['issues'])
        self.add_label(works, 'legacy', '#BFBFBF', ['issues'], state='archived')
        self.add_label(works, 'key client', '#5319E7', ['connections'])
        platform = self.add_initiative(works, 'Platform', 'Keep the platform dependable.', self.day(60))
        self.add_initiative(works, 'Empty', 'An initiative with no projects yet.', None)
        self.add_initiative(works, 'Desktop', 'The desktop client, under Platform.', None, parent=platform)
        self.add_initiative(works, 'Research', 'Exploratory work with no fleet.', None, fleet=False)
        retired = self.add_initiative(works, 'Retired', 'An archived initiative.', None, archived=True)
        self.add_initiative(works, 'Legacy', 'A child of an archived initiative.', None, parent=retired)
        launch = self.add_project(works, 'Launch', platform, 'started', 'onTrack', self.day(-30), self.day(30))
        hardening = self.add_project(works, 'Hardening', None, 'planned', None, None, self.day(90))
        self.add_milestone(launch, 'Alpha', self.day(-20), 1, 'done', self.ago(days=21))
        beta = self.add_milestone(launch, 'Beta', self.day(14), 2, 'next', None)
        current = self.add_cycle(ops, 3, 'current', self.day(-6), self.day(7))
        upcoming = self.add_cycle(ops, 4, 'upcoming', self.day(8), self.day(21))

        ops1 = self.add_issue(works, ops, 1, 'Restore the nightly backup check', 'started', self.ago(days=10),
                              assignee=ADA, priority=2, labels=[bug], project=launch, milestone=beta,
                              cycle=current, estimate=3, due=self.day(5))
        ops2 = self.add_issue(works, ops, 2, 'Harden the deployment pipeline', 'started', self.ago(days=9),
                              assignee=GRACE, priority=3, project=hardening, kind='tracker')
        self.add_issue(works, ops, 3, 'Rotate the staging certificates', 'completed', self.ago(days=8),
                       assignee=GRACE, priority=3, parent=ops2, estimate=2, cycle=current)
        self.add_issue(works, ops, 4, 'Document the rollback steps', 'unstarted', self.ago(days=7),
                       labels=[docs], priority=4, parent=ops2, cycle=upcoming)
        self.add_issue(works, ops, 5, 'Triage the alert noise', 'triage', self.ago(days=2), creator=GRACE)
        web1 = self.add_issue(works, web, 1, 'Fix the sign-in redirect loop', 'started', self.ago(days=6),
                              assignee=ADA, priority=1, labels=[bug], project=launch)
        self.add_issue(works, web, 2, 'Refresh the landing page copy', 'backlog', self.ago(days=5),
                       assignee=LINUS, labels=[docs])
        web3 = self.add_issue(works, web, 3, 'Retire the legacy theme', 'canceled', self.ago(days=20),
                              assignee=GRACE, priority=4, project=hardening)
        web3.update(archivedAt=self.ago(days=3), archivedById=GRACE['userId'], updatedAt=self.ago(days=3), version=2)
        self.touch(web3, self.ago(days=3), GRACE['userId'], 'issue.changed', dict(changed=['archived']))
        ops1['relations'].append(('relatedTo', web1['id']))
        web1['relations'].append(('relatedTo', ops1['id']))
        self.seed_discussion(works, ops1)
        self.seed_decisions(works, ops1, launch, hardening)
        self.seed_checklist(ops1)
        self.seed_attachments(ops1)

        if not single_org:
            second = self.add_org('second', 'Second Fixture Org', 'business', ['issue-tracking'],
                                  [(ADA, 'member', 'active'), (GRACE, 'owner', 'active')])
            second_ops = self.add_team(second, 'OPS', 'Operations', 'linear', False, self.ago(days=200))
            self.add_issue(second, second_ops, 1, 'Plan the second office move', 'unstarted', self.ago(days=4),
                           assignee=ADA, priority=3)
            self.add_initiative(second, 'Office', 'The second office move.', None)
            self.add_org('personal', 'Ada Fixture', 'personal', [], [(ADA, 'owner', 'active')], personal=True)
        # Ada is not a member, so every route treats it as unknown.
        self.add_org('outside', 'Outside Fixture Org', 'business', ['issue-tracking'], [(GRACE, 'owner', 'active')])

    def seed_discussion(self, org, row):
        image = self.add_media(org['id'], 'backup-dashboard.png', 'image/png', fixture_png(64, 32),
                               uid('media/backup-dashboard'), issue_id=row['id'], created=row['createdAt'])
        notes = self.add_media(org['id'], 'backup-runbook.md', 'text/markdown', FIXTURE_RUNBOOK,
                               uid('media/backup-runbook'), issue_id=row['id'], created=row['createdAt'])
        row['mediaIds'] += [image['id'], notes['id']]
        row['description'] = ('## Nightly backup check\n\nThe nightly backup check stopped reporting on Tuesday.\n\n'
                              f'![Backup dashboard](nett-media:{image["id"]})\n\n'
                              f'Read the [runbook](nett-media:{notes["id"]}) before restoring anything.\n')
        first = self.add_comment(row, GRACE['userId'], 'The backup job failed twice this week.', self.ago(days=3),
                                 comment_id=uid('comment/ops-1/first'))
        self.add_comment(row, ADA['userId'], 'I can reproduce it on the staging copy.', self.ago(days=2),
                         parent=first, comment_id=uid('comment/ops-1/first-reply-1'))
        self.add_comment(row, GRACE['userId'], 'Thanks. The job log is attached to the runbook.', self.ago(days=1),
                         parent=first, comment_id=uid('comment/ops-1/first-reply-2'))
        timing = self.add_media(org['id'], 'restore-timing.png', 'image/png', fixture_png(48, 48, warm=True),
                                uid('media/restore-timing'), issue_id=row['id'], created=self.ago(hours=1))
        second = self.add_comment(row, ADA['userId'],
                                  f'Restore timing from last night:\n\n![Restore timing](nett-media:{timing["id"]})',
                                  self.ago(hours=1), comment_id=uid('comment/ops-1/second'), media=[timing])
        timing['commentId'] = second['id']
        observed = self.ago(hours=2)
        self.pull_requests[row['id']] = [dict(
            id=uid('pull-request/ops-1'), repository=dict(id='fixture/workspace', name='workspace',
                                                          url='https://example.com/fixture/workspace'),
            number=42, url='https://example.com/fixture/workspace/pull/42', title='Check the backup job result',
            authorLogin='ada-fixture', state='open', isDraft=False, targetBranch='main', referenceKind='closes',
            referenceActive=True, lastObservedAt=observed, staleReason=None, completionDisposition=None)]

    def seed_decisions(self, org, row, launch, hardening):
        old = self.add_decision(org, 'Keep nightly backups for 30 days', 'superseded', -9, launch, [row])
        new = self.add_decision(org, 'Keep nightly backups for 90 days', 'accepted', -5, launch, [row])
        self.add_decision(org, 'Use one release branch', 'proposed', -2, hardening, [])
        old.update(supersededById=new['id'], version=2, revisionNumber=2)
        new['supersedesId'] = old['id']
        for decision in (old, new):
            at = stamp(datetime.fromisoformat(decision['decidedOn']).replace(hour=9, tzinfo=timezone.utc))
            self.record(row, 'decision.logged', at, ADA['userId'], dict(decisionId=decision['id']))

    def seed_attachments(self, row):
        """Two standalone attachments on OPS-1, a file and an image, older first."""
        self.add_attachment(row, 'restore-log.txt', 'text/plain', b'Restore finished in 4 minutes.\n',
                            uid('attachment/ops-1/restore-log'), self.ago(days=4))
        self.add_attachment(row, 'restore-chart.png', 'image/png', fixture_png(24, 12),
                            uid('attachment/ops-1/restore-chart'), self.ago(days=3))

    def seed_checklist(self, row):
        checklist = dict(id=uid('list/ops-1/checklist'), issueId=row['id'], name='Checklist', version=1,
                         removed=False, createdAt=row['createdAt'])
        self.lists[checklist['id']] = checklist
        for index, text in enumerate(('Check the nightly job log', 'Restore one file from last night',
                                      'Write down the restore time')):
            item = dict(id=uid(f'list/ops-1/checklist/{index}'), listId=checklist['id'], issueId=row['id'],
                        version=1, text=text, completed=index == 0, removed=False,
                        completedById=GRACE['userId'] if index == 0 else None,
                        completedAt=self.ago(days=2) if index == 0 else None)
            self.list_items[item['id']] = item

    def add_org(self, key, name, org_type, packs, members, personal=False):
        created = self.ago(days=400)
        org = dict(id=uid('org/' + key), key=key, name=name, handle=name.lower().replace(' ', '-'),
                   orgType=org_type, isPersonalOrg=personal, featurePacks=list(packs), createdAt=created,
                   members=[dict(userId=person['userId'], role=role, status=status, joinedAt=created)
                            for person, role, status in members])
        self.orgs[org['id']] = org
        return org

    def add_team(self, org, key, name, scale, cycles, created):
        team = dict(id=uid(f"{org['key']}/team/{key}"), orgId=org['id'], key=key, name=name,
                    estimateScale=scale, hasCycles=cycles, createdAt=created, stateIds=[])
        for position, (kind, label) in enumerate(zip(STATE_TYPES, STATE_NAMES)):
            state = dict(id=uid(f"{org['key']}/team/{key}/state/{kind}"), teamId=team['id'], name=label,
                         type=kind, position=position, isDefault=kind == 'backlog')
            self.states[state['id']] = state
            team['stateIds'].append(state['id'])
        self.teams[team['id']] = team
        return team

    def add_label(self, org, name, color, uses, state='active'):
        label = dict(id=uid(f"{org['key']}/label/{name}"), orgId=org['id'], name=name, color=color,
                     parentId=None, usedFor=list(uses), state=state, createdAt=self.ago(days=100),
                     updatedAt=self.ago(days=100))
        self.labels[label['id']] = label
        return label

    def add_initiative(self, org, name, description, target, parent=None, fleet=True, archived=False):
        initiative = dict(id=uid(f"{org['key']}/initiative/{name}"), orgId=org['id'], name=name,
                          description=description, targetDate=target, status='active',
                          parentId=parent and parent['id'], fleet=fleet, archived=archived,
                          createdAt=self.ago(days=60), updatedAt=self.ago(days=30))
        self.initiatives[initiative['id']] = initiative
        return initiative

    def add_attachment(self, row, file_name, content_type, data, media_id, created, upload_id=None):
        """A standalone attachment; `upload_id` names the transfer it was completed
        from, and stays None for a file that was never uploaded."""
        public = 'pf_' + hashlib.sha256(('attachment/' + media_id).encode()).hexdigest()[:24]
        record = dict(mediaId=media_id, orgId=row['orgId'], issueId=row['id'], version=1,
                      kind='image' if content_type.startswith('image/') else 'file', fileName=file_name,
                      contentType=content_type, sizeBytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                      publicFileId=public, expiresAt=None, createdAt=created, uploadId=upload_id)
        self.attachments[media_id] = record
        self.files[public] = dict(data=data, contentType=content_type, orgId=row['orgId'], disposition='attachment',
                                  issueId=row['id'])
        return record

    def add_project(self, org, name, initiative, status, health, start, target):
        project = dict(id=uid(f"{org['key']}/project/{name}"), orgId=org['id'], name=name, leadId=ADA['userId'],
                       status=status, priority=2, startDate=start, targetDate=target, archived=False,
                       health=health, initiativeId=initiative and initiative['id'],
                       createdAt=self.ago(days=45), updatedAt=self.ago(days=4))
        self.projects[project['id']] = project
        return project

    def add_milestone(self, project, name, target, order, status, completed):
        milestone = dict(id=uid(f"{project['id']}/milestone/{name}"), orgId=project['orgId'],
                         projectId=project['id'], name=name, description=f'Fixture milestone {name}.',
                         targetDate=target, sortOrder=order, status=status, completedAt=completed,
                         createdAt=self.ago(days=40), updatedAt=completed or self.ago(days=40))
        self.milestones[milestone['id']] = milestone
        return milestone

    def add_cycle(self, team, number, state, starts, ends):
        cycle = dict(id=uid(f"{team['id']}/cycle/{number}"), teamId=team['id'], teamKey=team['key'],
                     number=number, startsOn=starts, endsOn=ends, state=state)
        self.cycles[cycle['id']] = cycle
        return cycle

    def add_issue(self, org, team, number, title, state_type, created, *, assignee=None, priority=0, labels=(),
                  project=None, milestone=None, cycle=None, estimate=None, due=None, parent=None, kind='issue',
                  creator=ADA, issue_id=None):
        self.issue_numbers[team['id']] = max(self.issue_numbers[team['id']], number)
        state = self.state_of(team, state_type)
        row = dict(id=issue_id or uid(f"{org['key']}/issue/{team['key']}-{number}"), orgId=org['id'],
                   teamId=team['id'], teamKey=team['key'], number=number, identifier=f"{team['key']}-{number}",
                   type=kind, title=title, stateId=state, assigneeId=assignee and assignee['userId'],
                   priority=priority, estimate=estimate, cycleId=cycle and cycle['id'], dueDate=due,
                   labelIds=[label['id'] for label in labels], projectId=project and project['id'],
                   milestoneId=milestone and milestone['id'], parentId=parent and parent['id'], blocked=False,
                   archivedAt=None, archivedById=None, createdAt=created, updatedAt=created, version=1,
                   lastActivityAt=created, lastActivity=None, createdById=creator['userId'], description=None,
                   mediaIds=[], relations=[], startedAt=None, completedAt=None, canceledAt=None)
        self.mark_state(row, state, created)
        self.issues[row['id']] = row
        self.events[row['id']] = []
        self.record(row, 'issue.created', created, creator['userId'])
        return row

    def next_issue_number(self, team):
        self.issue_numbers[team['id']] += 1
        return self.issue_numbers[team['id']]

    def add_comment(self, row, author_id, body, created, parent=None, comment_id=None, media=(), resolved=False):
        comment = dict(id=comment_id or str(uuid.uuid4()), issueId=row['id'], parentCommentId=parent and parent['id'],
                       body=body, version=1, authorId=author_id, createdAt=created, editedAt=None, deletedAt=None,
                       resolved=resolved, resolvedById=author_id if resolved else None,
                       resolvedAt=created if resolved else None, mediaIds=[m['id'] for m in media])
        self.comments[comment['id']] = comment
        self.touch(row, created, author_id, 'comment.created',
                   dict(commentId=comment['id'], parentCommentId=comment['parentCommentId']), comment)
        return comment

    def add_media(self, org_id, file_name, content_type, data, media_id, issue_id=None, created=None, version=1):
        public = 'pf_' + hashlib.sha256(('public-file/' + media_id).encode()).hexdigest()[:24]
        record = dict(id=media_id, orgId=org_id, issueId=issue_id, commentId=None, draftId=None,
                      kind='image' if content_type.startswith('image/') else 'file', fileName=file_name,
                      contentType=content_type, sizeBytes=len(data), version=version, status='ready',
                      publicFileId=public, createdAt=created, attached=issue_id is not None)
        self.media[media_id] = record
        self.files[public] = dict(data=data, contentType=content_type, orgId=org_id)
        return record

    def add_decision(self, org, title, status, decided, project, issues):
        decided_on = self.day(decided)
        at = stamp(datetime.fromisoformat(decided_on).replace(hour=9, tzinfo=timezone.utc))
        decision = dict(id=uid(f"{org['key']}/decision/{title}"), orgId=org['id'], title=title, status=status,
                        decidedOn=decided_on, deciderIds=[ADA['userId'], GRACE['userId']],
                        projectId=project and project['id'], supersedesId=None, supersededById=None,
                        issueIds=[row['id'] for row in issues], version=1, revisionNumber=1,
                        decision=f'{title}.', context='The fixture team reviewed its backup routine.',
                        why='Restores must cover the period the team needs.', implications='Storage use grows.',
                        createdById=ADA['userId'], createdAt=at, updatedAt=at,
                        acceptedAt=at if status in ('accepted', 'superseded') else None,
                        acceptedById=ADA['userId'] if status in ('accepted', 'superseded') else None)
        self.decisions[decision['id']] = decision
        return decision

    # Writes shared by routes and seeds

    def state_of(self, team, state_type):
        return next(i for i in team['stateIds'] if self.states[i]['type'] == state_type)

    def mark_state(self, row, state_id, at):
        kind = self.states[state_id]['type']
        if kind == 'started' and row['startedAt'] is None:
            row['startedAt'] = at
        row['completedAt'] = at if kind == 'completed' else None
        row['canceledAt'] = at if kind == 'canceled' else None
        row['stateId'] = state_id

    def record(self, row, kind, at, user_id, data=None, comment=None, hint=None):
        """Appends an activity entry and tells live streams. `hint` adjusts the
        stream event: actorType, audit=False to omit auditId, and also, a
        further team key to hint in."""
        entries = self.events[row['id']]
        entries.append(dict(id=uid(f"activity/{row['id']}/{len(entries)}"), issueId=row['id'],
                            changeId=uid(f"change/{row['id']}/{len(entries)}"), kind=kind, dataVersion=1,
                            occurredAt=at, displayAt=at, actor=actor(user_id), data=data or {},
                            commentId=comment and comment['id']))
        if self.on_change:
            self.on_change(row, kind, at, entries[-1]['id'], hint or {})

    def touch(self, row, at, user_id, kind, data=None, comment=None, hint=None):
        """Records activity that moves the issue's lastActivityAt but not its version."""
        self.record(row, kind, at, user_id, data, comment, hint)
        if at >= row['lastActivityAt']:
            row['lastActivityAt'] = at
            row['lastActivity'] = dict(kind=kind, occurredAt=at, actor=actor(user_id))

    def grant(self, public_id, expires=None):
        token = secrets.token_urlsafe(24)
        expires = expires or utc_now() + timedelta(seconds=GRANT_SECONDS)
        self.grants[token] = (public_id, expires)
        return dict(publicFileId=public_id, url=f'{self.origin}/files/{quote(public_id)}?grant={token}',
                    expiresAtUtc=stamp(expires))

    # Lookups

    def is_member(self, org, user_id):
        return any(m['userId'] == user_id and m['status'] == 'active' for m in org['members'])

    def membership(self, org, user_id):
        return next(m for m in org['members'] if m['userId'] == user_id)

    def has_record(self, org, user_id):
        """Whether `user_id` was ever in `org`, active, former or revoked."""
        return any(m['userId'] == user_id for m in org['members'])

    def visible_project(self, project):
        return (project['orgId'], project['id'], self.caller) not in self.hidden_projects

    def accessible(self):
        return [org for org in self.orgs.values() if self.is_member(org, self.caller)]

    def org_issues(self, org):
        return [row for row in self.issues.values() if row['orgId'] == org['id']]

    def find_issue(self, org, ref):
        """An issue in `org` by UUID or case-insensitive identifier, or None."""
        if not isinstance(ref, str):
            return None
        issue_id = parse_uuid(ref)
        if issue_id:
            row = self.issues.get(issue_id)
            return row if row and row['orgId'] == org['id'] else None
        if not IDENTIFIER.fullmatch(ref.strip()):
            return None
        return next((row for row in self.org_issues(org) if row['identifier'] == ref.strip().upper()), None)

    def find_team(self, org, ref):
        team_id = parse_uuid(ref)
        return next((team for team in self.teams.values() if team['orgId'] == org['id']
                     and (team['id'] == team_id or team['key'] == ref.strip().upper())), None)

    def in_org(self, table, org, value):
        record = table.get(parse_uuid(value) or '')
        return record if record and record['orgId'] == org['id'] else None

    def effective_project(self, row):
        if row['projectId']:
            return row['projectId']
        parent = self.issues.get(row['parentId']) if row['parentId'] else None
        return parent['projectId'] if parent else None

    def children(self, row):
        return sorted((r for r in self.issues.values() if r['parentId'] == row['id']), key=lambda r: r['number'])

    def state_type(self, row):
        return self.states[row['stateId']]['type']

    def comment_count(self, row):
        return sum(1 for c in self.comments.values() if c['issueId'] == row['id'] and not c['deletedAt'])

    # Shapes

    def org_dto(self, org):
        me = self.membership(org, self.caller)
        permissions = OWNER_PERMISSIONS if me['role'] in ('owner', 'admin') else MEMBER_PERMISSIONS
        return dict(id=org['id'], name=org['name'], handle=org['handle'], description=None,
                    orgType=org['orgType'], role=me['role'], joinedAt=me['joinedAt'], createdAt=org['createdAt'],
                    updatedAt=org['createdAt'], version=1, isPersonalOrg=org['isPersonalOrg'],
                    permissions=list(permissions if not org['isPersonalOrg'] else ('members.view',)),
                    featurePacks=list(org['featurePacks']))

    def state_dto(self, state):
        count = sum(1 for r in self.issues.values() if r['stateId'] == state['id'] and not r['archivedAt'])
        return dict(state, issueCount=count)

    def team_dto(self, team):
        open_count = sum(1 for r in self.issues.values() if r['teamId'] == team['id'] and not r['archivedAt']
                         and self.state_type(r) not in TERMINAL)
        return dict(id=team['id'], name=team['name'], key=team['key'], defaultStateId=self.state_of(team, 'backlog'),
                    archivedAt=team.get('archivedAt'), createdByUserId=ADA['userId'], createdByName=display(ADA['userId']),
                    createdAt=team['createdAt'], updatedAt=team['createdAt'], version=1, openIssueCount=open_count,
                    states=[self.state_dto(self.states[i]) for i in team['stateIds']],
                    cycles=dict(enabled=team['hasCycles'], lengthWeeks=2, startDay='monday', cyclesAhead=1,
                                carryOver=True),
                    estimateScale=team['estimateScale'], hasCycles=team['hasCycles'])

    def label_summary(self, label_id):
        label = self.labels[label_id]
        return dict(id=label['id'], name=label['name'], color=label['color'], state=label['state'])

    def state_summary(self, state_id):
        state = self.states[state_id]
        return dict(id=state['id'], name=state['name'], type=state['type'])

    def progress(self, row):
        if row['type'] != 'tracker':
            return None
        children = [r for r in self.children(row) if not r['archivedAt']]
        types = [self.state_type(r) for r in children]
        return dict(done=types.count('completed'), total=len(types), canceled=types.count('canceled'),
                    duplicate=types.count('duplicate'),
                    sample=[dict(id=r['id'], stateType=t) for r, t in zip(children[:20], types)],
                    hasMore=len(children) > 20)

    def pull_request_summary(self, row):
        active = [pr for pr in self.pull_requests.get(row['id'], []) if pr['referenceActive']]
        if not active:
            return None
        counts = {state: sum(pr['state'] == state for pr in active) for state in ('open', 'closed', 'merged')}
        return dict(total=len(active), **counts, completionNeedsReview=0,
                    closingPending=sum(pr['state'] == 'open' and pr['referenceKind'] == 'closes' for pr in active),
                    lastObservedAt=max(pr['lastObservedAt'] for pr in active))

    def fields(self, row):
        """The IssueFields every issue shape shares."""
        cycle = self.cycles.get(row['cycleId'])
        return dict(id=row['id'], identifier=row['identifier'], teamId=row['teamId'], teamKey=row['teamKey'],
                    type=row['type'], title=row['title'], state=self.state_summary(row['stateId']),
                    assignee=person_dto(row['assigneeId']), priority=row['priority'], estimate=row['estimate'],
                    cycle={key: cycle[key] for key in ('id', 'number', 'state')} if cycle else None,
                    dueDate=row['dueDate'], labels=[self.label_summary(i) for i in row['labelIds']],
                    projectId=row['projectId'], milestoneId=row['milestoneId'],
                    effectiveProjectId=self.effective_project(row), parentId=row['parentId'],
                    blocked=row['blocked'], archivedAt=row['archivedAt'], createdAt=row['createdAt'],
                    updatedAt=row['updatedAt'], version=row['version'], progress=self.progress(row),
                    commentCount=self.comment_count(row), pullRequestSummary=self.pull_request_summary(row))

    def list_item(self, row):
        """An IssueListItem: the fields plus project, milestone and sub-issue summaries."""
        project = self.projects.get(self.effective_project(row))
        milestone = self.milestones.get(row['milestoneId'])
        progress = self.progress(row)
        return dict(self.fields(row),
                    project=dict(id=project['id'], name=project['name']) if project else None,
                    milestone={key: milestone[key] for key in ('id', 'name', 'targetDate')} if milestone else None,
                    subIssues=dict(done=progress['done'], total=progress['total']) if progress else None)

    def activity_item(self, row):
        """An ActivityIssueListItem; lastActivity stays null while creation is the latest activity."""
        return dict(self.list_item(row), lastActivityAt=row['lastActivityAt'], lastActivity=row['lastActivity'])

    def relation_items(self, row, include_archived=False):
        return [dict(self.list_item(self.issues[other]), kind=kind) for kind, other in row['relations']
                if include_archived or not self.issues[other]['archivedAt']]

    def reference(self, row):
        if row is None:
            return None
        return dict(id=row['id'], identifier=row['identifier'], title=row['title'], type=row['type'],
                    state=self.state_summary(row['stateId']), archived=row['archivedAt'] is not None)

    def detail(self, row):
        """An IssueDetail. It has no lastActivityAt: only list items carry it."""
        project = self.projects.get(self.effective_project(row))
        milestone = self.milestones.get(row['milestoneId'])
        children = [self.list_item(r) for r in self.children(row) if not r['archivedAt']]
        subscribed = self.caller in (row['assigneeId'], row['createdById'])
        return dict(self.fields(row), description=row['description'],
                    media=[self.descriptor(self.media[m]) for m in row['mediaIds']], mentions=[],
                    createdBy=person_dto(row['createdById']), startedAt=row['startedAt'],
                    completedAt=row['completedAt'], canceledAt=row['canceledAt'],
                    archivedBy=person_dto(row['archivedById']) if row['archivedAt'] else None,
                    parent=self.reference(self.issues.get(row['parentId'])), duplicateOf=None,
                    project=dict(id=project['id'], name=project['name']) if project else None,
                    milestone=dict(id=milestone['id'], name=milestone['name']) if milestone else None,
                    children=dict(items=children, nextCursor=None),
                    relations=dict(items=self.relation_items(row), nextCursor=None),
                    subscription=dict(subscribed=subscribed, explicitChoice=False))

    def descriptor(self, media):
        return dict(mediaId=media['id'], version=media['version'], kind=media['kind'], fileName=media['fileName'],
                    contentType=media['contentType'], sizeBytes=media['sizeBytes'], expiresAt=None,
                    publicFileId=media['publicFileId'])

    def comment_dto(self, comment):
        replies = [c for c in self.comments.values() if c['parentCommentId'] == comment['id'] and not c['deletedAt']]
        return dict(id=comment['id'], issueId=comment['issueId'], parentCommentId=comment['parentCommentId'],
                    body=comment['body'], version=comment['version'], actualAuthor=person_dto(comment['authorId']),
                    createdAt=comment['createdAt'], displayCreatedAt=comment['createdAt'],
                    editedAt=comment['editedAt'], deletedAt=comment['deletedAt'], resolved=comment['resolved'],
                    resolvedBy=person_dto(comment['resolvedById']), resolvedAt=comment['resolvedAt'],
                    replyCount=len(replies), hasReplies=bool(replies),
                    media=[self.descriptor(self.media[m]) for m in comment['mediaIds']], mentions=[])

    def activity_dto(self, entry):
        comment = self.comments.get(entry['commentId']) if entry['commentId'] else None
        shape = {key: value for key, value in entry.items() if key != 'commentId'}
        return dict(shape, comment=self.comment_dto(comment) if comment else None)

    def member_dto(self, org, member):
        person = PEOPLE[member['userId']]
        permissions = OWNER_PERMISSIONS if member['role'] in ('owner', 'admin') else MEMBER_PERMISSIONS
        return dict(id=uid(f"{org['key']}/membership/{member['userId']}"), orgId=org['id'],
                    userId=member['userId'], email=person['email'], firstName=person['firstName'],
                    lastName=person['lastName'], role=member['role'], status=member['status'],
                    joinedAt=member['joinedAt'], lastActiveAt=self.ago(hours=3),
                    permissions=list(permissions) if member['status'] == 'active' else [], invitedBy=None)

    def label_dto(self, label):
        return {key: label[key] for key in ('id', 'orgId', 'name', 'color', 'parentId', 'usedFor', 'state',
                                            'createdAt', 'updatedAt')}

    def cycle_dto(self, cycle):
        rows = [r for r in self.issues.values() if r['cycleId'] == cycle['id'] and not r['archivedAt']]
        types = [self.state_type(r) for r in rows]
        progress = dict(counted=len(rows), completed=types.count('completed'), started=types.count('started'),
                        toDo=sum(t in ('triage', 'backlog', 'unstarted') for t in types),
                        canceled=types.count('canceled'))
        return dict(cycle, finishedAt=None, finishedEarly=False, carriedToNumber=None, progress=progress)

    def project_dto(self, project):
        return dict(id=project['id'], orgId=project['orgId'], name=project['name'], leadId=project['leadId'],
                    leadName=display(project['leadId']), leadAvailable=True, status=project['status'],
                    priority=project['priority'], startDate=project['startDate'], targetDate=project['targetDate'],
                    archived=project['archived'], version=1, createdAt=project['createdAt'],
                    updatedAt=project['updatedAt'], health=project['health'], lastUpdateAt=None,
                    latestUpdateId=None, initiativeId=project['initiativeId'])

    def milestone_dto(self, milestone, content=True):
        rows = [r for r in self.issues.values() if r['milestoneId'] == milestone['id'] and not r['archivedAt']]
        types = [self.state_type(r) for r in rows]
        done = milestone['status'] == 'done'
        return dict(id=milestone['id'], orgId=milestone['orgId'], projectId=milestone['projectId'],
                    name=milestone['name'], description=milestone['description'] if content else None,
                    contentIncluded=content, targetDate=milestone['targetDate'], sortOrder=milestone['sortOrder'],
                    status=milestone['status'], completionSource='manual' if done else None,
                    completedAt=milestone['completedAt'],
                    completedBy=dict(userId=ADA['userId'], displayName=display(ADA['userId']), available=True)
                    if done else None,
                    issueProgress=dict(done=types.count('completed'), total=len(types),
                                       canceled=types.count('canceled'), duplicate=types.count('duplicate')),
                    version=1, createdAt=milestone['createdAt'], updatedAt=milestone['updatedAt'])

    def lineage(self, initiative):
        """The initiative's ancestors, root first, then the initiative itself."""
        chain = [initiative]
        while chain[0]['parentId']:
            chain.insert(0, self.initiatives[chain[0]['parentId']])
        return chain

    def initiative_item(self, initiative):
        projects = [p for p in self.projects.values() if p['initiativeId'] == initiative['id']]
        health = {key: sum(p['health'] == key for p in projects) for key in ('offTrack', 'atRisk', 'onTrack')}
        chain = self.lineage(initiative)
        children = sum(i['parentId'] == initiative['id'] for i in self.initiatives.values())
        return dict(id=initiative['id'], orgId=initiative['orgId'], name=initiative['name'],
                    parentId=initiative['parentId'], level=len(chain) - 1,
                    path=[dict(id=i['id'], name=i['name'], archived=i['archived']) for i in chain],
                    childCount=children, status=initiative['status'], ownerId=ADA['userId'],
                    ownerName=display(ADA['userId']), ownerAvailable=True, targetDate=initiative['targetDate'],
                    archived=initiative['archived'], ancestorArchived=any(i['archived'] for i in chain[:-1]),
                    rollUp=dict(projectCount=len(projects),
                                completedCount=sum(p['status'] == 'completed' for p in projects),
                                openHealth=dict(health, noUpdate=sum(p['health'] is None for p in projects))),
                    version=1, createdAt=initiative['createdAt'], updatedAt=initiative['updatedAt'])

    def decision_item(self, decision):
        def summary(decision_id):
            other = self.decisions.get(decision_id) if decision_id else None
            return {key: other[key] for key in ('id', 'title', 'status')} if other else None
        project = self.projects.get(decision['projectId'])
        initiative = self.initiatives.get(project['initiativeId']) if project and project['initiativeId'] else None
        links = [dict(id=row['id'], identifier=row['identifier'], title=row['title'])
                 for row in (self.issues[i] for i in decision['issueIds'])]
        return dict(id=decision['id'], title=decision['title'], status=decision['status'],
                    decidedOn=decision['decidedOn'],
                    deciders=[dict(userId=i, displayName=display(i), available=i not in FORMER)
                              for i in decision['deciderIds']],
                    deciderCount=len(decision['deciderIds']), projectId=decision['projectId'], initiativeId=None,
                    project=dict(id=project['id'], name=project['name'], archived=False) if project else None,
                    initiative=None,
                    effectiveInitiative=dict(id=initiative['id'], name=initiative['name'], archived=False)
                    if initiative else None,
                    supersedes=summary(decision['supersedesId']), supersededBy=summary(decision['supersededById']),
                    issues=dict(availability='available', items=links, nextCursor=None, hasMore=False,
                                count=len(links)),
                    version=decision['version'], revisionNumber=decision['revisionNumber'])

    def decision_detail(self, decision):
        return dict(self.decision_item(decision), sourceComment=None, decision=decision['decision'],
                    context=decision['context'], why=decision['why'], implications=decision['implications'],
                    createdByUserId=decision['createdById'], createdAt=decision['createdAt'],
                    updatedAt=decision['updatedAt'], acceptedAt=decision['acceptedAt'],
                    acceptedByUserId=decision['acceptedById'], provenance=None, mentions=[])

    def list_dto(self, named):
        items = [i for i in self.list_items.values() if i['listId'] == named['id'] and not i['removed']]
        return dict(id=named['id'], issueId=named['issueId'], name=named['name'], version=named['version'],
                    removed=named['removed'], itemCount=len(items),
                    completedItemCount=sum(i['completed'] for i in items), provenance=None)

    def list_item_dto(self, item):
        completion = (dict(kind='nett', actor=person_dto(item['completedById']), at=item['completedAt'])
                      if item['completed'] else None)
        return dict(id=item['id'], issueId=item['issueId'], listId=item['listId'], version=item['version'],
                    text=item['text'], completed=item['completed'], removed=item['removed'], completion=completion,
                    provenance=None)

    def upload_state(self, upload):
        media = self.media.get(upload['mediaId']) if upload['status'] == 'ready' else None
        prepared = upload['status'] == 'prepared'
        transfer = None
        if prepared:
            transfer = dict(uploadId=upload['id'], uploadUrl=f"{self.origin}/u/{upload['id']}?grant={upload['grant']}",
                            method='PUT', contentType=upload['contentType'], sizeBytes=upload['sizeBytes'],
                            expiresAtUtc=stamp(upload['expires']))
        return dict(mediaId=upload['mediaId'], uploadId=upload['id'], version=upload['version'],
                    status=upload['status'], transferDeadline=stamp(upload['expires']) if prepared else None,
                    expiresAt=None, failureCode=None, media=self.descriptor(media) if media else None,
                    upload=transfer)

    def finish_upload(self, upload):
        """Moves a checked upload to ready and attaches its media to the target."""
        upload['status'] = 'ready'
        upload['version'] += 1
        media = self.add_media(upload['orgId'], upload['fileName'], upload['contentType'], upload['data'],
                               upload['mediaId'], issue_id=upload['issueId'], created=self.now(),
                               version=upload['version'])
        media.update(draftId=upload['draftId'], target=upload['target'], attached=False)
        if upload['target'] == 'issueDescription':
            self.issues[upload['issueId']]['mediaIds'].append(media['id'])
            media['attached'] = True
        elif upload['target'] == 'issueComment':
            comment = self.comments[upload['commentId']]
            comment['mediaIds'].append(media['id'])
            media.update(attached=True, commentId=comment['id'])

    def move_team(self, team):
        return dict(id=team['id'], key=team['key'], name=team['name'], archived=bool(team.get('archivedAt')))

    def transfer_directive(self, transfer):
        return dict(uploadId=transfer['id'], uploadUrl=f"{self.origin}/u/{transfer['id']}?grant={transfer['grant']}",
                    method='PUT', contentType=transfer['contentType'], sizeBytes=transfer['sizeBytes'],
                    expiresAtUtc=stamp(transfer['expires']))

    def attachment_dto(self, record):
        return {key: record[key] for key in ('mediaId', 'version', 'kind', 'fileName', 'contentType', 'sizeBytes',
                                             'sha256', 'publicFileId', 'expiresAt', 'uploadId')}

    def chat_dto(self, record):
        return {key: record[key] for key in ('id', 'reference', 'fileName', 'contentType', 'sizeBytes', 'sha256',
                                             'uploadedByUserId', 'createdAt', 'expiresAt', 'version')}

    def chat_readable(self, record):
        return record is not None and not record['deleted'] and utc_now() < parse_stamp(record['expiresAt'])

    def take_draft(self, org, draft_id, target, row=None):
        """The ready, unattached media prepared for `draft_id`."""
        found = [m for m in self.media.values() if m['orgId'] == org['id'] and m['draftId'] == draft_id
                 and m.get('target') == target and m['status'] == 'ready' and not m['attached']
                 and (row is None or m['issueId'] in (None, row['id']))]
        return sorted(found, key=lambda m: m['createdAt'])


class DeviceAuthority:
    """A fake WorkOS device-authorisation authority. Tokens are opaque to the
    API side: it accepts any access token recorded here until it expires."""

    def __init__(self, auto_approve=False, token_seconds=ACCESS_TOKEN_SECONDS):
        self.auto_approve, self.token_seconds = auto_approve, token_seconds
        self.devices = {}
        self.access = {}
        self.refresh = {}

    def start(self, client_id, origin):
        device_code = secrets.token_urlsafe(32)
        while True:
            user_code = '-'.join(''.join(secrets.choice(USER_CODE_LETTERS) for _ in range(4)) for _ in range(2))
            if all(d['userCode'] != user_code for d in self.devices.values()):
                break
        self.devices[device_code] = dict(userCode=user_code, clientId=client_id, status='pending', polls=0,
                                         expires=time.monotonic() + DEVICE_EXPIRES_SECONDS, lastPoll=None)
        return dict(device_code=device_code, user_code=user_code, verification_uri=f'{origin}/device',
                    verification_uri_complete=f'{origin}/device?user_code={quote(user_code)}',
                    expires_in=DEVICE_EXPIRES_SECONDS, interval=DEVICE_INTERVAL_SECONDS)

    def by_user_code(self, user_code):
        wanted = str(user_code or '').strip().upper()
        return next((d for d in self.devices.values() if d['userCode'] == wanted), None)

    def decide(self, user_code, status):
        device = self.by_user_code(user_code)
        if device is None:
            raise Problem(404, 'Unknown user code.')
        if time.monotonic() >= device['expires']:
            raise Problem(400, 'The user code has expired.')
        if device['status'] not in ('pending', status):
            raise Problem(409, f"The user code is already {device['status']}.")
        device['status'] = status
        return dict(user_code=device['userCode'], status=status)

    def poll(self, device_code, client_id):
        device = self.devices.get(device_code)
        if device is None or device['clientId'] != client_id or device['status'] == 'redeemed':
            return 400, oauth_error('invalid_grant', 'The device code is unknown or already used.')
        now = time.monotonic()
        if now >= device['expires']:
            return 400, oauth_error('expired_token', 'The device code has expired.')
        too_fast = device['lastPoll'] is not None and now - device['lastPoll'] < (
            DEVICE_INTERVAL_SECONDS - POLL_TOLERANCE_SECONDS)
        device['lastPoll'] = now
        device['polls'] += 1
        if self.auto_approve and device['status'] == 'pending' and device['polls'] >= 2:
            device['status'] = 'approved'
        elif too_fast and not self.auto_approve:
            return 400, oauth_error('slow_down', 'Poll no faster than the interval.')
        if device['status'] == 'denied':
            return 400, oauth_error('access_denied', 'The user denied the request.')
        if device['status'] == 'pending':
            return 400, oauth_error('authorization_pending', 'The user has not approved the request yet.')
        device['status'] = 'redeemed'
        return 200, self.issue(client_id)

    def exchange(self, refresh_token, client_id):
        if not refresh_token or self.refresh.get(refresh_token) != client_id:
            return 400, oauth_error('invalid_grant', 'The refresh token is unknown or already used.')
        del self.refresh[refresh_token]
        return 200, self.issue(client_id)

    def issue(self, client_id):
        now = time.time()
        issued = int(now)
        header = b64url(json.dumps(dict(alg='HS256', typ='JWT'), separators=(',', ':')))
        claims = b64url(json.dumps(dict(sub=WORKOS_USER['id'], client_id=client_id, iat=issued,
                                        exp=int(now + self.token_seconds)), separators=(',', ':')))
        access = f'{header}.{claims}.{b64url(secrets.token_bytes(32))}'
        refresh = secrets.token_urlsafe(32)
        self.access[access] = now + self.token_seconds
        self.refresh[refresh] = client_id
        return dict(user=dict(WORKOS_USER), organization_id=None, access_token=access, refresh_token=refresh)

    def accepts(self, token):
        expires = self.access.get(token)
        return expires is not None and time.time() < expires

    def expiry(self, token):
        """When an issued token expires, in epoch seconds; None for any other token."""
        return self.access.get(token)


def oauth_error(code, description):
    return dict(error=code, error_description=description)


# Fake WorkOS routes


def workos_device(ctx):
    params = ctx.handler.read_form()
    client_id = params.get('client_id')
    if not isinstance(client_id, str) or not client_id.strip():
        return 400, oauth_error('invalid_request', 'client_id is required.')
    return 200, ctx.server.authority.start(client_id, ctx.server.origin)


def workos_authenticate(ctx):
    params = ctx.handler.read_form()
    client_id = params.get('client_id')
    if not isinstance(client_id, str) or not client_id.strip():
        return 400, oauth_error('invalid_request', 'client_id is required.')
    grant = params.get('grant_type')
    if grant == DEVICE_GRANT:
        return ctx.server.authority.poll(params.get('device_code'), client_id)
    if grant == 'refresh_token':
        return ctx.server.authority.exchange(params.get('refresh_token'), client_id)
    return 400, oauth_error('unsupported_grant_type', 'Use the device code or refresh token grant.')


def device_page(ctx):
    code = html.escape(ctx.query.one('user_code', '') or '', quote=True)
    field = (f'<input type="hidden" name="user_code" value="{code}"><p>Code: <strong>{code}</strong></p>' if code
             else '<label>Code <input name="user_code" autocomplete="off"></label>')
    page_html = ('<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Fixture device sign-in</title>'
                 '</head><body><h1>Fixture device sign-in</h1><p>Sign in as Ada Fixture.</p>'
                 f'<form method="post" action="/device/approve">{field}<button>Approve</button></form>'
                 f'<form method="post" action="/device/deny">{field}<button>Deny</button></form></body></html>')
    return 200, Raw(page_html.encode(), 'text/html; charset=utf-8')


def device_approve(ctx):
    return 200, ctx.server.authority.decide(ctx.handler.read_form().get('user_code'), 'approved')


def device_deny(ctx):
    return 200, ctx.server.authority.decide(ctx.handler.read_form().get('user_code'), 'denied')


# Credential-free file delivery and upload transfer


def refuse_credentials(ctx):
    """The file host takes no Authorization, Cookie or x-org-id: a grant in the URL is the only authority."""
    if any(ctx.headers.get(name) is not None for name in ('Authorization', 'Cookie', 'x-org-id')):
        raise Problem(400, 'This host is credential-free: send no Authorization, Cookie or x-org-id header.')


def deliver_file(ctx, publicFileId):
    refuse_credentials(ctx)
    grant = ctx.query.one('grant') or ''
    record = ctx.f.grants.get(grant)
    if record is None or record[0] != publicFileId or utc_now() >= record[1] or publicFileId not in ctx.f.files:
        raise Problem(403, 'The file grant is unknown or has expired.')
    stored = ctx.f.files[publicFileId]
    headers = [('X-Content-Type-Options', 'nosniff'), ('Cache-Control', 'private, max-age=60'),
               ('Content-Security-Policy', "default-src 'none'; sandbox")]
    if stored.get('disposition'):
        headers.append(('Content-Disposition', stored['disposition']))
    return 200, Raw(stored['data'], stored['contentType'], headers)


def transfer_upload(ctx, uploadId):
    data = ctx.handler.body
    refuse_credentials(ctx)
    upload_id = parse_uuid(uploadId) or ''
    upload = ctx.f.uploads.get(upload_id) or ctx.f.transfers.get(upload_id)
    grant = ctx.query.one('grant') or ''
    if upload is None or not hmac.compare_digest(grant.encode(), upload['grant'].encode()) \
            or utc_now() >= upload['expires']:
        raise Problem(403, 'The upload grant is unknown or has expired.')
    if upload['status'] != 'prepared' or upload['data'] is not None:
        raise Problem(409, 'This upload is no longer accepting bytes.')
    if ctx.headers.get('Content-Type', '').split(';')[0].strip().lower() != upload['contentType']:
        raise Problem(400, 'Content-Type must match the prepared content type.')
    if ctx.headers.get('Content-Length') != str(upload['sizeBytes']) or len(data) != upload['sizeBytes']:
        raise Problem(400, 'Content-Length and the body must match the prepared size.')
    if hashlib.sha256(data).hexdigest() != upload['sha256']:
        raise Problem(400, 'The body does not match the prepared SHA-256.')
    upload['data'] = data
    if ctx.server.config['dropPutResponse'] > 0:
        ctx.server.config['dropPutResponse'] -= 1
        return 0, Drop()
    return 204, Raw()


# Test hooks


def hook_object(ctx, usage):
    payload = ctx.handler.read_json()
    if not isinstance(payload, dict):
        raise Problem(400, f'Send {usage}.')
    return payload


def hook_org(ctx, payload):
    org = ctx.f.orgs.get(parse_uuid(payload.get('org')) or '')
    if org is None:
        raise Problem(404, 'Organisation not found.')
    return org


def hook_touch(ctx):
    """A change made elsewhere. Optional actorType (human, agent or integration)
    and audit: false, which leaves auditId out of the stream hint."""
    payload = hook_object(ctx, '{"org": <orgId>, "issue": <identifier or UUID>}')
    org = ctx.f.orgs.get(parse_uuid(payload.get('org')) or '')
    row = ctx.f.find_issue(org, payload.get('issue')) if org else None
    if row is None:
        raise Problem(404, 'Issue not found.')
    actor_type = payload.get('actorType', 'human')
    if actor_type not in ACTOR_TYPES or not isinstance(payload.get('audit', True), bool):
        raise Problem(400, 'actorType must be human, agent or integration; audit must be a boolean.')
    at = ctx.f.now()
    row['updatedAt'] = at
    ctx.f.touch(row, at, GRACE['userId'], 'issue.changed', dict(changed=[]),
                hint=dict(actorType=actor_type, audit=payload.get('audit', True)))
    return 200, dict(id=row['id'], identifier=row['identifier'], lastActivityAt=at, updatedAt=at)


def hook_requests(ctx):
    return 200, [dict(request) for request in ctx.server.requests]


def hook_updates(ctx):
    """Makes the update stream answer 503, as while NETT's listener reconnects."""
    payload = ctx.handler.read_json()
    ctx.server.updates_unavailable = bool(isinstance(payload, dict) and payload.get('unavailable'))
    return 200, dict(unavailable=ctx.server.updates_unavailable)


def hook_close_updates(ctx):
    """Closes open update streams, as NETT does on queue overflow or lost
    authority. An optional org limits it to that organisation's streams. With
    abort: true the connection is reset instead of ended, as when a network
    path fails mid-stream."""
    payload = hook_object(ctx, '{} or {"org": <orgId>, "abort": true|false}')
    org_id = hook_org(ctx, payload)['id'] if payload.get('org') is not None else None
    abort = payload.get('abort') is True
    closing = [s for s in ctx.server.subscribers if org_id in (None, s.org_id)]
    for subscriber in closing:
        subscriber.close('closed by the fixture', abort=abort)
    return 200, dict(closed=len(closing))


def hook_capabilities(ctx):
    """Overrides one organisation's capability list; features null restores the
    --serve default. Names the fixture does not implement are advertised only,
    so clients can show they ignore unknown names."""
    payload = hook_object(ctx, '{"org": <orgId>, "features": [names] or null}')
    org = hook_org(ctx, payload)
    features = payload.get('features')
    if features is None:
        ctx.server.org_served.pop(org['id'], None)
    elif not isinstance(features, list) or not all(isinstance(n, str) and 0 < len(n) <= 64 for n in features) \
            or len(set(features)) != len(features):
        raise Problem(400, 'features must be null or a list of distinct names.')
    else:
        ctx.server.org_served[org['id']] = list(features)
    return 200, dict(org=org['id'], features=ctx.server.served_for(org))


def bounded(payload, name, low, high):
    value = payload[name]
    if not is_integer(value) or not low <= value <= high:
        raise Problem(400, f'{name} must be an integer between {low} and {high}.')
    return value


def hook_config(ctx):
    """checkingPolls and retryAfterSeconds: attachment and chat completions answer
    202 with that Retry-After for the first N completions of each upload.
    streamRefusal: the next `times` update-stream requests get `status`.
    failNext: a list of {method, path, status, retryAfter?, times}; a matching
    request (exact path, no query) gets that status instead of an answer.
    slowBody: {path, seconds, times}; the answer's status and headers go first
    and its body after the delay. With waitForRelease, setting slowBody to
    null releases the body early. A path ending in * names every path that
    starts with the rest. dropPutResponse: the next N file-host PUTs
    store their bytes and close the connection without answering.
    mediaReadyAtOnce: managed media completions finish checking at once and
    answer 200 with the ready state."""
    payload = hook_object(ctx, '{"checkingPolls": N, "retryAfterSeconds": S, "streamRefusal": {...}, '
                               '"failNext": [...], "slowBody": {...}, "dropPutResponse": N, '
                               '"mediaReadyAtOnce": B}')
    unknown = sorted(set(payload) - {'checkingPolls', 'retryAfterSeconds', 'streamRefusal', 'failNext', 'slowBody',
                                     'dropPutResponse', 'dropNext', 'mediaReadyAtOnce'})
    if unknown:
        raise Problem(400, f'The fixture has no setting {unknown[0]}.')
    config = ctx.server.config
    if 'checkingPolls' in payload:
        config['checkingPolls'] = bounded(payload, 'checkingPolls', 0, 100)
    if 'retryAfterSeconds' in payload:
        config['retryAfterSeconds'] = bounded(payload, 'retryAfterSeconds', 0, MAX_RETRY_AFTER)
    if 'streamRefusal' in payload:
        refusal = payload['streamRefusal']
        if refusal is None:
            config['streamRefusal'] = None
        else:
            if not isinstance(refusal, dict) or set(refusal) != {'status', 'retryAfter', 'times'} \
                    or refusal['status'] not in (401, 403, 429, 503):
                raise Problem(400, 'streamRefusal is null or {"status": 401|403|429|503, "retryAfter": S, '
                                   '"times": K}.')
            config['streamRefusal'] = dict(status=refusal['status'],
                                           retryAfter=bounded(refusal, 'retryAfter', 0, MAX_RETRY_AFTER),
                                           times=bounded(refusal, 'times', 1, 100))
    if 'failNext' in payload:
        failures = payload['failNext']
        if not isinstance(failures, list):
            raise Problem(400, 'failNext is a list of {method, path, status, retryAfter?, times}.')
        parsed = []
        for failure in failures:
            if not isinstance(failure, dict) or not {'method', 'path', 'status'} <= set(failure) \
                    or set(failure) - {'method', 'path', 'status', 'retryAfter', 'times'} \
                    or failure['method'] not in ('GET', 'POST', 'PUT', 'PATCH', 'DELETE') \
                    or not isinstance(failure['path'], str) or not failure['path'].startswith('/'):
                raise Problem(400, 'failNext is a list of {method, path, status, retryAfter?, times}.')
            retry = failure.get('retryAfter')
            parsed.append(dict(method=failure['method'], path=failure['path'],
                               status=bounded(failure, 'status', 400, 599),
                               retryAfter=None if retry is None else bounded(failure, 'retryAfter', 0, MAX_RETRY_AFTER),
                               times=bounded(dict(times=failure.get('times', 1)), 'times', 1, 100)))
        config['failNext'] = parsed
    if 'slowBody' in payload:
        slow = payload['slowBody']
        if slow is not None:
            if not isinstance(slow, dict) or set(slow) - {'path', 'seconds', 'times', 'waitForRelease'} \
                    or not isinstance(slow.get('path'), str) or not is_number(slow.get('seconds')) \
                    or not 0 < slow['seconds'] <= 10 or not isinstance(slow.get('waitForRelease', False), bool):
                raise Problem(400, 'slowBody is null or {"path": "/…", "seconds": 0–10, "times": K, '
                                   '"waitForRelease": B}.')
            slow = dict(path=slow['path'], seconds=slow['seconds'],
                        times=bounded(dict(times=slow.get('times', 1)), 'times', 1, 100),
                        waitForRelease=slow.get('waitForRelease', False))
        ctx.server.slow_body_release.set()
        ctx.server.slow_body_release = threading.Event()
        config['slowBody'] = slow
    if 'dropPutResponse' in payload:
        config['dropPutResponse'] = bounded(payload, 'dropPutResponse', 0, 100)
    if 'mediaReadyAtOnce' in payload:
        if not isinstance(payload['mediaReadyAtOnce'], bool):
            raise Problem(400, 'mediaReadyAtOnce is true or false.')
        config['mediaReadyAtOnce'] = payload['mediaReadyAtOnce']
    if 'dropNext' in payload:
        drops = payload['dropNext']
        if not isinstance(drops, list) or not all(
                isinstance(d, dict) and set(d) == {'method', 'path', 'times'} and isinstance(d['method'], str)
                and isinstance(d['path'], str) and d['path'].startswith('/') for d in drops):
            raise Problem(400, 'dropNext is a list of {method, path, times}.')
        config['dropNext'] = [dict(method=d['method'].upper(), path=d['path'],
                                   times=bounded(d, 'times', 1, 100)) for d in drops]
    return 200, dict(config)


def hook_tokens(ctx):
    """Registers another bearer acting as one person (ada, grace or linus;
    ada by default). With denyGrants, every route that mints a file grant
    refuses it with 403."""
    payload = hook_object(ctx, '{"token": "...", "user": "ada|grace|linus", "denyGrants": true|false}')
    token, user = payload.get('token'), payload.get('user', 'ada')
    if not isinstance(token, str) or not re.fullmatch(r'[A-Za-z0-9._~+/=-]{8,512}', token) \
            or not isinstance(payload.get('denyGrants', False), bool) or user not in PEOPLE_BY_KEY:
        raise Problem(400, 'token must be 8–512 token characters, user ada, grace or linus, denyGrants a boolean.')
    ctx.server.extra_tokens[token] = dict(denyGrants=payload.get('denyGrants', False),
                                          user=PEOPLE_BY_KEY[user]['userId'])
    ctx.server.revoked_tokens.discard(token)
    return 200, dict(registered=True, user=user, denyGrants=payload.get('denyGrants', False))


def close_streams(ctx, payload, matches):
    """Closes the streams `matches` selects unless closeStreams is false, which
    leaves them to find out at their next flush. Returns how many closed."""
    if not isinstance(payload.get('closeStreams', True), bool):
        raise Problem(400, 'closeStreams must be a boolean.')
    closing = [s for s in ctx.server.subscribers if matches(s)]
    # A write already in flight must finish before the hook acknowledges
    # revocation, whether or not the hook also wakes the stream.
    ctx.handler.revoked_streams = closing
    if not payload.get('closeStreams', True):
        return 0
    for subscriber in closing:
        subscriber.close('lost authority')
    return len(closing)


def hook_revoke_token(ctx):
    """Revokes a bearer: it is 401 everywhere and its streams close."""
    payload = hook_object(ctx, '{"token": "...", "closeStreams": true|false}')
    token = payload.get('token')
    if not isinstance(token, str) or not token:
        raise Problem(400, 'Name the token to revoke.')
    closed = close_streams(ctx, payload, lambda s: s.token == token)
    ctx.server.revoked_tokens.add(token)
    return 200, dict(revoked=True, closed=closed)


def hook_member(ctx, payload):
    org = hook_org(ctx, payload)
    person = PEOPLE_BY_KEY.get(payload.get('user'))
    member = next((m for m in org['members'] if person and m['userId'] == person['userId']), None)
    if member is None:
        raise Problem(404, 'That person has no membership in the organisation.')
    return org, member


def hook_revoke_member(ctx):
    """Ends a person's membership: that organisation's routes and stream
    opens answer 403, and their open streams close."""
    payload = hook_object(ctx, '{"org": <orgId>, "user": "ada|grace|linus", "closeStreams": true|false}')
    org, member = hook_member(ctx, payload)
    closed = close_streams(ctx, payload, lambda s: s.org_id == org['id'] and s.user_id == member['userId'])
    if member['status'] != 'revoked':
        member['revokedFrom'], member['status'] = member['status'], 'revoked'
    return 200, dict(status=member['status'], closed=closed)


def hook_restore_member(ctx):
    """Gives back a membership the revoke hook ended."""
    payload = hook_object(ctx, '{"org": <orgId>, "user": "ada|grace|linus"}')
    _, member = hook_member(ctx, payload)
    if member['status'] == 'revoked':
        member['status'] = member.pop('revokedFrom', 'active')
    return 200, dict(status=member['status'])


def hook_hide_project(ctx):
    """Hides one planning project, by name, from one person."""
    payload = hook_object(ctx, '{"org": <orgId>, "project": "<name>", "user": "ada|grace|linus"}')
    org = hook_org(ctx, payload)
    person = PEOPLE_BY_KEY.get(payload.get('user'))
    project = next((p for p in ctx.f.projects.values() if p['orgId'] == org['id']
                    and p['name'] == payload.get('project')), None)
    if person is None or project is None:
        raise Problem(404, 'Name a project in the organisation and a person.')
    ctx.f.hidden_projects.add((org['id'], project['id'], person['userId']))
    return 200, dict(project=project['id'], hiddenFrom=payload['user'])


def hook_archive_team(ctx):
    """Archives a team: open streams following it close and later requests naming it get 404."""
    payload = hook_object(ctx, '{"org": <orgId>, "key": <team key>}')
    org = hook_org(ctx, payload)
    team = ctx.f.find_team(org, str(payload.get('key') or ''))
    if team is None:
        raise Problem(404, 'Team not found.')
    team['archivedAt'] = team.get('archivedAt') or ctx.f.now()
    closing = [s for s in ctx.server.subscribers if s.org_id == org['id'] and team['key'].upper() in s.keys]
    for subscriber in closing:
        subscriber.close('team archived')
    return 200, dict(id=team['id'], key=team['key'], archivedAt=team['archivedAt'], closed=len(closing))


def hook_archive_issue(ctx):
    """Archives an issue, as someone else would, so a move plan can include an archived sub-issue."""
    payload = hook_object(ctx, '{"org": <orgId>, "issue": <identifier or UUID>}')
    org = hook_org(ctx, payload)
    row = ctx.f.find_issue(org, payload.get('issue'))
    if row is None:
        raise Problem(404, 'Issue not found.')
    if not row['archivedAt']:
        at = ctx.f.now()
        row.update(archivedAt=at, archivedById=GRACE['userId'], updatedAt=at, version=row['version'] + 1)
        ctx.f.touch(row, at, GRACE['userId'], 'issue.changed', dict(changed=['archived']))
    return 200, dict(id=row['id'], identifier=row['identifier'], version=row['version'], archivedAt=row['archivedAt'])


def hook_expire_transfers(ctx):
    """Expires every upload reservation not yet completed, as time would, so
    a request key whose reservation lapsed is refused when used again."""
    hook_object(ctx, '{}')
    now = utc_now()
    expired = [t for t in ctx.f.transfers.values() if t['status'] == 'prepared' and t['expires'] > now]
    for transfer in expired:
        transfer['expires'] = now
    return 200, dict(expired=len(expired))


def updates_stream(ctx):
    """GET /api/v1/updates?project=KEY…, in the organisation x-org-id names."""
    f, server = ctx.f, ctx.server
    headers = ctx.headers.get_all('x-org-id') or []
    if len(headers) != 1 or parse_uuid(headers[0]) is None:
        raise Problem(400, 'Send exactly one x-org-id header naming the organisation whose teams to follow.')
    raw_keys = ctx.query.all('project')
    keys = {key.strip().upper() for key in raw_keys}
    if not all(TEAM_KEY.fullmatch(key.strip()) for key in raw_keys) or not 1 <= len(keys) <= MAX_STREAM_PROJECTS:
        raise Problem(400, f'Name 1 to {MAX_STREAM_PROJECTS} team keys with project=KEY.')
    org = f.orgs.get(parse_uuid(headers[0]))
    if org is None or not f.is_member(org, f.caller):
        raise Problem(403, 'You cannot follow this organisation.')
    if 'issue-tracking' not in org['featurePacks'] or not server.serves(org, 'updates'):
        raise Problem(404, 'No route matches this path.')
    teams = {team['key'].upper(): team for team in f.teams.values() if team['orgId'] == org['id']}
    if any(key not in teams or teams[key].get('archivedAt') for key in keys):
        raise Problem(404, 'A team is unavailable.')
    refusal = server.config['streamRefusal']
    if refusal:
        refusal['times'] -= 1
        if refusal['times'] <= 0:
            server.config['streamRefusal'] = None
        retry = [('Retry-After', str(refusal['retryAfter']))] if refusal['status'] in (429, 503) else []
        raise Problem(refusal['status'], 'The update stream is refused for now.', headers=retry)
    if server.updates_unavailable:
        raise Problem(503, 'The update listener is reconnecting.', headers=[('Retry-After', '1')])
    mine = [s for s in server.subscribers if s.user_id == f.caller]
    if len(mine) >= server.max_streams_per_user or len(server.subscribers) >= server.max_streams:
        raise Problem(429, 'Too many update streams are open.', headers=[('Retry-After', '1')])
    subscriber = Subscriber(org['id'], keys, f.caller, server.authority.expiry(ctx.token), ctx.token)
    server.subscribers.append(subscriber)
    return 200, Stream(subscriber)


def hook_clear(ctx):
    cleared = len(ctx.server.requests)
    ctx.server.requests.clear()
    return 200, dict(cleared=cleared)


# NETT account routes


def api_me(ctx):
    return 200, dict(isAuthenticated=True)


def api_auth_context(ctx):
    f = ctx.f
    orgs = f.accessible()
    header = parse_uuid(ctx.headers.get('x-org-id'))
    current = next((org for org in orgs if org['id'] == header), None)
    if current is None and len(orgs) == 1:
        current = orgs[0]
    created = f.ago(days=400)
    current_dto = f.org_dto(current) if current else None
    person = PEOPLE[f.caller]
    return 200, dict(userId=f.caller,
                     user=dict(id=f.caller, email=person['email'], firstName=person['firstName'],
                               lastName=person['lastName'],
                               createdAt=created, updatedAt=created, systemRole=None),
                     currentOrgId=current and current['id'], currentOrg=current_dto,
                     organisations=[f.org_dto(org) for org in orgs], accessibleOrgIds=[org['id'] for org in orgs],
                     permissions=current_dto['permissions'] if current_dto else [], isSystemContext=False,
                     legal=dict(currentTermsVersion='2026-01', acceptedTermsVersion='2026-01',
                                acceptedTermsAt=created, requiresTermsAcceptance=False))


def api_auth_orgs(ctx):
    return 200, [ctx.f.org_dto(org) for org in ctx.f.accessible()]


# Teams


def teams_list(ctx, orgId):
    include_archived = ctx.query.flag('includeArchived')
    teams = sorted((t for t in ctx.f.teams.values() if t['orgId'] == ctx.org['id']), key=lambda t: t['key'])
    teams = [t for t in teams if include_archived or not t.get('archivedAt')]
    items, cursor = page(ctx, teams, f"teams:{ctx.org['id']}")
    return 200, dict(items=[ctx.f.team_dto(t) for t in items], nextCursor=cursor)


def team_get(ctx, orgId, teamRef):
    team = ctx.f.find_team(ctx.org, teamRef)
    if team is None:
        raise Problem(404, 'Team not found.')
    return 200, ctx.f.team_dto(team)


def team_states(ctx, orgId, teamId):
    team_id = parse_uuid(teamId)
    if team_id is None:
        raise Problem(400, 'teamId must be a UUID.')
    team = ctx.f.teams.get(team_id)
    if team is None or team['orgId'] != ctx.org['id']:
        raise Problem(404, 'Team not found.')
    states = [ctx.f.state_dto(ctx.f.states[i]) for i in team['stateIds']]
    items, cursor = page(ctx, states, f'states:{team_id}')
    return 200, dict(items=items, nextCursor=cursor)


# Issue lists


def issue_filter(ctx):
    """Reads the shared issue filters and returns a predicate over issue rows."""
    f, q, org = ctx.f, ctx.query, ctx.org
    checks = []
    team_ref = q.one('team')
    if team_ref is not None:
        team = f.find_team(org, team_ref)
        if team is None:
            raise Problem(404, 'Team not found.')
        checks.append(lambda r: r['teamId'] == team['id'])
    states = q.uuids('state')
    if states:
        checks.append(lambda r: r['stateId'] in states)
    types = set(q.all('stateType'))
    if types - set(STATE_TYPES):
        raise Problem(400, 'stateType must be a workflow state type.')
    if types:
        checks.append(lambda r: f.state_type(r) in types)
    assignees = set()
    for value in q.all('assignee'):
        if value in ('me', 'none'):
            assignees.add(f.caller if value == 'me' else None)
        elif parse_uuid(value):
            assignees.add(parse_uuid(value))
        else:
            raise Problem(400, 'assignee must be a UUID, me or none.')
    if q.all('assignee'):
        checks.append(lambda r: r['assigneeId'] in assignees)
    labels = q.uuids('label')
    if labels:
        checks.append(lambda r: labels & set(r['labelIds']))
    projects = q.uuids('project')
    if projects:
        checks.append(lambda r: f.effective_project(r) in projects)
    milestones = q.uuids('milestone')
    if milestones:
        checks.append(lambda r: r['milestoneId'] in milestones)
    cycles = q.uuids('cycle')
    if cycles:
        checks.append(lambda r: r['cycleId'] in cycles)
    creators = {f.caller if value == 'me' else parse_uuid(value) for value in q.all('creator')}
    if None in creators:
        raise Problem(400, 'creator must be a UUID or me.')
    if creators:
        checks.append(lambda r: r['createdById'] in creators)
    parent = q.one('parent')
    if parent == 'none':
        checks.append(lambda r: r['parentId'] is None)
    elif parent is not None:
        if not parse_uuid(parent) and not IDENTIFIER.fullmatch(parent):
            raise Problem(400, 'parent must be a UUID, an identifier or none.')
        parent_row = f.find_issue(org, parent)
        checks.append(lambda r: parent_row is not None and r['parentId'] == parent_row['id'])
    priorities = set()
    for value in q.all('priority'):
        if value not in ('0', '1', '2', '3', '4'):
            raise Problem(400, 'priority must be between 0 and 4.')
        priorities.add(int(value))
    if priorities:
        checks.append(lambda r: r['priority'] in priorities)
    text = q.one('q')
    if text is not None:
        if not 1 <= len(text) <= 200:
            raise Problem(400, 'q must be 1–200 characters.')
        checks.append(lambda r: text.casefold() in r['title'].casefold())
    kind = q.one('type')
    if kind is not None:
        if kind not in ('issue', 'tracker'):
            raise Problem(400, 'type must be issue or tracker.')
        checks.append(lambda r: r['type'] == kind)
    if not q.flag('includeArchived'):
        checks.append(lambda r: r['archivedAt'] is None)
    return lambda r: all(check(r) for check in checks)


def ordered(ctx, rows):
    order = ctx.query.one('order', 'created')
    if order == 'created':
        return sorted(rows, key=lambda r: (r['createdAt'], r['id']), reverse=True)
    if order == 'activity':
        return sorted(rows, key=lambda r: (r['lastActivityAt'], r['id']), reverse=True)
    if order == 'priority':
        rows = sorted(rows, key=lambda r: (r['createdAt'], r['id']), reverse=True)
        return sorted(rows, key=lambda r: r['priority'] or 5)
    if order == 'manual':
        if ctx.query.one('team') is None and ctx.query.one('parent') is None:
            raise Problem(400, 'Manual order needs one team or one parent.')
        return sorted(rows, key=lambda r: (r['teamKey'], r['number']))
    raise Problem(400, 'order must be created, priority, manual or activity.')


def issues_list(ctx, orgId):
    predicate = issue_filter(ctx)
    since = ctx.query.one('activitySince')
    since = parse_instant('activitySince', since) if since is not None else None
    rows = [r for r in ctx.f.org_issues(ctx.org)
            if predicate(r) and (since is None or parse_stamp(r['lastActivityAt']) >= since)]
    items, cursor = page(ctx, ordered(ctx, rows), f"issues:{ctx.org['id']}")
    return 200, dict(items=[ctx.f.activity_item(r) for r in items], nextCursor=cursor)


def issues_groups(ctx, orgId):
    f, q = ctx.f, ctx.query
    by = q.one('groupBy')
    if by not in ('state', 'stateType'):
        raise Problem(400, 'groupBy must be state or stateType.')
    if by == 'state' and q.one('team') is None:
        raise Problem(400, 'groupBy=state needs one team.')
    if 'limit' in q.values or 'cursor' in q.values:
        raise Problem(400, 'Groups refuse limit and cursor; use limitPerGroup.')
    per_group = q.integer('limitPerGroup', 25, 0, 25)
    predicate = issue_filter(ctx)
    rows = ordered(ctx, [r for r in f.org_issues(ctx.org) if predicate(r)])
    filters = {name: values for name, values in q.values.items() if name not in ('groupBy', 'limitPerGroup')}
    if by == 'state':
        team = f.find_team(ctx.org, q.one('team'))
        buckets = [(state_id, f.states[state_id]['type']) for state_id in team['stateIds']]
    else:
        buckets = [(None, kind) for kind in STATE_TYPES]
    groups = []
    for state_id, kind in buckets:
        members = [r for r in rows if (r['stateId'] == state_id if state_id else f.state_type(r) == kind)]
        shown = [] if kind in TERMINAL else members[:per_group]
        continuation = dict(filters, **({'state': [state_id]} if state_id else {'stateType': [kind]}))
        more = kind not in TERMINAL and len(members) > per_group
        groups.append(dict(stateId=state_id, stateType=kind, count=len(members),
                           items=[f.list_item(r) for r in shown],
                           nextCursor=make_cursor(f"issues:{ctx.org['id']}", continuation, per_group) if more else None))
    return 200, dict(groupBy=by, groups=groups)


# One issue


def issue_or_404(ctx, ref):
    row = ctx.f.find_issue(ctx.org, ref)
    if row is None:
        raise Problem(404, 'Issue not found.')
    return row


def issue_get(ctx, orgId, issueRef):
    return 200, ctx.f.detail(issue_or_404(ctx, issueRef))


def valid_title(value):
    if not isinstance(value, str) or not value.strip() or len(value) > 200 or any(c in value for c in '\r\n\0'):
        raise Problem(400, 'title must be a single line of 1–200 characters.')
    return value.strip()


def issue_changes(f, org, team, payload, row=None):
    """Checks the editable values of a create or sparse update body and returns
    them as field updates. `row` is None for a create; null clears a value."""
    changes = {}
    if row is None or 'title' in payload:
        changes['title'] = valid_title(payload.get('title'))
    if 'description' in payload:
        value = payload['description']
        if value is not None and (not isinstance(value, str) or len(value) > 50000):
            raise Problem(400, 'description must be at most 50000 characters.')
        changes['description'] = value
    kind = payload.get('type', row['type'] if row else 'issue')
    if 'type' in payload:
        if kind not in ('issue', 'tracker'):
            raise Problem(400, 'type must be issue or tracker.')
        changes['type'] = kind
    if 'stateId' in payload:
        state = f.states.get(parse_uuid(payload['stateId']) or '')
        if state is None or state['teamId'] != team['id']:
            raise Problem(400, "stateId must be one of this team's workflow states.")
        changes['stateId'] = state['id']
    if 'assigneeId' in payload:
        value = payload['assigneeId']
        if value is not None and not f.is_member(org, parse_uuid(value)):
            raise Problem(400, 'assigneeId must be an active member of this organisation.')
        changes['assigneeId'] = parse_uuid(value) if value is not None else None
    if 'priority' in payload:
        if not is_integer(payload['priority']) or not 0 <= payload['priority'] <= 4:
            raise Problem(400, 'priority must be an integer between 0 and 4.')
        changes['priority'] = payload['priority']
    if 'estimate' in payload:
        value = payload['estimate']
        if value is not None and (kind == 'tracker' or not is_number(value)
                                  or value not in ESTIMATES[team['estimateScale']]):
            raise Problem(400, "estimate must be on the team's estimate scale; trackers take none.")
        changes['estimate'] = value
    if 'labelIds' in payload:
        value = payload['labelIds'] if payload['labelIds'] is not None else []
        if not isinstance(value, list) or len(value) > 100:
            raise Problem(400, 'labelIds must be an array of label UUIDs.')
        labels = [f.in_org(f.labels, org, i) for i in value]
        if any(label is None or label['state'] != 'active' or 'issues' not in label['usedFor'] for label in labels) \
                or len({label['id'] for label in labels}) != len(labels):
            raise Problem(400, 'labelIds must name distinct active issue labels in this organisation.')
        changes['labelIds'] = [label['id'] for label in labels]
    if 'projectId' in payload:
        value = payload['projectId']
        project = f.in_org(f.projects, org, value) if value is not None else None
        if value is not None and project is None:
            raise Problem(400, 'projectId must be a project in this organisation.')
        changes['projectId'] = project and project['id']
    if 'parentId' in payload:
        value = payload['parentId']
        parent = f.find_issue(org, value) if value is not None else None
        if value is not None and (parent is None or parent['type'] != 'tracker' or parent['archivedAt']
                                  or kind == 'tracker' or (row and parent['id'] == row['id'])):
            raise Problem(400, 'parentId must be an open tracker in this organisation; trackers cannot nest.')
        changes['parentId'] = parent and parent['id']
    if 'milestoneId' in payload or 'projectId' in changes or 'parentId' in changes:
        project_id = changes.get('projectId', row and row['projectId'])
        if project_id is None:
            parent = f.issues.get(changes.get('parentId', row and row['parentId']) or '')
            project_id = parent and parent['projectId']
        value = payload.get('milestoneId', row and row['milestoneId'])
        milestone = f.in_org(f.milestones, org, value) if value is not None else None
        if 'milestoneId' in payload and value is not None and (milestone is None or milestone['projectId'] != project_id):
            raise Problem(400, "milestoneId must be a milestone of the issue's project.")
        if 'milestoneId' in payload or (milestone and milestone['projectId'] != project_id):
            changes['milestoneId'] = milestone['id'] if milestone and milestone['projectId'] == project_id else None
    if 'cycleId' in payload:
        value = payload['cycleId']
        cycle = f.cycles.get(parse_uuid(value) or '') if value is not None else None
        if value is not None and (cycle is None or cycle['teamId'] != team['id'] or kind == 'tracker'):
            raise Problem(400, "cycleId must be one of this team's cycles; trackers take none.")
        changes['cycleId'] = cycle and cycle['id']
    if 'dueDate' in payload:
        value = payload['dueDate']
        if value is not None:
            try:
                if not isinstance(value, str) or date.fromisoformat(value).isoformat() != value:
                    raise ValueError(value)
            except ValueError as error:
                raise Problem(400, 'dueDate must be a YYYY-MM-DD date.') from error
        changes['dueDate'] = value
    if kind == 'tracker':
        changes.update(estimate=None, cycleId=None)
    return changes


def apply_changes(f, row, changes, at):
    if 'stateId' in changes and changes['stateId'] != row['stateId']:
        f.mark_state(row, changes['stateId'], at)
    row.update({key: value for key, value in changes.items() if key != 'stateId'})


def issue_create(ctx, orgId):
    f, org = ctx.f, ctx.org
    payload = ctx.handler.read_json()
    closed_body(payload, CREATE_FIELDS, CREATE_DECLARED, 'CreateIssueRequest')
    team = f.in_org(f.teams, org, payload.get('teamId'))
    if team is None:
        raise Problem(400, 'teamId must name a team in this organisation.')
    changes = issue_changes(f, org, team, payload)
    drafts = []
    if payload.get('mediaDraftId') is not None:
        draft = parse_uuid(payload['mediaDraftId'])
        drafts = f.take_draft(org, draft, 'issueDescriptionDraft') if draft else []
        if not drafts:
            raise Problem(400, 'mediaDraftId has no ready media.')
    at = f.now()
    number = f.next_issue_number(team)
    row = f.add_issue(org, team, number, changes.pop('title'), 'backlog', at, issue_id=str(uuid.uuid4()))
    apply_changes(f, row, changes, at)
    for media in drafts:
        media.update(issueId=row['id'], attached=True)
        row['mediaIds'].append(media['id'])
    return 201, f.detail(row)


def issue_patch(ctx, orgId, issueRef):
    f = ctx.f
    row = issue_or_404(ctx, issueRef)
    payload = ctx.handler.read_json()
    if isinstance(payload, dict) and payload.get('movePreviewToken') is not None \
            and ctx.server.serves(ctx.org, 'issueMoves'):
        return move_commit(ctx, row, payload)
    closed_body(payload, UPDATE_FIELDS, UPDATE_DECLARED, 'UpdateIssueRequest')
    if not is_integer(payload.get('version')):
        raise Problem(400, 'version is required and must be an integer.')
    if payload['version'] != row['version']:
        raise Problem(409, 'The issue has changed. Read it again and retry.', currentVersion=row['version'])
    if row['archivedAt']:
        raise Problem(409, 'An archived issue refuses changes.', currentVersion=row['version'])
    body = {key: value for key, value in payload.items() if key != 'version'}
    if not body:
        raise Problem(400, 'Send at least one property to change.')
    changes = issue_changes(f, ctx.org, f.teams[row['teamId']], body, row)
    if row['type'] == 'tracker' and changes.get('type') == 'issue' and f.children(row):
        raise Problem(409, 'A tracker with children cannot become an issue.', currentVersion=row['version'])
    at = f.now()
    apply_changes(f, row, changes, at)
    row['version'] += 1
    row['updatedAt'] = at
    f.touch(row, at, f.caller, 'issue.changed', dict(changed=sorted(body)))
    return 200, f.detail(row)


# Discussion


def root_comments(f, row):
    return sorted((c for c in f.comments.values() if c['issueId'] == row['id'] and c['parentCommentId'] is None),
                  key=lambda c: (c['createdAt'], c['id']))


def comments_list(ctx, orgId, issueRef):
    row = issue_or_404(ctx, issueRef)
    items, cursor = page(ctx, root_comments(ctx.f, row), f"comments:{row['id']}", default=20, maximum=50)
    return 200, dict(items=[ctx.f.comment_dto(c) for c in items], nextCursor=cursor)


def comment_replies(ctx, orgId, issueRef, rootId):
    row = issue_or_404(ctx, issueRef)
    root = ctx.f.comments.get(parse_uuid(rootId) or '')
    if root is None or root['issueId'] != row['id'] or root['parentCommentId'] is not None:
        raise Problem(404, 'Thread root not found.')
    replies = sorted((c for c in ctx.f.comments.values() if c['parentCommentId'] == root['id']),
                     key=lambda c: (c['createdAt'], c['id']))
    items, cursor = page(ctx, replies, f"replies:{root['id']}", default=20, maximum=50)
    return 200, dict(items=[ctx.f.comment_dto(c) for c in items], nextCursor=cursor)


def comment_create(ctx, orgId, issueRef):
    f, org = ctx.f, ctx.org
    row = issue_or_404(ctx, issueRef)
    payload = ctx.handler.read_json()
    closed_body(payload, {'body', 'parentCommentId', 'resolved', 'mediaDraftId'}, {'provenance'},
                'CreateIssueCommentRequest')
    body = payload.get('body')
    if not isinstance(body, str) or not body.strip() or len(body) > 50000:
        raise Problem(400, 'body must contain 1–50000 characters.')
    parent = None
    if payload.get('parentCommentId') is not None:
        parent = f.comments.get(parse_uuid(payload['parentCommentId']) or '')
        if parent is None or parent['issueId'] != row['id'] or parent['parentCommentId'] or parent['deletedAt']:
            raise Problem(400, 'parentCommentId must be an undeleted thread root on this issue.')
    resolved = payload.get('resolved')
    if resolved is not None and not isinstance(resolved, bool):
        raise Problem(400, 'resolved must be a boolean.')
    if resolved and parent:
        raise Problem(400, 'Only a thread root can be resolved.')
    media = []
    if payload.get('mediaDraftId') is not None:
        draft = parse_uuid(payload['mediaDraftId'])
        media = f.take_draft(org, draft, 'issueCommentDraft', row) if draft else []
        if not media:
            raise Problem(400, 'mediaDraftId has no ready media for this issue.')
    if row['archivedAt']:
        raise Problem(409, 'An archived issue refuses new comments.')
    comment = f.add_comment(row, f.caller, body.replace('\r\n', '\n').replace('\r', '\n'), f.now(), parent,
                            media=media, resolved=bool(resolved))
    for item in media:
        item.update(issueId=row['id'], commentId=comment['id'], attached=True)
    return 201, dict(f.comment_dto(comment), outcome='created')


def relations_list(ctx, orgId, issueRef):
    row = issue_or_404(ctx, issueRef)
    kind = ctx.query.one('kind')
    if kind is not None and kind not in RELATION_KINDS:
        raise Problem(400, 'kind must be a relation direction.')
    items = [i for i in ctx.f.relation_items(row, ctx.query.flag('includeArchived')) if kind in (None, i['kind'])]
    items, cursor = page(ctx, items, f"relations:{row['id']}")
    return 200, dict(items=items, nextCursor=cursor)


def activity_list(ctx, orgId, issueRef):
    f = ctx.f
    row = issue_or_404(ctx, issueRef)
    entries = sorted(f.events[row['id']], key=lambda e: (e['occurredAt'], e['id']), reverse=True)
    items, cursor = page(ctx, entries, f"activity:{row['id']}")
    people = {e['actor']['userId'] for e in items} | {row['assigneeId'], row['createdById']} - {None}
    decisions = [f.decisions[e['data']['decisionId']] for e in items if e['kind'] == 'decision.logged']
    project = f.projects.get(f.effective_project(row))
    milestone = f.milestones.get(row['milestoneId'])
    references = dict(issues=[], people=[person_dto(i) for i in sorted(people)],
                      labels=[f.label_summary(i) for i in row['labelIds']],
                      projects=[dict(id=project['id'], name=project['name'])] if project else [],
                      milestones=[dict(id=milestone['id'], name=milestone['name'])] if milestone else [],
                      decisions=[{key: d[key] for key in ('id', 'title', 'status')} for d in decisions],
                      lists=[dict(id=n['id'], name=n['name'], removed=n['removed'])
                             for n in f.lists.values() if n['issueId'] == row['id']])
    return 200, dict(items=[f.activity_dto(e) for e in items], nextCursor=cursor, references=references)


def pull_requests_list(ctx, orgId, issueRef):
    row = issue_or_404(ctx, issueRef)
    rows = sorted(ctx.f.pull_requests.get(row['id'], []), key=lambda pr: (pr['lastObservedAt'], pr['id']),
                  reverse=True)
    items, cursor = page(ctx, rows, f"pull-requests:{row['id']}", default=20)
    return 200, dict(items=items, nextCursor=cursor)


def image_items(f, row):
    """Current images in the description, then in comments and replies by time."""
    def alt(body, media):
        found = {media_id: text for text, media_id in MEDIA_LINK.findall(body or '')}
        return found.get(media['id']) or media['fileName']
    items = [dict(media=f.descriptor(f.media[m]), altText=alt(row['description'], f.media[m]),
                  source=dict(kind='description', commentId=None, author=person_dto(row['createdById']),
                              createdAt=row['createdAt'], isReply=False))
             for m in row['mediaIds'] if f.media[m]['kind'] == 'image']
    comments = sorted((c for c in f.comments.values() if c['issueId'] == row['id'] and not c['deletedAt']),
                      key=lambda c: (c['createdAt'], c['id']))
    for comment in comments:
        for media_id in comment['mediaIds']:
            if f.media[media_id]['kind'] == 'image':
                items.append(dict(media=f.descriptor(f.media[media_id]), altText=alt(comment['body'], f.media[media_id]),
                                  source=dict(kind='comment', commentId=comment['id'],
                                              author=person_dto(comment['authorId']), createdAt=comment['createdAt'],
                                              isReply=comment['parentCommentId'] is not None)))
    return items


def images_list(ctx, orgId, issueRef):
    row = issue_or_404(ctx, issueRef)
    q = ctx.query
    items = image_items(ctx.f, row)
    limit = q.integer('limit', 50, 1, 50)
    around = q.one('around')
    scope = f"images:{row['id']}"
    if around is not None:
        if q.one('cursor') is not None:
            raise Problem(400, 'around and cursor are mutually exclusive.')
        if parse_uuid(around) is None:
            raise Problem(400, 'around must be a media UUID.')
        index = next((i for i, item in enumerate(items) if item['media']['mediaId'] == parse_uuid(around)), None)
        if index is None:
            raise Problem(404, 'That image is not on this issue.')
        start = max(0, index - limit // 2)
    else:
        start = read_cursor(q, scope, len(items))
    end = start + limit
    return 200, dict(issueId=row['id'], items=items[start:end],
                     previousCursor=make_cursor(scope, q.values, max(0, start - limit)) if start else None,
                     nextCursor=make_cursor(scope, q.values, end) if end < len(items) else None)


# Media


def media_read(ctx, orgId, mediaId):
    f = ctx.f
    media = f.in_org(f.media, ctx.org, mediaId)
    if media is not None:
        ready = media['status'] == 'ready'
        if ready:
            refuse_denied_grants(ctx)
        return 200, dict(mediaId=media['id'], version=media['version'], status=media['status'], failureCode=None,
                         media=f.descriptor(media) if ready else None,
                         grant=f.grant(media['publicFileId']) if ready else None)
    upload = next((u for u in f.uploads.values() if u['mediaId'] == parse_uuid(mediaId) and u['orgId'] == ctx.org['id']),
                  None)
    if upload is None:
        raise Problem(404, 'Media not found.')
    return 200, dict(mediaId=upload['mediaId'], version=upload['version'], status=upload['status'],
                     failureCode=None, media=None, grant=None)


def file_grants(ctx, orgId):
    f = ctx.f
    payload = ctx.handler.read_json()
    closed_body(payload, {'publicFileIds'}, set(), 'CustomerFilesGrantsRequest')
    ids = payload.get('publicFileIds')
    if not isinstance(ids, list) or not 1 <= len(ids) <= 50 or len(set(map(str, ids))) != len(ids) \
            or not all(isinstance(i, str) and i for i in ids):
        raise Problem(400, 'publicFileIds must hold 1–50 distinct file IDs.')
    if any(i not in f.files or f.files[i]['orgId'] != ctx.org['id'] for i in ids):
        raise Problem(404, 'A file was not found in this organisation.')
    refuse_denied_grants(ctx)
    return 200, dict(items=[f.grant(i) for i in ids])


def media_prepare(ctx, orgId):
    f, org = ctx.f, ctx.org
    payload = ctx.handler.read_json()
    closed_body(payload, {'requestKey', 'target', 'fileName', 'contentType', 'sizeBytes', 'sha256'}, set(),
                'PrepareIssueMediaRequest')
    key = parse_uuid(payload.get('requestKey'))
    if key is None:
        raise Problem(400, 'requestKey must be a UUID.')
    target = payload.get('target')
    closed_body(target, {'kind', 'issueId', 'commentId', 'draftId'}, set(), 'IssueMediaTargetRequest')
    kind = target.get('kind')
    if kind not in MEDIA_TARGETS:
        raise Problem(400, 'target.kind must be ' + ', '.join(MEDIA_TARGETS) + '.')
    row = None
    if target.get('issueId') is not None:
        row = f.in_org(f.issues, org, target['issueId'])
        if row is None:
            raise Problem(404, 'Issue not found.')
    if kind in ('issueDescription', 'issueComment') and row is None:
        raise Problem(400, 'target.issueId is required for this target.')
    comment = None
    if kind == 'issueComment':
        comment = f.comments.get(parse_uuid(target.get('commentId')) or '')
        if comment is None or comment['issueId'] != row['id']:
            raise Problem(400, 'target.commentId must be a comment on the issue.')
    elif target.get('commentId') is not None:
        raise Problem(400, 'target.commentId is only for issueComment.')
    draft = parse_uuid(target.get('draftId'))
    if kind.endswith('Draft') != (draft is not None) or (target.get('draftId') is not None and draft is None):
        raise Problem(400, 'target.draftId is a UUID, required for drafts only.')
    name = payload.get('fileName')
    if not isinstance(name, str) or not 1 <= len(name) <= 255 or any(c in name for c in '/\\') \
            or any(ord(c) < 32 for c in name):
        raise Problem(400, 'fileName must be a plain file name of 1–255 characters.')
    if payload.get('contentType') not in MEDIA_TYPES:
        raise Problem(400, 'contentType must be one of ' + ', '.join(MEDIA_TYPES) + '.')
    if not is_integer(payload.get('sizeBytes')) or not 1 <= payload['sizeBytes'] <= MAX_UPLOAD_BYTES:
        raise Problem(400, f'sizeBytes must be between 1 and {MAX_UPLOAD_BYTES}.')
    if not isinstance(payload.get('sha256'), str) or not re.fullmatch(r'[0-9a-f]{64}', payload['sha256']):
        raise Problem(400, 'sha256 must be 64 lower-case hexadecimal characters.')
    request = json.dumps(payload, sort_keys=True)
    existing = f.upload_keys.get((org['id'], key))
    if existing:
        upload = f.uploads[existing]
        if upload['request'] != request:
            raise Problem(409, 'requestKey was already used for a different upload.')
        return 200, f.upload_state(upload)
    upload = dict(id=str(uuid.uuid4()), mediaId=str(uuid.uuid4()), orgId=org['id'], request=request, target=kind,
                  issueId=row and row['id'], commentId=comment and comment['id'], draftId=draft, fileName=name,
                  contentType=payload['contentType'], sizeBytes=payload['sizeBytes'], sha256=payload['sha256'],
                  grant=secrets.token_urlsafe(24), status='prepared', version=1, data=None,
                  expires=utc_now() + timedelta(seconds=UPLOAD_SECONDS))
    f.uploads[upload['id']] = upload
    f.upload_keys[(org['id'], key)] = upload['id']
    return 200, f.upload_state(upload)


def upload_or_404(ctx, upload_id):
    upload = ctx.f.in_org(ctx.f.uploads, ctx.org, upload_id)
    if upload is None:
        raise Problem(404, 'Upload not found.')
    return upload


def media_complete(ctx, orgId):
    payload = ctx.handler.read_json()
    closed_body(payload, {'uploadId'}, set(), 'CompleteIssueMediaRequest')
    upload = upload_or_404(ctx, payload.get('uploadId'))
    if upload['status'] == 'cleaned':
        raise Problem(409, 'The upload was cancelled.')
    if upload['data'] is None:
        raise Problem(409, 'Transfer the bytes before completing the upload.')
    if upload['status'] == 'prepared':
        upload['status'] = 'checking'
        upload['version'] += 1
        if ctx.server.config['mediaReadyAtOnce']:
            ctx.f.finish_upload(upload)
    return (202 if upload['status'] == 'checking' else 200), ctx.f.upload_state(upload)


def upload_read(ctx, orgId, uploadId):
    upload = upload_or_404(ctx, uploadId)
    if upload['status'] == 'checking':
        ctx.f.finish_upload(upload)
    return 200, ctx.f.upload_state(upload)


def media_cancel(ctx, orgId, mediaId):
    f = ctx.f
    payload = ctx.handler.read_json()
    closed_body(payload, {'version', 'cancel'}, set(), 'CancelIssueMediaRequest')
    if not is_integer(payload.get('version')) or payload.get('cancel') is not True:
        raise Problem(400, 'Send an integer version and cancel: true.')
    upload = next((u for u in f.uploads.values() if u['mediaId'] == parse_uuid(mediaId) and u['orgId'] == ctx.org['id']),
                  None)
    if upload is None:
        if f.in_org(f.media, ctx.org, mediaId):
            raise Problem(409, 'Only an unfinished or draft upload can be cancelled.')
        raise Problem(404, 'Media not found.')
    if payload['version'] != upload['version']:
        raise Problem(409, 'The upload has changed. Read it again and retry.', currentVersion=upload['version'])
    media = f.media.get(upload['mediaId'])
    if media and media['attached']:
        raise Problem(409, 'Attached media is kept; remove it from its issue or comment instead.',
                      currentVersion=upload['version'])
    upload['status'] = 'cleaned'
    upload['version'] += 1
    if media:
        media.update(status='cleaned', version=upload['version'])
        f.files.pop(media['publicFileId'], None)
    return 200, dict(mediaId=upload['mediaId'], version=upload['version'], outcome='cancelled', retention='cleaned')


# Coming desktop features: each answers a neutral 404 unless served for the organisation.


def require(ctx, feature):
    if not ctx.server.serves(ctx.org, feature):
        raise Problem(404, 'No route matches this path.')


def refuse_denied_grants(ctx):
    if ctx.denied_grants:
        raise Problem(403, 'You cannot read this file.')


def declaration(ctx, schema):
    """A Declaration body: requestKey, fileName, contentType, sizeBytes and sha256."""
    payload = ctx.handler.read_json()
    closed_body(payload, {'requestKey', 'fileName', 'contentType', 'sizeBytes', 'sha256'}, set(), schema)
    key = parse_uuid(payload.get('requestKey'))
    if key is None:
        raise Problem(400, 'requestKey must be a UUID.')
    name = payload.get('fileName')
    if not isinstance(name, str) or not 1 <= len(name) <= 255 or any(c in name for c in '/\\') \
            or any(ord(c) < 32 for c in name):
        raise Problem(400, 'fileName must be a plain file name of 1–255 characters.')
    if payload.get('contentType') not in FILE_TYPES:
        raise Problem(400, 'contentType is not an accepted file type.')
    size = payload.get('sizeBytes')
    if not is_integer(size) or size < 1:
        raise Problem(400, 'sizeBytes must be a positive integer.')
    if size > MAX_UPLOAD_BYTES:
        raise Problem(413, f'A file may be at most {MAX_UPLOAD_BYTES} bytes.')
    if not isinstance(payload.get('sha256'), str) or not re.fullmatch(r'[0-9a-f]{64}', payload['sha256']):
        raise Problem(400, 'sha256 must be 64 lower-case hexadecimal characters.')
    return key, payload


def prepare_transfer(ctx, kind, key, payload, issue=None):
    """Reserves an upload, or repeats an exact retry while it is still valid."""
    f, org = ctx.f, ctx.org
    request = json.dumps(dict(payload, issue=issue and issue['id']), sort_keys=True)
    scope = (org['id'], kind, key)
    if scope in f.spent_keys:
        raise Problem(409, 'requestKey was already used for a completed upload.')
    existing = f.transfers.get(f.transfer_keys.get(scope, ''))
    if existing:
        if existing['request'] != request or existing['status'] != 'prepared' or utc_now() >= existing['expires']:
            raise Problem(409, 'requestKey was already used for a different or finished upload.')
        return 200, f.transfer_directive(existing)
    transfer = dict(id=str(uuid.uuid4()), orgId=org['id'], kind=kind, key=key, request=request,
                    issueId=issue and issue['id'], fileName=payload['fileName'], contentType=payload['contentType'],
                    sizeBytes=payload['sizeBytes'], sha256=payload['sha256'], grant=secrets.token_urlsafe(24),
                    status='prepared', data=None, completions=0, recordId=None,
                    expires=utc_now() + timedelta(seconds=UPLOAD_SECONDS))
    f.transfers[transfer['id']] = transfer
    f.transfer_keys[scope] = transfer['id']
    return 200, f.transfer_directive(transfer)


def transfer_or_404(ctx, kind, upload_id, issue=None):
    transfer = ctx.f.transfers.get(parse_uuid(upload_id) or '')
    if transfer is None or transfer['orgId'] != ctx.org['id'] or transfer['kind'] != kind \
            or (issue is not None and transfer['issueId'] != issue['id']):
        raise Problem(404, 'Upload not found.')
    return transfer


def complete_transfer(ctx, kind, issue, create, read):
    """Completes a transfer: 202 while checking, 201 with the new record, 200 on replay."""
    payload = ctx.handler.read_json()
    closed_body(payload, {'uploadId'}, set(), 'CompleteUploadRequest')
    transfer = transfer_or_404(ctx, kind, payload.get('uploadId'), issue)
    if transfer['status'] == 'completed':
        record = read(transfer['recordId'])
        if record is None:
            raise Problem(404, 'The upload is no longer readable.')
        return 200, record
    if transfer['status'] == 'cancelled' or utc_now() >= transfer['expires']:
        raise Problem(409, 'The upload was cancelled or has expired.')
    if transfer['data'] is None:
        raise Problem(409, 'Transfer the bytes before completing the upload.')
    config = ctx.server.config
    transfer['completions'] += 1
    if transfer['completions'] <= config['checkingPolls']:
        return 202, Accepted(dict(uploadId=transfer['id'], status='checking'), config['retryAfterSeconds'])
    record_id = create(transfer)
    transfer.update(status='completed', recordId=record_id, data=None)
    ctx.f.spent_keys.add((transfer['orgId'], kind, transfer['key']))
    return 201, read(record_id)


def cancel_transfer(ctx, kind, upload_id, issue=None):
    transfer = transfer_or_404(ctx, kind, upload_id, issue)
    if transfer['status'] == 'completed':
        raise Problem(409, 'A completed upload cannot be cancelled; delete the file instead.')
    transfer.update(status='cancelled', data=None)
    return 204, Raw()


def version_query(ctx):
    value = ctx.query.one('version')
    if value is None or not re.fullmatch(r'[1-9]\d{0,9}', value):
        raise Problem(400, 'version must be a positive integer.')
    return int(value)


# Standalone issue attachments


def writable_issue(ctx, issueRef):
    row = issue_or_404(ctx, issueRef)
    if row['archivedAt']:
        raise Problem(409, 'An archived issue refuses changes.')
    return row


def attachments_list(ctx, orgId, issueRef):
    require(ctx, 'attachments')
    row = issue_or_404(ctx, issueRef)
    rows = sorted((a for a in ctx.f.attachments.values() if a['issueId'] == row['id']),
                  key=lambda a: (a['createdAt'], a['mediaId']))
    items, cursor = page(ctx, rows, f"attachments:{row['id']}")
    return 200, dict(items=[ctx.f.attachment_dto(a) for a in items], nextCursor=cursor)


def attachment_prepare(ctx, orgId, issueRef):
    require(ctx, 'attachments')
    row = writable_issue(ctx, issueRef)
    key, payload = declaration(ctx, 'PrepareAttachmentRequest')
    return prepare_transfer(ctx, 'attachment', key, payload, row)


def attachment_complete(ctx, orgId, issueRef):
    require(ctx, 'attachments')
    f = ctx.f
    row = writable_issue(ctx, issueRef)

    def create(transfer):
        record = f.add_attachment(row, transfer['fileName'], transfer['contentType'], transfer['data'],
                                  str(uuid.uuid4()), f.now(), upload_id=transfer['id'])
        f.touch(row, record['createdAt'], f.caller, 'attachment.added', dict(mediaId=record['mediaId']))
        return record['mediaId']

    def read(media_id):
        record = f.attachments.get(media_id)
        return f.attachment_dto(record) if record else None

    return complete_transfer(ctx, 'attachment', row, create, read)


def attachment_cancel(ctx, orgId, issueRef, uploadId):
    require(ctx, 'attachments')
    return cancel_transfer(ctx, 'attachment', uploadId, issue_or_404(ctx, issueRef))


def attachment_delete(ctx, orgId, issueRef, mediaId):
    require(ctx, 'attachments')
    f = ctx.f
    row = writable_issue(ctx, issueRef)
    version = version_query(ctx)
    record = f.attachments.get(parse_uuid(mediaId) or '')
    if record is None or record['issueId'] != row['id']:
        raise Problem(404, 'Attachment not found.')
    if version != record['version']:
        raise Problem(409, 'The attachment has changed. Read it again and retry.', currentVersion=record['version'])
    del f.attachments[record['mediaId']]
    f.files.pop(record['publicFileId'], None)
    f.touch(row, f.now(), f.caller, 'attachment.removed', dict(mediaId=record['mediaId']))
    return 204, Raw()


# Chat uploads


def chat_reference(chat_id):
    return 'UPL-' + b64url(uuid.UUID(chat_id).bytes)


def chat_or_404(ctx, reference):
    record = next((c for c in ctx.f.chat_uploads.values() if c['orgId'] == ctx.org['id']
                   and c['reference'] == reference), None)
    if not ctx.f.chat_readable(record):
        raise Problem(404, 'No upload has this reference.')
    return record


def chat_prepare(ctx, orgId):
    require(ctx, 'chatUploads')
    key, payload = declaration(ctx, 'PrepareChatUploadRequest')
    if (ctx.org['id'], key) in ctx.f.chat_uploads:
        raise Problem(409, 'requestKey was already used for a completed upload.')
    return prepare_transfer(ctx, 'chat', key, payload)


def chat_complete(ctx, orgId):
    require(ctx, 'chatUploads')
    f = ctx.f

    def create(transfer):
        created = utc_now()
        record = dict(id=transfer['key'], orgId=transfer['orgId'], reference=chat_reference(transfer['key']),
                      fileName=transfer['fileName'], contentType=transfer['contentType'],
                      sizeBytes=transfer['sizeBytes'], sha256=transfer['sha256'], uploadedByUserId=f.caller,
                      createdAt=f.now(), expiresAt=stamp(created + timedelta(days=CHAT_UPLOAD_DAYS)), version=1,
                      deleted=False, publicFileId='pf_' + hashlib.sha256(
                          f"chat/{transfer['orgId']}/{transfer['key']}".encode()).hexdigest()[:24])
        # Kept per organisation: the same draft UUID in another organisation
        # is another upload, with its own file.
        f.chat_uploads[(record['orgId'], record['id'])] = record
        f.files[record['publicFileId']] = dict(data=transfer['data'], contentType=record['contentType'],
                                               orgId=record['orgId'], disposition='attachment')
        return (record['orgId'], record['id'])

    def read(chat_id):
        record = f.chat_uploads.get(chat_id)
        return f.chat_dto(record) if f.chat_readable(record) else None

    return complete_transfer(ctx, 'chat', None, create, read)


def chat_cancel(ctx, orgId, uploadId):
    require(ctx, 'chatUploads')
    return cancel_transfer(ctx, 'chat', uploadId)


def chat_get(ctx, orgId, reference):
    require(ctx, 'chatUploads')
    return 200, ctx.f.chat_dto(chat_or_404(ctx, reference))


def chat_grant(ctx, orgId, reference):
    require(ctx, 'chatUploads')
    record = chat_or_404(ctx, reference)
    refuse_denied_grants(ctx)
    grant = ctx.f.grant(record['publicFileId'], min(utc_now() + timedelta(seconds=GRANT_SECONDS),
                                                    parse_stamp(record['expiresAt'])))
    return 200, grant


def chat_delete(ctx, orgId, reference):
    require(ctx, 'chatUploads')
    version = version_query(ctx)
    record = chat_or_404(ctx, reference)
    if version != record['version']:
        raise Problem(409, 'The upload has changed. Read it again and retry.', currentVersion=record['version'])
    record['deleted'] = True
    ctx.f.files.pop(record['publicFileId'], None)
    return 204, Raw()


# Team moves


def move_rows(f, root, destination):
    """The root, then every descendant whose team changes, depth first with siblings by ID."""
    rows = [root]

    def walk(parent):
        for child in sorted((r for r in f.issues.values() if r['parentId'] == parent['id']), key=lambda r: r['id']):
            if child['teamId'] != destination['id']:
                rows.append(child)
            walk(child)
    walk(root)
    return rows


def move_row(f, row, destination):
    state = f.states[f.state_of(destination, f.state_type(row))]
    keeps = row['estimate'] is not None and row['estimate'] in ESTIMATES[destination['estimateScale']]
    return dict(id=row['id'], version=row['version'], oldIdentifier=row['identifier'], newIdentifier=None,
                stateId=state['id'], stateName=state['name'], cycleCleared=row['cycleId'] is not None,
                milestoneCleared=False, estimateCleared=row['estimate'] is not None and not keeps,
                estimateAfter=row['estimate'] if keeps else None, parentDetached=False,
                archived=row['archivedAt'] is not None)


def move_context(f, row):
    return dict(issueVersion=row['version'], issueContextVersion=row.get('contextVersion', 1),
                effectiveProjectId=f.effective_project(row), policyVersionId=None)


def move_preview(ctx, orgId, issueRef):
    require(ctx, 'issueMoves')
    f = ctx.f
    row = issue_or_404(ctx, issueRef)
    payload = ctx.handler.read_json()
    closed_body(payload, {'destinationTeamId', 'version'}, set(), 'IssueMovePreviewRequest')
    if not is_integer(payload.get('version')) or payload['version'] < 1:
        raise Problem(400, 'version is required and must be a positive integer.')
    team_id = parse_uuid(payload.get('destinationTeamId'))
    if team_id is None:
        raise Problem(400, 'destinationTeamId must be a team UUID.')
    destination = f.in_org(f.teams, ctx.org, team_id)
    if destination is None:
        raise Problem(404, 'Team not found.')
    if destination['id'] == row['teamId'] or destination.get('archivedAt'):
        raise Problem(400, 'destinationTeamId must be a different active team.')
    if payload['version'] != row['version']:
        raise Problem(409, 'The issue has changed. Read it again and retry.', currentVersion=row['version'])
    if row['archivedAt']:
        raise Problem(409, 'An archived issue cannot move.', currentVersion=row['version'])
    rows = move_rows(f, row, destination)
    token = secrets.token_hex(32)
    context = move_context(f, row)
    f.move_plans[token] = dict(orgId=ctx.org['id'], rootId=row['id'], destinationId=destination['id'],
                               rows=[(r['id'], r['version']) for r in rows], context=context, used=False,
                               expires=utc_now() + timedelta(seconds=MOVE_PLAN_SECONDS))
    return 200, dict(token=token, source=f.move_team(f.teams[row['teamId']]), destination=f.move_team(destination),
                     issues=[move_row(f, r, destination) for r in rows], reviewContext=context)


def move_commit(ctx, row, payload):
    """PATCH with movePreviewToken: moves the whole previewed subtree at once, or 409."""
    f = ctx.f
    closed_body(payload, {'teamId', 'version', 'reviewContext', 'movePreviewToken'}, set(), 'UpdateIssueRequest')
    token = payload['movePreviewToken']
    if not isinstance(token, str) or not re.fullmatch(r'[0-9a-f]{64}', token):
        raise Problem(400, 'movePreviewToken must be 64 lower-case hexadecimal characters.')
    if not is_integer(payload.get('version')) or not isinstance(payload.get('reviewContext'), dict) \
            or parse_uuid(payload.get('teamId')) is None:
        raise Problem(400, 'A move confirmation needs teamId, version, reviewContext and movePreviewToken.')
    plan = f.move_plans.get(token)
    changed = 'The move plan has changed. Preview the move again.'
    if plan is None or plan['used'] or plan['orgId'] != ctx.org['id'] or plan['rootId'] != row['id'] \
            or utc_now() >= plan['expires'] or parse_uuid(payload['teamId']) != plan['destinationId']:
        raise Problem(409, changed, currentVersion=row['version'])
    if payload['version'] != row['version']:
        raise Problem(409, 'The issue has changed. Read it again and retry.', currentVersion=row['version'])
    destination = f.teams[plan['destinationId']]
    rows = move_rows(f, row, destination)
    if payload['reviewContext'] != plan['context'] or [(r['id'], r['version']) for r in rows] != plan['rows'] \
            or destination.get('archivedAt'):
        raise Problem(409, changed, currentVersion=row['version'])
    if any(r['archivedAt'] for r in rows):
        raise Problem(409, 'An archived sub-issue cannot move. Restore it, then preview the move again.',
                      currentVersion=row['version'])
    plan['used'] = True
    at = f.now()
    for r in rows:
        planned = move_row(f, r, destination)
        number = f.next_issue_number(destination)
        old, source = r['identifier'], r['teamKey']
        r.update(teamId=destination['id'], teamKey=destination['key'], number=number,
                 identifier=f"{destination['key']}-{number}", stateId=planned['stateId'], cycleId=None,
                 estimate=planned['estimateAfter'], version=r['version'] + 1, updatedAt=at)
        f.touch(r, at, f.caller, 'issue.moved', dict(fromIdentifier=old, toIdentifier=r['identifier']),
                hint=dict(also=source))
    return 200, f.detail(row)


# Pickers


def members_list(ctx, orgId):
    q = ctx.query
    number = q.integer('page', 1, 1, 100000)
    size = q.integer('pageSize', 20, 1, 100)
    status, role, search = q.one('status', 'active'), q.one('role'), (q.one('search') or '').casefold()
    members = [m for m in ctx.org['members'] if status in ('all', m['status']) and role in (None, m['role'])]
    rows = sorted((ctx.f.member_dto(ctx.org, m) for m in members), key=lambda m: (m['firstName'], m['userId']))
    rows = [m for m in rows if search in f"{m['firstName']} {m['lastName']} {m['email']}".casefold()]
    start = (number - 1) * size
    return 200, dict(items=rows[start:start + size], totalCount=len(rows), page=number, pageSize=size)


def labels_list(ctx, orgId):
    include_inactive = ctx.query.flag('includeInactive')
    used_for = ctx.query.one('usedFor')
    if used_for is not None and used_for not in LABEL_USES:
        raise Problem(400, 'usedFor must be connections, issues or scheduling.')
    labels = [ctx.f.label_dto(label) for label in ctx.f.labels.values() if label['orgId'] == ctx.org['id']
              and (include_inactive or label['state'] == 'active') and used_for in (None, *label['usedFor'])]
    labels.sort(key=lambda label: label['name'])
    return 200, dict(total=len(labels), items=labels)


def cycles_list(ctx, orgId):
    f, q = ctx.f, ctx.query
    team_ref = q.one('team')
    team = f.find_team(ctx.org, team_ref) if team_ref is not None else None
    if team_ref is not None and team is None:
        raise Problem(404, 'Team not found.')
    state = q.one('state')
    if state is not None and state not in ('upcoming', 'current', 'finished'):
        raise Problem(400, 'state must be upcoming, current or finished.')
    number = q.integer('number', None, 1, 100000)
    teams = {t['id'] for t in f.teams.values() if t['orgId'] == ctx.org['id']}
    cycles = sorted((c for c in f.cycles.values() if c['teamId'] in teams and (team is None or c['teamId'] == team['id'])
                     and state in (None, c['state']) and number in (None, c['number'])),
                    key=lambda c: (c['teamKey'], c['number']))
    items, cursor = page(ctx, cycles, f"cycles:{ctx.org['id']}")
    return 200, dict(items=[f.cycle_dto(c) for c in items], nextCursor=cursor, total=len(cycles))


def projects_list(ctx, orgId):
    q = ctx.query
    include_archived, status = q.flag('includeArchived'), q.one('status')
    search = (q.one('search') or '').casefold()
    projects = sorted((p for p in ctx.f.projects.values() if p['orgId'] == ctx.org['id']
                       and (include_archived or not p['archived']) and status in (None, p['status'])
                       and search in p['name'].casefold() and ctx.f.visible_project(p)), key=lambda p: p['name'])
    items, cursor = page(ctx, projects, f"projects:{ctx.org['id']}")
    return 200, dict(items=[ctx.f.project_dto(p) for p in items], nextCursor=cursor)


def milestones_list(ctx, orgId, projectId):
    f, q = ctx.f, ctx.query
    project = f.in_org(f.projects, ctx.org, projectId)
    if project is None:
        raise Problem(404, 'Project not found.')
    wanted = q.uuids('milestoneId')
    content = q.flag('includeContent', True)
    milestones = sorted((m for m in f.milestones.values() if m['projectId'] == project['id']
                         and (not wanted or m['id'] in wanted)), key=lambda m: m['sortOrder'])
    items, cursor = page(ctx, milestones, f"milestones:{project['id']}")
    return 200, dict(items=[f.milestone_dto(m, content) for m in items], nextCursor=cursor,
                     totalCount=len(milestones), doneCount=sum(m['status'] == 'done' for m in milestones),
                     today=f.day(0), timeZone='UTC')


def mention_candidates(ctx, orgId):
    f, q = ctx.f, ctx.query
    if q.one('kind') != 'issue':
        raise Problem(400, 'The fixture lists mention candidates for kind=issue only.')
    content = q.one('contentId')
    row = None
    if content is not None:
        row = f.in_org(f.issues, ctx.org, content)
        if row is None:
            raise Problem(404, 'Issue not found.')
    text = (q.one('q') or '').casefold()
    limit = q.integer('limit', 20, 1, 50)
    suggested = {row['assigneeId'], row['createdById']} if row else set()
    people = [m for m in ctx.org['members'] if m['status'] == 'active' and text in display(m['userId']).casefold()]
    people.sort(key=lambda m: (m['userId'] not in suggested, display(m['userId'])))
    return 200, dict(items=[dict(userId=m['userId'], displayName=display(m['userId']), role=m['role'], canRead=True,
                                 reason=None, isSuggested=m['userId'] in suggested) for m in people[:limit]])


def desktop_capabilities(ctx, orgId):
    """The coming desktop features this NETT serves for the organisation."""
    return 200, dict(features=ctx.server.served_for(ctx.org))


def fleet_dto(ctx):
    at = ctx.f.ago(days=30)
    return dict(FIXTURE_FLEET, id=uid(f"fleet/{ctx.org['id']}"), createdAt=at, updatedAt=at)


def member_initiatives(ctx, orgId):
    """Visible initiatives with their channels, stakeholders and fleet. Archived
    initiatives and everything under one are left out; a fleetless one has
    null fleetId and fleet."""
    require(ctx, 'initiativeChannels')
    f = ctx.f
    visible = [i for i in f.initiatives.values() if i['orgId'] == ctx.org['id']
               and not any(a['archived'] for a in f.lineage(i))]
    rows = sorted(visible, key=lambda i: (i['name'].casefold(), i['id']))
    items, cursor = page(ctx, rows, f"me-initiatives:{ctx.org['id']}")
    fleet = fleet_dto(ctx)
    stakeholder = dict(id=uid(f"{ctx.org['id']}/contact/coordinator"), name='Grace Fixture', role='Coordinator',
                       slackHandle=None, teamsHandle=None, version=1)

    def item(i):
        return dict(id=i['id'], name=i['name'], path=[a['name'] for a in f.lineage(i)],
                    channels=f"#{i['name'].lower()}", stakeholders=[stakeholder] if i['name'] == 'Platform' else [],
                    fleetId=fleet['id'] if i['fleet'] else None, fleet=fleet if i['fleet'] else None, version=1)
    return 200, dict(items=[item(i) for i in items], nextCursor=cursor)


def fleet_get(ctx, orgId, fleetId):
    require(ctx, 'initiativeChannels')
    fleet = fleet_dto(ctx)
    if fleetId not in (fleet['id'], fleet['slug']):
        raise Problem(404, 'Fleet not found.')
    return 200, fleet


def initiatives_list(ctx, orgId):
    q = ctx.query
    search, status = (q.one('search') or '').casefold(), q.one('status')
    include_archived = q.flag('includeArchived')
    rows = sorted((i for i in ctx.f.initiatives.values() if i['orgId'] == ctx.org['id']
                   and search in i['name'].casefold() and status in (None, i['status'])
                   and (include_archived or not any(a['archived'] for a in ctx.f.lineage(i)))),
                  key=lambda i: i['name'])
    items, cursor = page(ctx, rows, f"initiatives:{ctx.org['id']}")
    return 200, dict(items=[ctx.f.initiative_item(i) for i in items], nextCursor=cursor, totalCount=len(rows),
                     maxLevel=max((len(ctx.f.lineage(i)) - 1 for i in rows), default=0), today=ctx.f.day(0))


def initiative_or_404(ctx, initiative_id):
    initiative = ctx.f.in_org(ctx.f.initiatives, ctx.org, initiative_id)
    if initiative is None:
        raise Problem(404, 'Initiative not found.')
    return initiative


def initiative_get(ctx, orgId, initiativeId):
    f = ctx.f
    initiative = initiative_or_404(ctx, initiativeId)
    direct = sum(p['initiativeId'] == initiative['id'] for p in f.projects.values())
    return 200, dict(f.initiative_item(initiative), description=initiative['description'], directProjectCount=direct,
                     today=f.day(0))


def initiative_projects(ctx, orgId, initiativeId):
    f = ctx.f
    initiative = initiative_or_404(ctx, initiativeId)
    ctx.query.one('scope')
    projects = sorted((p for p in f.projects.values() if p['initiativeId'] == initiative['id']
                       and f.visible_project(p)), key=lambda p: p['name'])
    items, cursor = page(ctx, projects, f"initiative-projects:{initiative['id']}")
    item = f.initiative_item(initiative)
    group = {key: item[key] for key in ('id', 'name', 'status', 'archived', 'ancestorArchived', 'path')}
    return 200, dict(items=[f.project_dto(p) for p in items], groups=[group] if projects else [], nextCursor=cursor,
                     totalCount=len(projects), directCount=len(projects))


# Decisions


def decisions_list(ctx, orgId):
    f, q = ctx.f, ctx.query
    issue_id = None
    if q.one('issueId') is not None:
        issue_id = parse_uuid(q.one('issueId'))
        if issue_id is None:
            raise Problem(400, 'issueId must be a UUID.')
    status = q.one('status', 'current')
    wanted = {'current': CURRENT_DECISIONS, 'all': DECISION_STATUSES}.get(status, (status,))
    if not set(wanted) <= set(DECISION_STATUSES):
        raise Problem(400, 'status must be current, all, proposed, accepted, rejected or superseded.')
    scoped = [d for d in f.decisions.values() if d['orgId'] == ctx.org['id']
              and (issue_id is None or issue_id in d['issueIds'])]
    rows = sorted((d for d in scoped if d['status'] in wanted), key=lambda d: (d['decidedOn'], d['id']), reverse=True)
    items, cursor = page(ctx, rows, f"decisions:{ctx.org['id']}", default=20, maximum=50)
    superseded = sum(d['status'] == 'superseded' for d in scoped)
    return 200, dict(items=[f.decision_item(d) for d in items], nextCursor=cursor,
                     counts=dict(current=sum(d['status'] in CURRENT_DECISIONS for d in scoped),
                                 superseded=superseded, all=len(scoped)),
                     omittedSupersededCount=superseded if 'superseded' not in wanted else 0)


def decision_get(ctx, orgId, decisionId):
    decision = ctx.f.in_org(ctx.f.decisions, ctx.org, decisionId)
    if decision is None:
        raise Problem(404, 'Decision not found.')
    return 200, ctx.f.decision_detail(decision)


# Named issue lists


def list_issue(ctx, issue_id):
    if parse_uuid(issue_id) is None:
        raise Problem(400, 'issueId must be a UUID.')
    return issue_or_404(ctx, issue_id)


def list_or_404(ctx, row, list_id):
    named = ctx.f.lists.get(parse_uuid(list_id) or '')
    if named is None or named['issueId'] != row['id']:
        raise Problem(404, 'List not found.')
    return named


def lists_list(ctx, orgId, issueId):
    row = list_issue(ctx, issueId)
    include_removed = ctx.query.flag('includeRemoved')
    rows = sorted((n for n in ctx.f.lists.values() if n['issueId'] == row['id'] and (include_removed or not n['removed'])),
                  key=lambda n: (n['createdAt'], n['id']))
    items, cursor = page(ctx, rows, f"lists:{row['id']}")
    return 200, dict(items=[ctx.f.list_dto(n) for n in items], nextCursor=cursor)


def list_items_list(ctx, orgId, issueId, listId):
    row = list_issue(ctx, issueId)
    named = list_or_404(ctx, row, listId)
    include_removed = ctx.query.flag('includeRemoved')
    completed = ctx.query.one('completed')
    if completed is not None:
        completed = ctx.query.flag('completed')
    rows = [i for i in ctx.f.list_items.values() if i['listId'] == named['id'] and (include_removed or not i['removed'])
            and completed in (None, i['completed'])]
    items, cursor = page(ctx, rows, f"list-items:{named['id']}")
    return 200, dict(items=[ctx.f.list_item_dto(i) for i in items], nextCursor=cursor)


def valid_item_text(value):
    if not isinstance(value, str) or not value.strip() or len(value) > 2000:
        raise Problem(400, 'text must contain 1–2000 characters.')
    return value.strip()


def list_item_create(ctx, orgId, issueId, listId):
    f = ctx.f
    row = list_issue(ctx, issueId)
    named = list_or_404(ctx, row, listId)
    payload = ctx.handler.read_json()
    closed_body(payload, {'text', 'completed'}, {'provenance'}, 'CreateIssueNamedListItemRequest')
    text = valid_item_text(payload.get('text'))
    if payload.get('completed') not in (None, False):
        raise Problem(400, 'An item created as completed needs imported provenance.')
    at = f.now()
    item = dict(id=str(uuid.uuid4()), listId=named['id'], issueId=row['id'], version=1, text=text, completed=False,
                removed=False, completedById=None, completedAt=None)
    f.list_items[item['id']] = item
    f.touch(row, at, f.caller, 'list.item.created', dict(listId=named['id'], itemId=item['id']))
    return 201, dict(item=f.list_item_dto(item), outcome='created')


def list_item_update(ctx, orgId, issueId, listId, itemId):
    f = ctx.f
    row = list_issue(ctx, issueId)
    named = list_or_404(ctx, row, listId)
    item = f.list_items.get(parse_uuid(itemId) or '')
    if item is None or item['listId'] != named['id']:
        raise Problem(404, 'List item not found.')
    payload = ctx.handler.read_json()
    closed_body(payload, {'version', 'text', 'completed', 'removed'}, set(), 'UpdateIssueNamedListItemRequest')
    if not is_integer(payload.get('version')):
        raise Problem(400, 'version is required and must be an integer.')
    if payload['version'] != item['version']:
        raise Problem(409, 'The item has changed. Read it again and retry.', currentVersion=item['version'])
    changes = {key: payload[key] for key in ('text', 'completed', 'removed') if payload.get(key) is not None}
    if not changes:
        raise Problem(400, 'Send text, completed or removed.')
    if any(not isinstance(changes.get(key, False), bool) for key in ('completed', 'removed')):
        raise Problem(400, 'completed and removed must be booleans.')
    at = f.now()
    if 'text' in changes:
        item['text'] = valid_item_text(changes['text'])
    if 'completed' in changes and changes['completed'] != item['completed']:
        item.update(completed=changes['completed'], completedById=f.caller if changes['completed'] else None,
                    completedAt=at if changes['completed'] else None)
    if 'removed' in changes:
        item['removed'] = changes['removed']
    item['version'] += 1
    f.touch(row, at, f.caller, 'list.item.changed', dict(listId=named['id'], itemId=item['id']))
    return 200, dict(item=f.list_item_dto(item), outcome='updated')


class Route:
    """One method and path template. `scope` is None, 'org' (a member
    organisation), 'issues' (also needs the issue-tracking pack) or 'files'
    (also needs a matching x-org-id header)."""

    def __init__(self, method, template, action, scope=None, *, query=''):
        self.method, self.template, self.action, self.scope = method, template, action, scope
        self.query = frozenset(query.split())
        parts = [f'(?P<{part[1:-1]}>[^/]+)' if part.startswith('{') else re.escape(part)
                 for part in template.split('/')]
        self.pattern = re.compile('/'.join(parts))


ORG = '/api/v1/orgs/{orgId}'
PAGE_QUERY = 'limit cursor'
ISSUE_QUERY = 'team state stateType assignee label project milestone cycle creator parent priority q type includeArchived order'
# Specific templates come before the ones whose placeholders would also match them.
ROUTES = [
    Route('POST', '/user_management/authorize/device', workos_device),
    Route('POST', '/user_management/authenticate', workos_authenticate),
    Route('GET', '/device', device_page),
    Route('POST', '/device/approve', device_approve),
    Route('POST', '/device/deny', device_deny),
    Route('GET', '/files/{publicFileId}', deliver_file),
    Route('PUT', '/u/{uploadId}', transfer_upload),
    Route('POST', '/__fixture/touch', hook_touch),
    Route('GET', '/__fixture/requests', hook_requests),
    Route('POST', '/__fixture/requests/clear', hook_clear),
    Route('POST', '/__fixture/updates', hook_updates),
    Route('POST', '/__fixture/updates/close', hook_close_updates),
    Route('POST', '/__fixture/capabilities', hook_capabilities),
    Route('POST', '/__fixture/config', hook_config),
    Route('POST', '/__fixture/tokens', hook_tokens),
    Route('POST', '/__fixture/tokens/revoke', hook_revoke_token),
    Route('POST', '/__fixture/members/revoke', hook_revoke_member),
    Route('POST', '/__fixture/members/restore', hook_restore_member),
    Route('POST', '/__fixture/projects/hide', hook_hide_project),
    Route('POST', '/__fixture/teams/archive', hook_archive_team),
    Route('POST', '/__fixture/issues/archive', hook_archive_issue),
    Route('POST', '/__fixture/transfers/expire', hook_expire_transfers),
    Route('GET', '/api/v1/updates', updates_stream, query='project'),
    Route('GET', '/api/v1/me', api_me),
    Route('GET', '/api/v1/auth/context', api_auth_context),
    Route('GET', '/api/v1/auth/orgs', api_auth_orgs),
    Route('GET', ORG + '/issue-teams', teams_list, 'issues', query=f'includeArchived {PAGE_QUERY}'),
    Route('GET', ORG + '/issue-teams/{teamId}/workflow-states', team_states, 'issues', query=PAGE_QUERY),
    Route('GET', ORG + '/issue-teams/{teamRef}', team_get, 'issues'),
    Route('GET', ORG + '/issues', issues_list, 'issues', query=f'{ISSUE_QUERY} activitySince {PAGE_QUERY}'),
    Route('POST', ORG + '/issues', issue_create, 'issues'),
    Route('GET', ORG + '/issues/groups', issues_groups, 'issues',
          query=f'{ISSUE_QUERY} groupBy limitPerGroup {PAGE_QUERY}'),
    Route('POST', ORG + '/issues/media/prepare', media_prepare, 'issues'),
    Route('POST', ORG + '/issues/media/from-upload', media_complete, 'issues'),
    Route('GET', ORG + '/issues/media/uploads/{uploadId}', upload_read, 'issues'),
    Route('PUT', ORG + '/issues/media/{mediaId}/upload', media_cancel, 'issues'),
    Route('GET', ORG + '/issues/media/{mediaId}', media_read, 'issues'),
    Route('GET', ORG + '/issues/{issueId}/lists', lists_list, 'issues', query=f'includeRemoved {PAGE_QUERY}'),
    Route('GET', ORG + '/issues/{issueId}/lists/{listId}/items', list_items_list, 'issues',
          query=f'includeRemoved completed {PAGE_QUERY}'),
    Route('POST', ORG + '/issues/{issueId}/lists/{listId}/items', list_item_create, 'issues'),
    Route('PUT', ORG + '/issues/{issueId}/lists/{listId}/items/{itemId}', list_item_update, 'issues'),
    Route('GET', ORG + '/issues/{issueRef}', issue_get, 'issues'),
    Route('PATCH', ORG + '/issues/{issueRef}', issue_patch, 'issues'),
    Route('GET', ORG + '/issues/{issueRef}/comments', comments_list, 'issues', query=PAGE_QUERY),
    Route('POST', ORG + '/issues/{issueRef}/comments', comment_create, 'issues'),
    Route('GET', ORG + '/issues/{issueRef}/comments/{rootId}/replies', comment_replies, 'issues', query=PAGE_QUERY),
    Route('GET', ORG + '/issues/{issueRef}/relations', relations_list, 'issues',
          query=f'kind includeArchived {PAGE_QUERY}'),
    Route('GET', ORG + '/issues/{issueRef}/activity', activity_list, 'issues', query=PAGE_QUERY),
    Route('GET', ORG + '/issues/{issueRef}/pull-requests', pull_requests_list, 'issues', query=PAGE_QUERY),
    Route('GET', ORG + '/issues/{issueRef}/images', images_list, 'issues', query=f'around {PAGE_QUERY}'),
    Route('GET', ORG + '/issues/{issueRef}/attachments', attachments_list, 'issues', query=PAGE_QUERY),
    Route('POST', ORG + '/issues/{issueRef}/attachments/prepare', attachment_prepare, 'issues'),
    Route('POST', ORG + '/issues/{issueRef}/attachments/from-upload', attachment_complete, 'issues'),
    Route('DELETE', ORG + '/issues/{issueRef}/attachments/pending/{uploadId}', attachment_cancel, 'issues'),
    Route('DELETE', ORG + '/issues/{issueRef}/attachments/{mediaId}', attachment_delete, 'issues'),
    Route('POST', ORG + '/issues/{issueRef}/move-preview', move_preview, 'issues'),
    Route('POST', ORG + '/chat/uploads/prepare', chat_prepare, 'org'),
    Route('POST', ORG + '/chat/uploads/from-upload', chat_complete, 'org'),
    Route('DELETE', ORG + '/chat/uploads/pending/{uploadId}', chat_cancel, 'org'),
    Route('GET', ORG + '/chat/uploads/{reference}', chat_get, 'org'),
    Route('DELETE', ORG + '/chat/uploads/{reference}', chat_delete, 'org'),
    Route('POST', ORG + '/chat/uploads/{reference}/grants', chat_grant, 'org'),
    Route('POST', ORG + '/files/grants', file_grants, 'files'),
    Route('GET', ORG + '/members', members_list, 'org', query='page pageSize status role search'),
    Route('GET', ORG + '/labels', labels_list, 'org', query='includeInactive usedFor'),
    Route('GET', ORG + '/issue-cycles', cycles_list, 'issues', query=f'team state number {PAGE_QUERY}'),
    Route('GET', ORG + '/projects', projects_list, 'org', query=f'includeArchived status search {PAGE_QUERY}'),
    Route('GET', ORG + '/projects/{projectId}/milestones', milestones_list, 'org',
          query=f'milestoneId includeContent {PAGE_QUERY}'),
    Route('GET', ORG + '/mention-candidates', mention_candidates, 'issues', query='kind contentId q limit'),
    Route('GET', ORG + '/initiatives', initiatives_list, 'org', query=f'search status includeArchived {PAGE_QUERY}'),
    Route('GET', ORG + '/initiatives/{initiativeId}', initiative_get, 'org'),
    Route('GET', ORG + '/initiatives/{initiativeId}/projects', initiative_projects, 'org', query=f'scope {PAGE_QUERY}'),
    Route('GET', ORG + '/decisions', decisions_list, 'org', query=f'issueId status {PAGE_QUERY}'),
    Route('GET', ORG + '/decisions/{decisionId}', decision_get, 'org'),
    Route('GET', ORG + '/desktop/capabilities', desktop_capabilities, 'org'),
    Route('GET', ORG + '/me/initiatives', member_initiatives, 'org', query=PAGE_QUERY),
    Route('GET', ORG + '/fleets/{fleetId}', fleet_get, 'org'),
]


class Context:
    def __init__(self, handler, query):
        self.handler, self.server, self.query = handler, handler.server, query
        self.f, self.headers, self.org = handler.server.fixture, handler.headers, None
        self.token = handler.bearer()
        self.denied_grants = handler.server.extra_tokens.get(self.token, {}).get('denyGrants', False)


def loopback_host(value):
    """The numeric loopback address in `value`; a name or routable address is a ValueError."""
    try:
        address = ipaddress.ip_address(value)
    except ValueError:
        raise ValueError(f'{value!r} is not a numeric loopback address.') from None
    if not address.is_loopback:
        raise ValueError(f'{value!r} is not a loopback address.')
    return address


class FixtureServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, host='127.0.0.1', port=DEFAULT_PORT, access_token=DEFAULT_ACCESS_TOKEN, single_org=False,
                 auto_approve=False, stream_seconds=STREAM_SECONDS, keepalive_seconds=KEEPALIVE_SECONDS,
                 max_streams_per_user=MAX_STREAMS_PER_USER, max_streams=MAX_STREAMS, served=(),
                 access_token_seconds=ACCESS_TOKEN_SECONDS):
        address = loopback_host(host)
        if not access_token or not access_token.strip():
            raise ValueError('A non-empty fixture access token is required.')
        self.address_family = socket.AF_INET6 if address.version == 6 else socket.AF_INET
        self.access_token = access_token
        self.lock = threading.RLock()
        self.requests = []
        self.slow_body_release = threading.Event()
        self.subscribers = []
        self.served = set(served)
        self.org_served = {}
        self.config = dict(checkingPolls=0, retryAfterSeconds=0, streamRefusal=None, failNext=[], slowBody=None,
                           mediaReadyAtOnce=False,
                           dropPutResponse=0, dropNext=[])
        self.started = time.monotonic()
        self.extra_tokens = {}
        self.revoked_tokens = set()
        self.updates_unavailable = False
        self.stream_seconds, self.keepalive_seconds = stream_seconds, keepalive_seconds
        self.max_streams_per_user, self.max_streams = max_streams_per_user, max_streams
        self.fixture = Fixture(single_org)
        self.fixture.on_change = self.publish
        self.authority = DeviceAuthority(auto_approve, access_token_seconds)
        super().__init__((str(address), port), Handler)
        host_text = f'[{address}]' if address.version == 6 else str(address)
        self.origin = f'http://{host_text}:{self.server_port}'
        self.fixture.origin = self.origin

    def token_valid(self, token):
        """The static token, a registered one or an unexpired issued one, unless revoked."""
        if token in self.revoked_tokens:
            return False
        return (hmac.compare_digest(token.encode(), self.access_token.encode())
                or token in self.extra_tokens or self.authority.accepts(token))

    def token_user(self, token):
        """The person a bearer acts as: a registered token's, otherwise Ada."""
        return self.extra_tokens.get(token, {}).get('user', CALLER)

    def authorised(self, subscriber):
        """Whether a stream may still flush: its opening bearer is still valid
        and its person is still an active member with issue tracking."""
        with self.lock:
            org = self.fixture.orgs.get(subscriber.org_id)
            return (subscriber.token is not None and self.token_valid(subscriber.token) and org is not None
                    and self.fixture.is_member(org, subscriber.user_id) and 'issue-tracking' in org['featurePacks'])

    def take_failure(self, method, path):
        """A configured failure for this request, consumed, or None."""
        for failure in self.config['failNext']:
            if failure['method'] == method and failure['path'] == path:
                failure['times'] -= 1
                if failure['times'] <= 0:
                    self.config['failNext'].remove(failure)
                return failure
        return None

    def take_drop(self, method, path):
        """Whether this request is served and then left without an answer, consumed."""
        for drop in self.config['dropNext']:
            if drop['method'] == method and drop['path'] == path:
                drop['times'] -= 1
                if drop['times'] <= 0:
                    self.config['dropNext'].remove(drop)
                return True
        return False

    def take_slow(self, path):
        """The delay before this answer's body, consumed, or None."""
        with self.lock:
            slow = self.config['slowBody']
            wanted = slow and slow['path']
            if not wanted or not (path == wanted or wanted.endswith('*') and path.startswith(wanted[:-1])):
                return None
            slow['times'] -= 1
            if slow['times'] <= 0:
                self.config['slowBody'] = None
            release = self.slow_body_release if slow['waitForRelease'] else None
            return slow['seconds'], release

    def served_for(self, org):
        """The capability list for `org`: a hook override, or each --serve
        feature the organisation is eligible for."""
        if org['id'] in self.org_served:
            return list(self.org_served[org['id']])
        eligible = dict(business=org['orgType'] in ('business', 'enterprise'),
                        issues='issue-tracking' in org['featurePacks'])
        return sorted(name for name in self.served if eligible[FEATURE_NEEDS[name]])

    def serves(self, org, feature):
        return feature in self.served_for(org)

    def publish(self, row, kind, at, audit_id, hint):
        """An issueChanged event for every stream following the issue's
        organisation and team (and hint['also'], a move's source team). The
        action is dotted; auditId is left out when hint['audit'] is false.
        Callers hold the lock."""
        parent = self.fixture.issues.get(row['parentId'] or '')
        action = kind if kind in ('issue.created', 'comment.created') else 'issue.updated'
        event = dict(projectKey=row['teamKey'], issueId=row['id'], identifier=row['identifier'],
                     parentIdentifier=parent['identifier'] if parent else None, version=row['version'],
                     action=action, actorType=hint.get('actorType', 'human'), occurredAt=at)
        if hint.get('audit', True):
            event['auditId'] = audit_id
        keys = {row['teamKey'].upper()} | ({hint['also'].upper()} if hint.get('also') else set())
        for subscriber in self.subscribers:
            if subscriber.org_id == row['orgId'] and keys & subscriber.keys:
                subscriber.push(event)

    def release(self, value):
        """Lets go of a stream that will never be answered, freeing its place."""
        if isinstance(value, Stream) and value.subscriber in self.subscribers:
            self.subscribers.remove(value.subscriber)

    def record(self, method, path, raw_query, headers):
        """Logs the request's shape only: never credentials, bodies, grants or
        device approval codes. It keeps x-org-id, Content-Type and
        Content-Length values."""
        team_keys = {team['key'].upper() for team in self.fixture.teams.values()}

        def safe_value(name, value):
            return (name in ('team', 'project') and value.upper() in team_keys
                    or name == 'stateType' and value in STATE_TYPES)

        query = {name: values if all(safe_value(name, value) for value in values) else ['REDACTED']
                 for name, values in parse_qs(raw_query, keep_blank_values=True).items()}
        length = headers.get('Content-Length')
        request = dict(method=method, path=path, query=query,
                       authorization=headers.get('Authorization') is not None,
                       cookie=headers.get('Cookie') is not None, orgHeader=headers.get('x-org-id'),
                       contentType=headers.get('Content-Type'),
                       contentLength=int(length) if length and re.fullmatch(r'[0-9]{1,10}', length) else None,
                       at=round(time.monotonic() - self.started, 3))
        self.requests.append(request)
        del self.requests[:-MAX_LOGGED_REQUESTS]
        return request


class Handler(BaseHTTPRequestHandler):
    server_version = 'NettFixture/1'
    # Header and write inactivity timeout; request bodies have a total deadline.
    timeout = 10
    body_timeout = 10

    def log_message(self, *_args):
        pass  # Never log headers, bodies or tokens.

    def do_GET(self):
        self.answer()

    def do_POST(self):
        self.answer()

    def do_PUT(self):
        self.answer()

    def do_PATCH(self):
        self.answer()

    def do_DELETE(self):
        self.answer()

    def answer(self):
        url = urlsplit(self.path)
        self.request_log = None
        self.revoked_streams = ()
        if self.headers.get('Host') != urlsplit(self.server.origin).netloc:
            # Only the fixture's own numeric origin, so a page that rebinds a
            # name to loopback cannot drive it.
            self.respond(421, Problem(421, 'Address the fixture by its numeric loopback origin.'))
            return
        log_request = not url.path.startswith('/__fixture/')
        if not log_request and any(origin != self.server.origin for origin in self.headers.get_all('Origin', [])):
            self.respond(403, Problem(403, 'Fixture controls require a same-origin browser request.'))
            return
        failure = None
        with self.server.lock:
            if log_request:
                self.request_log = self.server.record(self.command, url.path, url.query, self.headers)
                failure = self.server.take_failure(self.command, url.path)
        try:
            limit = MAX_UPLOAD_BYTES if self.command == 'PUT' and url.path.startswith('/u/') else MAX_JSON_BYTES
            self.body = self.read_request_body(limit)
            if failure:
                retry = [('Retry-After', str(failure['retryAfter']))] if failure['retryAfter'] is not None else []
                raise Problem(failure['status'], 'The fixture refuses this request for now.', headers=retry)
            with self.server.lock:
                status, value = self.dispatch(url.path, Query(url.query))
                if log_request and self.server.take_drop(self.command, url.path):
                    # Served, then the answer is lost.
                    self.server.release(value)
                    status, value = 0, Drop()
        except Problem as problem:
            status, value = problem.status, problem
        except Exception as error:  # Keep serving; report the type only.
            print(f'NETT fixture error: {type(error).__name__}', file=sys.stderr, flush=True)
            status, value = 500, Problem(500, 'The fixture could not answer this request.')
        # Do not hold the fixture lock while waiting for a subscriber's write.
        for subscriber in self.revoked_streams:
            with subscriber.delivery:
                pass
        self.respond(status, value)

    def dispatch(self, path, query):
        api = path == '/api/v1' or path.startswith('/api/v1/')
        self.server.fixture.caller = CALLER
        if api and not self.bearer_valid():
            raise Problem(401, 'A valid bearer token is required.')
        if api:
            self.server.fixture.caller = self.server.token_user(self.bearer())
        candidates = [(route, match) for route in ROUTES if (match := route.pattern.fullmatch(path))]
        if not candidates:
            raise Problem(404, 'No route matches this path.')
        chosen = next(((route, match) for route, match in candidates if route.method == self.command), None)
        if chosen is None:
            allowed = ', '.join(sorted({route.method for route, _ in candidates}))
            raise Problem(405, f'{self.command} is not supported here.', headers=[('Allow', allowed)])
        route, match = chosen
        params = {name: unquote(value) for name, value in match.groupdict().items()}
        ctx = Context(self, query)
        if route.scope:
            ctx.org = self.scope_org(route.scope, params['orgId'])
        if api and self.command == 'GET':
            query.check_names(route.query)
        return route.action(ctx, **params)

    def bearer(self):
        scheme, _, token = self.headers.get('Authorization', '').partition(' ')
        return token.strip() if scheme.lower() == 'bearer' and token.strip() else None

    def bearer_valid(self):
        token = self.bearer()
        return token is not None and self.server.token_valid(token)

    def scope_org(self, scope, org_ref):
        """The route's organisation. A stranger gets a neutral 404. Every
        organisation route needs exactly one x-org-id naming that organisation:
        a malformed or repeated header is 400, a missing or different one 403,
        as is a person whose membership is no longer active."""
        f = self.server.fixture
        org_id = parse_uuid(org_ref)
        org = f.orgs.get(org_id or '')
        if org is None or not f.has_record(org, f.caller):
            raise Problem(404, 'Organisation not found.')
        headers = self.headers.get_all('x-org-id') or []
        if len(headers) > 1 or (headers and parse_uuid(headers[0]) is None):
            raise Problem(400, 'Send exactly one well-formed x-org-id header.')
        if not headers or parse_uuid(headers[0]) != org_id:
            raise Problem(403, 'x-org-id must name the organisation in the route.')
        if not f.is_member(org, f.caller):
            raise Problem(403, 'You are no longer an active member of this organisation.')
        if scope == 'issues' and 'issue-tracking' not in org['featurePacks']:
            raise Problem(404, 'Issue tracking is not available in this organisation.')
        return org

    def read_request_body(self, limit):
        try:
            size = int(self.headers.get('Content-Length', '0'))
        except ValueError as error:
            raise Problem(400, 'Content-Length must be an integer.') from error
        if size < 0 or size > limit:
            self.close_connection = True
            raise Problem(413, f'The body must be at most {limit} bytes.')
        if not size:
            return b''
        deadline = time.monotonic() + self.body_timeout
        body = bytearray()
        try:
            while len(body) < size:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError
                self.connection.settimeout(remaining)
                # Unlike read(), read1() makes at most one underlying read.
                chunk = self.rfile.read1(min(size - len(body), 64 * 1024))
                if not chunk:
                    raise Problem(400, 'The body ended before Content-Length bytes arrived.')
                body.extend(chunk)
            return bytes(body)
        except (TimeoutError, socket.timeout):
            self.close_connection = True
            raise Problem(408, 'The body did not arrive before the deadline.') from None
        finally:
            self.connection.settimeout(self.timeout)

    def read_json(self):
        content_type = self.headers.get('Content-Type', '').split(';')[0].strip().lower()
        if content_type != 'application/json':
            raise Problem(415, 'Send the body as application/json.')
        try:
            return json.loads(self.body)
        except (ValueError, UnicodeError) as error:
            raise Problem(400, 'The body is not valid JSON.') from error

    def read_form(self):
        """A WorkOS-style body, form-urlencoded or JSON, as a dict of strings."""
        content_type = self.headers.get('Content-Type', '').split(';')[0].strip().lower()
        try:
            if content_type == 'application/json':
                value = json.loads(self.body)
                return value if isinstance(value, dict) else {}
            if content_type == 'application/x-www-form-urlencoded':
                return {name: values[0] for name, values in parse_qs(self.body.decode(), keep_blank_values=True).items()}
        except (ValueError, UnicodeError):
            pass
        return {}

    def stream(self, subscriber):
        """Sends `ready`, then each queued event, a keep-alive comment when
        idle, and closes after the stream lifetime, at the bearer's expiry,
        when the subscriber is closed (overflow, lost authority, an archived
        team) or when the client goes."""
        self.close_connection = True
        lifetime = self.server.stream_seconds
        if subscriber.expires is not None:
            lifetime = min(lifetime, max(0.0, subscriber.expires - time.time()))
        deadline = time.monotonic() + lifetime
        try:
            self.connection.settimeout(STREAM_WRITE_SECONDS)
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream; charset=utf-8')
            self.send_header('Cache-Control', 'private, no-store, no-transform')
            self.end_headers()
            if not self.flush_stream(subscriber, b'event: ready\ndata: null\n\n', deadline):
                return
            while (remaining := deadline - time.monotonic()) > 0:
                with subscriber.ready:
                    if not subscriber.queue and not subscriber.closed:
                        subscriber.ready.wait(min(remaining, self.server.keepalive_seconds))
                    events = list(subscriber.queue)
                    subscriber.queue.clear()
                frame = b': keepalive\n\n'
                if events:
                    encoded = [json.dumps(event, separators=(',', ':')).encode() for event in events]
                    frame = b''.join(b'event: issueChanged\ndata: ' + data + b'\n\n' for data in encoded)
                if not self.flush_stream(subscriber, frame, deadline):
                    if subscriber.abort:
                        # Reset the connection rather than end the response.
                        self.connection.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER,
                                                   struct.pack('ii', 1, 0))
                        self.connection.close()
                    break
        except OSError:
            pass  # The client went away.
        finally:
            with self.server.lock:
                if subscriber in self.server.subscribers:
                    self.server.subscribers.remove(subscriber)

    def flush_stream(self, subscriber, frame, deadline):
        """Deliver one batch under the same lock revocation waits for."""
        with subscriber.delivery:
            if not self.server.authorised(subscriber) or subscriber.closed:
                return False
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return False
            self.connection.settimeout(min(remaining, STREAM_WRITE_SECONDS))
            self.connection.sendall(frame)
            return True

    def respond(self, status, value):
        if isinstance(value, Stream):
            self.stream(value.subscriber)
            return
        if isinstance(value, Drop):
            self.close_connection = True
            return
        headers = []
        if isinstance(value, Accepted):
            headers, value = [('Retry-After', str(value.retry_after))], value.value
        if isinstance(value, Problem):
            headers = list(value.headers)
            body = json.dumps(dict(type='about:blank', title=self.responses.get(status, ('Error',))[0],
                                   status=status, detail=value.detail, **value.extensions)).encode()
            content_type = 'application/problem+json'
        elif isinstance(value, Raw):
            body, content_type, headers = value.body, value.content_type, list(value.headers)
        else:
            body, content_type = json.dumps(value, separators=(',', ':')).encode(), 'application/json'
        self.send_response(status)
        if content_type:
            self.send_header('Content-Type', content_type)
        if status != 204:
            self.send_header('Content-Length', str(len(body)))
        if content_type in ('application/json', 'application/problem+json'):
            self.send_header('Cache-Control', 'no-store')
        for name, header in headers:
            self.send_header(name, header)
        self.end_headers()
        if self.request_log is not None and (slow := self.server.take_slow(self.request_log['path'])):
            self.wfile.flush()
            with self.server.lock:
                self.request_log['slowBodyHeadersStatus'] = status
            seconds, release = slow
            if release is None:
                time.sleep(seconds)
            else:
                release.wait(seconds)
        if status != 204:
            self.wfile.write(body)


def host_argument(value):
    try:
        loopback_host(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError(f'{error} The fixture listens on a numeric loopback address only.')
    return value


def port_argument(value):
    if not re.fullmatch(r'\d{1,5}', value) or int(value) > 65535:
        raise argparse.ArgumentTypeError('Use a port between 0 and 65535.')
    return int(value)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--host', type=host_argument, default='127.0.0.1',
                        help='Numeric loopback address to listen on (default 127.0.0.1).')
    parser.add_argument('--port', type=port_argument, default=DEFAULT_PORT,
                        help=f'Port to listen on; 0 picks a free one (default {DEFAULT_PORT}).')
    parser.add_argument('--access-token', default=DEFAULT_ACCESS_TOKEN,
                        help='Static bearer the API accepts besides issued tokens; never printed.')
    parser.add_argument('--single-org', action='store_true',
                        help='Seed only Fixture Works, so the caller has exactly one organisation.')
    parser.add_argument('--auto-approve', action='store_true',
                        help='Approve each device code on its second poll, without slow_down.')
    parser.add_argument('--stream-seconds', type=float, default=STREAM_SECONDS,
                        help=f'Close each update stream after this long (default {STREAM_SECONDS}).')
    parser.add_argument('--keepalive-seconds', type=float, default=KEEPALIVE_SECONDS,
                        help=f'Send a keep-alive comment after this long idle (default {KEEPALIVE_SECONDS}).')
    parser.add_argument('--serve', action='append', default=[], choices=DESKTOP_FEATURES,
                        help='Advertise and serve a coming desktop feature; repeat for several.')
    parser.add_argument('--access-token-seconds', type=float, default=ACCESS_TOKEN_SECONDS,
                        help=f'Lifetime of tokens the fake authority issues (default {ACCESS_TOKEN_SECONDS}).')
    args = parser.parse_args(argv)
    if not args.access_token.strip():
        parser.error('--access-token must not be empty.')
    if args.stream_seconds <= 0 or args.keepalive_seconds <= 0 or args.access_token_seconds <= 0:
        parser.error('--stream-seconds, --keepalive-seconds and --access-token-seconds must be positive.')
    with FixtureServer(args.host, args.port, args.access_token, args.single_org, args.auto_approve,
                       args.stream_seconds, args.keepalive_seconds, served=args.serve,
                       access_token_seconds=args.access_token_seconds) as server:
        print(f'NETT fixture listening on {server.origin}', flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == '__main__':
    main()
