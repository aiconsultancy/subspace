#!/usr/bin/env python3
"""Loopback-only, memory-only Stargate Fleet API v1 fixture.

Serves the fictional `aurora` fleet of the repository's demo world (the
Fallow Reach rehearsal and its NAV, MAP, WARP and RELAY issues) in the
shapes of Stargate's `src/api/v1/types.ts`: GET /api/v1/fleet, the ship and
crew routes and the /api/v1/events stream, which sends `fleet` and one
`ship` per ship on connect, then activity as the crew works, `host` every
ten seconds and `heartbeat` every fifteen. Every name, host, repository and
token is invented. Never calls upstream services.

Crew diff routes provide a paged checkout for Lyra's Wren, a clean
checkout for Lyra's Talon, and no checkout for Castor's Corvin. The
ordinary fleet, ship and crew reads and --snapshot keep their old shapes.

  python3 scripts/fleet_api_test_server.py --port 0 --token fixture-fleet
  python3 scripts/fleet_api_test_server.py --port 0 --refuse-events  # 503, as with eight streams open
  python3 scripts/fleet_api_test_server.py --port 0 --log-requests   # one line per request
  python3 scripts/fleet_api_test_server.py --snapshot   # one JSON document

--log-requests prints each request's method, path and status with a
fingerprint of its Authorization header (`auth=none`, or `auth=` and the
first 12 hex digits of the header's SHA-256), never the header itself, so a
capture can show which token reached the fixture. Control characters in the
method and path are escaped, and a malformed request logs `-` for them.
"""

import argparse
import base64
import copy
import hashlib
import hmac
import json
import queue
import random
import re
import struct
import threading
import time
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlsplit

M = 60_000
H = 60 * M
GiB = 1024 ** 3
HOST_INTERVAL = 10
HEARTBEAT_INTERVAL = 15
# Stargate's default browser origins: macOS web Spaces and WebView2 hosts.
ORIGINS = (re.compile(r'subspace://[a-z0-9-]+'), re.compile(r'https://subspace\.[a-z0-9-]+'))

# The GitHub fixture's repositories (scripts/github_test_server.py), whose
# pull requests these numbers match where it has them.
NAVIGATION = 'acme/web'
ATLAS = 'acme/design-system'
RELAY = 'acme/api'
REHEARSAL = 'acme/api'

# Deliberately short pages make continuation controls reachable in captures.
DIFF_LIST_LIMIT = 3
DIFF_BYTES_LIMIT = 96
DIFF_PROJECTS = {('aurora-lyra', 'wren'): 'navigation-console',
                 ('aurora-lyra', 'talon'): 'navigation-console'}
DIFF_BASE = 'e19c4f0' + '0' * 33


def chart_png(background, accent):
    """A fictional chart tile with a grid and a highlighted survey corridor."""
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))

    width, height = 160, 96
    pixels = b''.join(b'\0' + b''.join(
        accent if abs(y - (80 - x // 3)) < 4 else b'\x43\x51\x66' if x % 16 == 0 or y % 16 == 0 else background
        for x in range(width)) for y in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(pixels)) + chunk(b'IEND', b''))


def text_change(path, status, additions, deletions, *patches):
    return dict(path=path, previousPath=None, status=status, additions=additions, deletions=deletions), patches, {}


def binary_change(path, status, old, new=None):
    sides = {'old': old} if new is None else {'old': old, 'new': new}
    return (dict(path=path, previousPath=None, status=status, additions=None, deletions=None,
                 binary=True, size=len(sides.get('new', old))), (), sides)


COMMITTED_DIFF = (
    text_change('src/chart/tiles.ts', 'modified', 3, 1,
                '@@ -18,3 +18,4 @@\n export function tileKey(tile: Tile) {\n'
                '-  return tile.id;\n+  return `${tile.sector}:${tile.id}`;\n }\n+\n',
                '@@ -42 +43,2 @@\n export const TILE_SIZE = 256;\n+export const MAX_VISIBLE_TILES = 48;\n'),
    text_change('src/chart/viewport.ts', 'modified', 1, 1,
                '@@ -7 +7 @@\n-export const MIN_ZOOM = 2;\n+export const MIN_ZOOM = 1;\n'),
)
WORKING_DIFF = (
    text_change('docs/chart notes.md', 'added', 2, 0,
                '@@ -0,0 +1,2 @@\n+# Survey chart\n+Keep the next waypoint visible while panning.\n'),
    binary_change('fixtures/chart-tile.png', 'modified', chart_png(b'\x12\x21\x36', b'\xea\xb3\x69'),
                  chart_png(b'\x15\x2d\x3e', b'\x71\xe3\xda')),
    binary_change('fixtures/legacy-tile.bin', 'removed', b'CHART\x00\x01\xff' + bytes(range(184))),
    text_change('src/chart/waypoints.ts', 'added', 1, 0,
                '@@ -0,0 +1 @@\n+export const corridor = "' + 'AuroraFallowReachWaypoint' * 24 + '";\n'),
)
DIFF_MODES = {'branch': sorted(COMMITTED_DIFF + WORKING_DIFF, key=lambda entry: entry[0]['path']),
              'committed': COMMITTED_DIFF, 'working': WORKING_DIFF}


def diff_fingerprint(ship_id, member_id, git_state):
    # The fixture contents and base are fixed; the simulation can change HEAD.
    value = (ship_id, member_id, git_state['branch'], git_state['head'], DIFF_BASE)
    return hashlib.sha256(json.dumps(value).encode()).hexdigest()


def ship_event(detail):
    """Add the checkout fingerprint to SSE only, not to legacy HTTP reads."""
    detail = copy.deepcopy(detail)
    for who in detail['crewMembers']:
        key = (detail['id'], who['id'])
        if key not in DIFF_PROJECTS:
            continue
        for row in who['contexts']:
            if row['git'] and row['project'] == DIFF_PROJECTS[key]:
                row['git']['fingerprint'] = diff_fingerprint(*key, row['git'])
    return 'ship', detail


def page_cursor(*scope):
    return hashlib.sha256(json.dumps(scope).encode()).hexdigest()


def inline_file(entry, fingerprint, mode):
    metadata, patches, _ = entry
    file = dict(metadata, patch=patches[0] if patches else None)
    if len(patches) > 1:
        file['patchNextPage'] = page_cursor(fingerprint, mode, file['path'], 'patch')
    return file


def diff_file(entry, fingerprint, mode, page=None, side=None):
    metadata, patches, sides = entry
    scope = (fingerprint, mode, metadata['path'])
    file = dict(metadata, patch=None)
    result = dict(fingerprint=fingerprint, file=file)
    if sides:
        if side is None:
            side = 'old' if file['status'] == 'removed' else 'new'
        if side not in sides:
            return 404, None
        data = sides[side]
        offset = next((i for i in range(DIFF_BYTES_LIMIT, len(data), DIFF_BYTES_LIMIT)
                       if page == page_cursor(*scope, side, i)), None) if page is not None else 0
        if offset is None:
            return 400, None
        chunk = data[offset:offset + DIFF_BYTES_LIMIT]
        content = dict(side=side, encoding='base64', offset=offset, size=len(data),
                       data=base64.b64encode(chunk).decode('ascii'))
        end = offset + len(chunk)
        if end < len(data):
            content['nextPage'] = page_cursor(*scope, side, end)
        result['content'] = content
    elif side is not None:
        return 400, None
    elif page is None:
        result['file'] = inline_file(entry, fingerprint, mode)
    elif len(patches) > 1 and page == page_cursor(*scope, 'patch'):
        file['patch'] = patches[1]
    else:
        return 400, None
    return 200, result


def diff_list(entries, fingerprint, mode, cursor):
    continuation = page_cursor(fingerprint, mode, 'files')
    if cursor is not None and (cursor != continuation or len(entries) <= DIFF_LIST_LIMIT):
        return None
    start = DIFF_LIST_LIMIT if cursor is not None else 0
    end = start + DIFF_LIST_LIMIT
    files = [inline_file(entry, fingerprint, mode) for entry in entries[start:end]]
    value = dict(total=len(entries), additions=sum(entry[0]['additions'] or 0 for entry in entries),
                 deletions=sum(entry[0]['deletions'] or 0 for entry in entries), files=files)
    if end < len(entries):
        value['nextCursor'] = continuation
    return value


def crew_diff(fleet, ship_id, member_id, file_route, query):
    params = parse_qs(query, keep_blank_values=True)
    keys = {'project', 'mode'} | ({'path', 'page', 'at', 'side'} if file_route else {'cursor'})
    if not params.keys() <= keys or any(len(values) != 1 or not values[0] for values in params.values()):
        return 400, None
    args = {key: values[0] for key, values in params.items()}
    mode = args.get('mode', 'branch')
    project = args.get('project')
    if mode not in DIFF_MODES or args.get('side') not in (None, 'old', 'new'):
        return 400, None
    path = args.get('path', '')
    if file_route and ('at' not in args or not valid_diff_path(path)):
        return 400, None
    fixture_project = DIFF_PROJECTS.get((ship_id, member_id))
    if fixture_project is None or project not in (None, fixture_project):
        return 404, None
    member = fleet.crew(ship_id, member_id)['member']
    git_state = next(row['git'] for row in member['contexts'] if row['project'] == fixture_project)
    fingerprint = diff_fingerprint(ship_id, member_id, git_state)
    entries = DIFF_MODES[mode] if member_id == 'wren' else ()
    if file_route:
        if args['at'] != fingerprint:
            return 409, None
        entry = next((entry for entry in entries if entry[0]['path'] == path), None)
        return diff_file(entry, fingerprint, mode, args.get('page'), args.get('side')) if entry else (404, None)
    value = diff_list(entries, fingerprint, mode, args.get('cursor'))
    if value is None:
        return 400, None
    return 200, dict(value, ship=ship_id, member=member_id, project=project, mode=mode,
                     branch=git_state['branch'], head=git_state['head'].ljust(40, '0'),
                     base={'ref': git_state['base'], 'commit': DIFF_BASE}, fingerprint=fingerprint,
                     computedAt=fleet.now)


def valid_diff_path(path):
    return (all(part not in ('', '.', '..', '.git') for part in path.split('/'))
            and '\\' not in path and all(ch.isprintable() for ch in path))


class Clock:
    """Task ids and times for one fleet."""

    def __init__(self, now):
        self.now = now
        self.serial = 4100

    def ago(self, minutes):
        return self.now - round(minutes * M)

    def task_id(self):
        self.serial += 1
        return f'task-{self.serial}'


def task(clock, status, summary, phase='', progress='', created=None, started=None, finished=None):
    return dict(id=clock.task_id(), status=status, kind='assignment', summary=summary, phase=phase,
                progress=progress, createdAt=created if created is not None else started if started is not None else finished,
                startedAt=started, updatedAt=finished if finished is not None else started if started is not None else created,
                finishedAt=finished)


def running(clock, summary, phase, progress, minutes):
    return task(clock, 'running', summary, phase, progress, created=clock.ago(minutes + 2), started=clock.ago(minutes))


def waiting(clock, summary, minutes):
    return task(clock, 'queued', summary, 'Waiting for a worker slot', created=clock.ago(minutes))


def done(clock, summary, minutes_ago, took, status='completed'):
    finished = clock.ago(minutes_ago)
    phase = {'completed': 'Done', 'abandoned': 'Abandoned', 'cancelled': 'Cancelled'}.get(status, 'No confirmed outcome')
    return task(clock, status, summary, phase, created=finished - (took + 3) * M,
                started=finished - took * M, finished=finished)


def git(clock, branch, head, ahead, behind, changed, repository, base='origin/main'):
    return dict(branch=branch, head=head, base=base, ahead=ahead, behind=behind, changedFiles=changed,
                repository=repository, checkedAt=clock.ago(0.2))


def pull(repository, number, title, state, checks):
    return dict(number=number, url=f'https://github.com/{repository}/pull/{number}', title=title, state=state, checks=checks)


def context(project, current=None, queued=(), recent=(), git_state=None, pr=None, observed=False,
            provisioned='ready', known=False, recent_count=None):
    row = dict(project=project, activity='idle', provisioned=provisioned, observed=observed, current=current,
               queuedCount=len(queued), queued=list(queued), lastOutcome=recent[0] if recent else None,
               recent=list(recent), recentCount=recent_count or len(recent), git=git_state, pullRequest=pr)
    if known or pr is not None:
        row['pullRequestKnown'] = True
    return row


def context_activity(row):
    current = row['current']
    if current and current['status'] in ('running', 'dispatching', 'cancelling'):
        return 'running'
    last = row['lastOutcome']
    if (current and current['status'] == 'uncertain') or (not current and last and last['status'] == 'uncertain'):
        return 'uncertain'
    if not current and row['queuedCount'] > 0:
        return 'queued'
    return 'idle' if row['provisioned'] == 'ready' else 'unprovisioned'


RANK = dict(running=0, uncertain=1, queued=2, idle=3, unprovisioned=4)


def derive_member(member):
    """The member's derived state from its contexts, as Stargate's projection does."""
    for row in member['contexts']:
        row['activity'] = context_activity(row)
        row['queuedCount'] = max(row['queuedCount'], len(row['queued']))
        row['lastOutcome'] = row['recent'][0] if row['recent'] else None
        row['recentCount'] = max(row['recentCount'], len(row['recent']))
    primary = sorted(member['contexts'], key=lambda row: RANK[row['activity']])[0] if member['contexts'] else None
    member['activity'] = primary['activity'] if primary else 'unprovisioned'
    if member['activity'] == 'running':
        member['since'] = primary['current']['startedAt']
    elif member['activity'] == 'uncertain':
        member['since'] = (primary['current'] or primary['lastOutcome'])['updatedAt']
    elif member['activity'] == 'queued':
        member['since'] = min(row['createdAt'] for row in primary['queued'])
    elif member['activity'] == 'idle' and primary['lastOutcome']:
        member['since'] = primary['lastOutcome']['finishedAt']
    member['idleSince'] = member['since'] if member['activity'] == 'idle' else None
    return member


def member(member_id, name, role, contexts, model='gpt-6.2-sol', backend='opencode', subagents=(), since=None):
    return derive_member(dict(id=member_id, name=name, role=role, model=model, backend=backend, activity='idle',
                              since=since, idleSince=None, contexts=list(contexts), subagents=list(subagents)))


def feature(key, label, enabled, default=False, value=None):
    row = dict(key=key, label=label, enabled=enabled, defaultEnabled=default)
    if value is not None:
        row['value'] = value
    return row


def ship_features(transport, peers):
    common = [feature('crewActivity', 'Crew activity', True, False, 'summary'),
              feature('fleetPolicy', 'Fleet policy', True), feature('fleetControl', 'Fleet control', True),
              feature('fleetControl.reviews', 'Fleet reviews', True), feature('waiting', 'Turn waiting notices', True, True)]
    if transport == 'irc':
        return [feature('irc.tls', 'IRC TLS', True, True), feature('irc.sasl', 'IRC authentication', True, True),
                feature('irc.channelRequests', 'IRC channel requests', True, True, 'mention'),
                feature('irc.liveProgress', 'Live IRC request progress', True, True),
                feature('irc.peerCaptains', 'IRC peer captains', peers),
                feature('irc.crew', 'IRC crew connections', True),
                feature('irc.audit', 'IRC direct message audit', False)] + common
    return [feature('slack.threadContext', 'Slack thread context', True, True),
            feature('slack.peerCaptains', 'Slack peer captains', peers),
            feature('slack.captainPost', 'Slack captain posting', True),
            feature('slack.health', 'Slack health command', False)] + common


def request_progress(clock, first, stages):
    return [dict(request=first + index, stage=stage, observedAt=clock.ago(minutes), disconnected=False,
                 stale=False, status=status, failure=None)
            for index, (stage, minutes, status) in enumerate(stages)]


def captain(clock, captain_id, name, transport, backend, queue_counts, since_minutes, requests=None):
    # derive_ship sets the activity from the crew; `since` is when it last changed.
    row = dict(id=captain_id, name=name, role='captain', transport=transport, backend=backend,
               model='claude-opus-5-5', queue=queue_counts, activity=None, since=clock.ago(since_minutes))
    if requests is not None:
        row['requests'] = requests
    return row


def queue_counts(queued, running_jobs, done_jobs, failed=0, responding=0):
    return dict(queued=queued, running=running_jobs, responding=responding, done=done_jobs, failed=failed)


def ship(ship_id, name, captain_row, repository, crew, peers=False):
    return dict(id=ship_id, name=name, captain=captain_row, repository=repository, available=True,
                crew=dict(people=0, busy=0, queued=0), features=ship_features(captain_row['transport'], peers),
                crewMembers=crew)


def derive_ship(detail, now):
    members = detail['crewMembers']
    busy = sum(entry['activity'] == 'running' for entry in members)
    queued = sum(row['queuedCount'] for entry in members for row in entry['contexts'])
    detail['crew'] = dict(people=len(members), busy=busy, queued=queued)
    activity = 'running' if busy else 'queued' if queued else 'idle'
    previous = detail['captain']['activity']
    detail['captain']['activity'] = activity
    if previous is not None and previous != activity:
        detail['captain']['since'] = now
    detail['captain']['queue'] = dict(detail['captain']['queue'], running=busy, queued=queued)
    return detail


def lyra(c):
    return ship('aurora-lyra', 'Lyra', captain(c, 'aurora-lyra', 'aurora-lyra', 'irc', 'opencode',
                queue_counts(0, 0, 128, 2), 34, request_progress(c, 3811, [('completed', 41, 'done'), ('tool', 2, 'running')])),
                NAVIGATION, [
        member('wren', 'Wren', 'Chart rendering', [context(
            'navigation-console', observed=True,
            current=running(c, 'Cancel tile decodes that leave the viewport (NAV-7)', 'Running tests',
                            'Decode jobs carry the viewport epoch now. 38 of 41 chart tests pass; the seam-crossing cases are still running.', 34),
            queued=[waiting(c, 'Capture a cold-cache pan trace for NAV-1', 12)],
            recent=[done(c, 'Capture a chart-pan baseline on the pilot laptop (NAV-6)', 205, 46),
                    done(c, 'Size the decode pool to the visible tiles', 470, 63),
                    done(c, 'Flaky seam-crossing snapshot', 790, 9, 'abandoned')],
            recent_count=14,
            git_state=git(c, 'perf/nav-7-cancel-offscreen-decodes', 'b41e7d2', 5, 0, 2, NAVIGATION),
            pr=pull(NAVIGATION, 492, 'Cancel tile decodes that leave the viewport (NAV-7)', 'open', 'running'))],
            subagents=[dict(id='ses-wren-1', title='test-runner · chart-tiles', updatedAt=c.ago(1)),
                       dict(id='ses-wren-2', title='explore · decode pool', updatedAt=c.ago(6))]),
        member('quill', 'Quill', 'Accessibility', [context(
            'navigation-console', observed=True,
            current=running(c, 'Tab from a chart marker to the burn summary (NAV-11)', 'Implementing',
                            'Focus order follows the navigation checklist; wiring the marker shortcut into the burn summary.', 18),
            recent=[done(c, 'Screen-reader labels for chart markers', 300, 38)],
            git_state=git(c, 'feat/nav-11-marker-focus-order', '7c90a1e', 2, 1, 4, NAVIGATION), known=True)],
            model='claude-sonnet-5-5'),
        member('juniper', 'Juniper', 'Route preview', [context(
            'navigation-console',
            queued=[waiting(c, 'Course correction preview ignores relay drift (NAV-2)', 9),
                    waiting(c, 'Label unconfirmed burns in the route preview (NAV-8)', 7)],
            recent=[done(c, 'Route preview snapshot tests', 95, 21)],
            git_state=git(c, 'main', 'e19c4f0', 0, 0, 0, NAVIGATION))]),
        member('talon', 'Talon', 'Telemetry uplink', [context(
            'navigation-console',
            recent=[done(c, 'Document the telemetry frame format (NAV-4)', 222, 31),
                    done(c, 'Parse the frame header in one pass', 640, 27)],
            git_state=git(c, 'main', 'e19c4f0', 0, 0, 0, NAVIGATION))]),
    ], peers=True)


def orion(c):
    return ship('aurora-orion', 'Orion', captain(c, 'aurora-orion', 'aurora-orion', 'irc', 'opencode',
                queue_counts(0, 0, 96, 1), 52, request_progress(c, 2207, [('completed', 64, 'done'), ('reasoning', 1, 'running')])),
                RELAY, [
        member('marlow', 'Marlow', 'Relay protocol', [context(
            'relay-handover', observed=True,
            current=running(c, 'Recover queued frames after a relay swap (RELAY-1)', 'Reviewing diff',
                            'Unresolved acknowledgements now cross the handover; checking that confirmed frames are never replayed.', 52),
            recent=[done(c, 'Add sequence counters to the handover dashboard (RELAY-2)', 260, 41)],
            git_state=git(c, 'fix/relay-1-recover-queued-frames', '3fa06b9', 7, 0, 0, RELAY),
            pr=pull(RELAY, 1188, 'Carry unresolved acknowledgements across a relay swap (RELAY-1)', 'draft', 'passing'))],
            model='claude-opus-5-5',
            subagents=[dict(id='ses-marlow-1', title='review · sequence filter', updatedAt=c.ago(2))]),
        member('pike', 'Pike', 'Simulator scenarios', [context(
            'relay-handover',
            current=task(c, 'uncertain', 'Replay a duplicate acknowledgement in the simulator (RELAY-3)', 'No confirmed outcome',
                         'The worker stopped reporting during the late-ack replay. Stored state does not prove the run is live.',
                         created=c.ago(49), started=c.ago(47)),
            recent=[done(c, 'Replay edge-case sequence gaps during rollout (RELAY-12)', 410, 55)],
            git_state=git(c, 'test/relay-3-duplicate-ack-replay', '0d5a2c8', 3, 2, 1, RELAY),
            pr=pull(RELAY, 1187, 'Simulator: duplicate acknowledgement replay (RELAY-3)', 'open', 'failing'))]),
        member('ines', 'Ines', 'Atlas overlays', [
            context('sector-atlas', observed=True,
                    current=running(c, 'Show the survey corridor at every zoom level (MAP-1)', 'Implementing',
                                    'Corridor geometry is pinned across the three zoom bands; label thinning comes next.', 26),
                    queued=[waiting(c, 'Pick atlas label density by zoom band (MAP-2)', 15)],
                    recent=[done(c, 'Record the high-contrast map palette (MAP-3)', 180, 24)],
                    git_state=git(c, 'feat/map-1-corridor-zoom-bands', '5e2b8f4', 4, 0, 6, ATLAS), known=True),
            context('relay-handover', recent=[done(c, 'Relay status colours on the atlas', 1500, 18)],
                    git_state=git(c, 'main', '9a41c03', 0, 3, 0, RELAY)),
        ], model='claude-sonnet-5-5', subagents=[dict(id='ses-ines-1', title='explore · zoom bands', updatedAt=c.ago(4))]),
        member('sable', 'Sable', 'Atlas data', [context(
            'sector-atlas', observed=True,
            current=running(c, 'Reject route manifests with mismatched atlas versions (MAP-9)', 'Opening PR',
                            'Pushing the branch and opening a draft pull request with the version check and its tests.', 41),
            recent=[done(c, 'Compare tile cache hit rates after warm-up (MAP-11)', 330, 29)],
            git_state=git(c, 'fix/map-9-atlas-version-check', 'c8d17e5', 3, 0, 0, ATLAS),
            pr=pull(ATLAS, 107, 'Reject route manifests with mismatched atlas versions (MAP-9)', 'draft', 'running'))]),
    ], peers=True)


def vega(c):
    return ship('aurora-vega', 'Vega', captain(c, 'aurora-vega', 'aurora-vega', 'irc', 'pi-durable',
                queue_counts(0, 0, 41), 22, request_progress(c, 915, [('model', 3, 'running')])), REHEARSAL, [
        member('kestrel', 'Kestrel', 'Rehearsal scripts', [context(
            'fallow-reach-rehearsal', observed=True,
            current=running(c, 'Rehearse the 25% relay handover on the Fallow Reach route', 'Running tests',
                            'relay rehearse --traffic 25 --scenario late-ack: two of three passes done, no lost frames so far.', 22),
            recent=[done(c, 'Script the cold-cache chart pan for the rehearsal', 150, 17)],
            git_state=git(c, 'chore/fallow-reach-25-percent-rehearsal', '61f3b0a', 1, 0, 3, REHEARSAL))],
            backend='pi-durable'),
        member('moss', 'Moss', 'Replay data', [context(
            'fallow-reach-rehearsal', observed=True,
            current=running(c, 'Pack the Fallow Reach replay bundle for the rehearsal', 'Implementing',
                            'Packing the route, its sector pack and the replay trace into one bundle.', 11),
            queued=[waiting(c, 'Replay the WARP-7 sample-range paging against the Fallow Reach run', 4)],
            recent=[done(c, 'Trim the replay trace to the survey corridor', 120, 22)],
            git_state=git(c, 'chore/fallow-reach-replay-bundle', 'a07c6d1', 2, 0, 5, REHEARSAL), known=True)],
            backend='pi-durable', model='claude-sonnet-5-5'),
        member('fern', 'Fern', 'Runbooks', [context(
            'fallow-reach-rehearsal',
            recent=[done(c, 'Publish the pilot handover runbook (RELAY-5)', 47, 33)],
            git_state=git(c, 'main', 'd2b9e71', 0, 0, 0, REHEARSAL))], backend='pi-durable'),
        member('basil', 'Basil', 'Operator checklists', [context(
            'fallow-reach-rehearsal',
            recent=[done(c, 'Checklist for the relay stand-down brief', 372, 44),
                    done(c, 'Checklist for the pre-burn crew brief', 1210, 25)],
            git_state=git(c, 'main', 'd2b9e71', 0, 0, 0, REHEARSAL))], backend='pi-durable'),
    ])


def castor(c):
    return ship('aurora-castor', 'Castor', captain(c, 'aurora-castor', 'aurora-castor', 'slack', 'opencode',
                queue_counts(0, 0, 73, 3), 63), NAVIGATION, [
        member('rook', 'Rook', 'Performance traces', [context(
            'navigation-console', observed=True,
            current=running(c, 'Render star chart tiles off the UI thread (NAV-1)', 'Running tests',
                            'Worker pool lands decodes off the UI thread; warm pans hold at 41 ms, the cold pass is being re-run.', 63),
            recent=[done(c, 'Input-latency trace for the pilot review', 280, 36)],
            git_state=git(c, 'perf/nav-1-tile-worker-pool', '8be4d39', 9, 3, 0, NAVIGATION),
            pr=pull(NAVIGATION, 491, 'Decode star chart tiles in a worker pool (NAV-1)', 'open', 'passing'))],
            model='claude-opus-5-5', subagents=[dict(id='ses-rook-1', title='test-runner · pan traces', updatedAt=c.ago(1))]),
        member('hazel', 'Hazel', 'Status panel design', [context(
            'relay-handover', observed=True,
            current=running(c, 'Show when the backup relay last responded (RELAY-6)', 'Implementing',
                            'Using the last valid acknowledgement in the status panel; empty and stale states drafted.', 14),
            recent=[done(c, 'Status panel tokens for relay states', 410, 30)],
            git_state=git(c, 'feat/relay-6-backup-relay-last-ack', 'f4c2a90', 1, 0, 5, RELAY), known=True)]),
        member('linnet', 'Linnet', 'Release and CI', [context(
            'navigation-console',
            recent=[done(c, 'Release navigation console 0.9.0', 72, 28),
                    done(c, 'Cache the chart fixtures in CI', 900, 19)],
            git_state=git(c, 'main', 'e19c4f0', 0, 0, 0, NAVIGATION))]),
        member('corvin', 'Corvin', 'Security review', [context('navigation-console', provisioned='pending')],
               model=None, since=c.ago(3)),
    ])


def gateway_features():
    return [feature('dashboard', 'Dashboard', True), feature('dashboard.groupAccess', 'Shared dashboard socket', False),
            feature('api', 'Fleet API', True), feature('cronjobs', 'File-backed cron', True),
            feature('waiting', 'Turn waiting notices', True, True), feature('transcription', 'Transcription', False),
            feature('notifications.updateNotice', 'Update notices', True),
            feature('quotaWatch', 'Quota watch', True, False, 'anthropic, openai'),
            feature('maintenance', 'Nightly maintenance', True), feature('maintenance.drain', 'Maintenance drain', True),
            feature('maintenance.cleanup', 'Worktree cleanup', True),
            feature('maintenance.report', 'Maintenance reports', True),
            feature('maintenance.components.stargate', 'Stargate updates', True),
            feature('maintenance.components.models', 'Model updates', False),
            feature('maintenance.stargate.requireTreeCheck', 'Release archive tree check', True, True)]


class Fleet:
    """The aurora fleet and a small simulation that keeps it working."""

    def __init__(self, now=None, seed=7):
        now = int(now if now is not None else time.time() * 1000)
        self.clock = Clock(now)
        self.random = random.Random(seed)
        self.pr_number = 420
        c = self.clock
        self.name = 'aurora'
        self.host = dict(hostname='aurora-vm-01', platform='linux', arch='x64',
                         uptimeSec=23 * 86400 + 7 * 3600 + 41 * 60,
                         cpu=dict(cores=16, usage=0.47, load=[6.12, 5.48, 4.91]),
                         memory=dict(totalBytes=64 * GiB, availableBytes=round(23.6 * GiB)),
                         disks=[dict(path='/', totalBytes=256 * GiB, freeBytes=round(141.2 * GiB)),
                                dict(path='data', totalBytes=1024 * GiB, freeBytes=round(587.4 * GiB))],
                         sampledAt=now)
        self.gateway = dict(version='0.43.0', opencodeVersion='2.3.2', bunVersion='1.4.4',
                            startedAt=c.ago(3 * 24 * 60 + 5 * 60 + 12), features=gateway_features())
        self.admiral = dict(id='aurora-admiral', name='aurora-admiral', role='rear_admiral', transport='irc',
                            backend='claude', model='claude-opus-5-5',
                            queue=queue_counts(2, 1, 57, 1), activity='running', since=c.ago(6),
                            requests=request_progress(c, 5120, [('completed', 38, 'done'), ('completed', 17, 'done'),
                                                                 ('reasoning', 1, 'running')]))
        self.ships = [derive_ship(build(c), now) for build in (lyra, orion, vega, castor)]
        for detail in self.ships:
            detail['detailUpdatedAt'] = now

    @property
    def now(self):
        return self.clock.now

    def set_now(self, now):
        self.clock.now = int(now)

    def overview(self):
        idle = [(entry['idleSince'], detail['id'], entry['id']) for detail in self.ships
                for entry in detail['crewMembers'] if entry['activity'] == 'idle' and entry['idleSince']]
        longest = min(idle) if idle else None
        summaries = [{key: value for key, value in detail.items() if key not in ('crewMembers', 'detailUpdatedAt')}
                     for detail in self.ships]
        return copy.deepcopy(dict(apiVersion=1, generatedAt=self.now, name=self.name, host=self.host,
                                  gateway=self.gateway, admiral=self.admiral, ships=summaries,
                                  longestIdle=dict(ship=longest[1], member=longest[2], since=longest[0]) if longest else None))

    def ship(self, ship_id):
        detail = next((entry for entry in self.ships if entry['id'] == ship_id), None)
        if detail is None:
            return None
        detail = copy.deepcopy(detail)
        detail.pop('detailUpdatedAt', None)
        return detail

    def crew(self, ship_id, member_id):
        detail = self.ship(ship_id)
        found = next((entry for entry in detail['crewMembers'] if entry['id'] == member_id), None) if detail else None
        return dict(ship=ship_id, member=found) if found else None

    def snapshot(self):
        return dict(fleet=self.overview(), ships={entry['id']: self.ship(entry['id']) for entry in self.ships},
                    crew={entry['id']: {who['id']: self.crew(entry['id'], who['id']) for who in entry['crewMembers']}
                          for entry in self.ships})

    def initial(self):
        return [('fleet', self.overview())] + [ship_event(detail) for detail in self.ships]

    def host_sample(self):
        cpu = self.host['cpu']
        cpu['usage'] = round(min(0.92, max(0.18, cpu['usage'] + (self.random.random() - 0.5) * 0.1)), 3)
        cpu['load'] = [round(max(0.5, value + (self.random.random() - 0.5) * 0.6), 2) for value in cpu['load']]
        memory = self.host['memory']
        memory['availableBytes'] = round(min(40 * GiB, max(12 * GiB, memory['availableBytes'] + (self.random.random() - 0.5) * GiB)))
        self.host['uptimeSec'] += HOST_INTERVAL
        self.host['sampledAt'] = self.now
        return [('host', copy.deepcopy(self.host))] + [
            ('ship-sampled', dict(id=detail['id'], detailUpdatedAt=self.now)) for detail in self.ships]

    def tick(self):
        """One change, as the events Stargate would send for it."""
        working = [(detail, who) for detail in self.ships for who in detail['crewMembers']
                   if who['activity'] == 'running']
        waiting_crew = [(detail, who) for detail in self.ships for who in detail['crewMembers']
                        if who['activity'] == 'queued']
        if waiting_crew and len(working) < 9:
            return self.start(*self.random.choice(waiting_crew))
        if not working:
            return []
        return self.advance(*self.random.choice(working))

    PHASES = ('Implementing', 'Running tests', 'Reviewing diff', 'Opening PR')
    PROGRESS = {'Implementing': 'Writing the change and its focused tests.',
                'Running tests': 'Focused tests running. Lint is clean.',
                'Reviewing diff': 'Reviewing the diff before opening a pull request.',
                'Opening PR': 'Pushing the branch and opening a draft pull request.'}

    def activity(self, detail, who, kind, text, **extra):
        return ('activity', dict(ship=detail['id'], member=who['id'] if who else None, kind=kind,
                                 text=text[:200], at=self.now, **extra))

    def changed(self, detail, who):
        derive_member(who)
        derive_ship(detail, self.now)
        detail['detailUpdatedAt'] = self.now
        return [ship_event(detail), ('fleet', self.overview())]

    def start(self, detail, who):
        row = next(row for row in who['contexts'] if row['queued'])
        nxt = row['queued'].pop(0)
        row['queuedCount'] = len(row['queued'])
        branch = 'feat/' + re.sub(r'[^a-z0-9]+', '-', nxt['summary'].lower()).strip('-')[:36]
        row['current'] = dict(nxt, status='running', phase='Implementing', progress=self.PROGRESS['Implementing'],
                              startedAt=self.now, updatedAt=self.now)
        row['observed'] = True
        row['git'] = dict(row['git'] or {}, branch=branch, head=f'{self.random.getrandbits(28):07x}', base='origin/main',
                          ahead=0, behind=0, changedFiles=1, repository=row['git']['repository'] if row['git'] else detail['repository'],
                          checkedAt=self.now)
        row['pullRequest'] = None
        row['pullRequestKnown'] = True
        return [self.activity(detail, who, 'dispatch', f'{who["name"]} started “{nxt["summary"]}”', branch=branch)] + self.changed(detail, who)

    def advance(self, detail, who):
        row = next(row for row in who['contexts'] if row['current'] and row['current']['status'] == 'running')
        current = row['current']
        index = self.PHASES.index(current['phase']) if current['phase'] in self.PHASES else 0
        if index == len(self.PHASES) - 1:
            return self.complete(detail, who, row)
        phase = self.PHASES[index + 1]
        row['current'] = dict(current, phase=phase, progress=self.PROGRESS[phase], updatedAt=self.now)
        events = []
        if row['git']:
            row['git'] = dict(row['git'], ahead=(row['git']['ahead'] or 0) + 1, checkedAt=self.now,
                              changedFiles=0 if phase == 'Opening PR' else (row['git']['changedFiles'] or 0) + 1)
        if phase == 'Opening PR' and not row['pullRequest'] and row['git'] and row['git']['branch'] != 'main':
            self.pr_number += 1
            row['pullRequest'] = pull(row['git']['repository'], self.pr_number, current['summary'], 'draft', 'running')
            row['pullRequestKnown'] = True
            events.append(self.activity(detail, who, 'git', f'{who["name"]} opened draft pull request #{self.pr_number}',
                                        branch=row['git']['branch']))
        else:
            events.append(self.activity(detail, who, 'phase', f'{who["name"]} · {phase}',
                                        **({'branch': row['git']['branch']} if row['git'] else {})))
        return events + self.changed(detail, who)

    def complete(self, detail, who, row):
        finished = dict(row['current'], status='completed', phase='Done', updatedAt=self.now, finishedAt=self.now)
        row['current'] = None
        row['recent'] = [finished] + row['recent'][:7]
        row['recentCount'] += 1
        if row['pullRequest']:
            row['pullRequest'] = dict(row['pullRequest'], state='open', checks='passing')
        detail['captain']['queue']['done'] += 1
        events = [self.activity(detail, who, 'outcome', f'{who["name"]} completed “{finished["summary"]}”',
                                status='completed')] + self.changed(detail, who)
        return events + self.refill()

    def refill(self):
        """Queues pool work for an idle crew member while little is queued."""
        queued = sum(row['queuedCount'] for detail in self.ships for who in detail['crewMembers'] for row in who['contexts'])
        idle = [(detail, who) for detail in self.ships for who in detail['crewMembers'] if who['activity'] == 'idle']
        if queued >= 4 or not idle:
            return []
        detail, who = self.random.choice(idle)
        summary = self.random.choice(POOL)
        row = who['contexts'][0]
        row['queued'].append(task(self.clock, 'queued', summary, 'Waiting for a worker slot', created=self.now))
        row['queuedCount'] = len(row['queued'])
        return [self.activity(detail, who, 'queued', f'{detail["captain"]["name"]} queued “{summary}” for {who["name"]}')] + self.changed(detail, who)


# Work the captains hand out when the queues run low, so a long-running
# fixture keeps a busy fleet.
POOL = (
    'Retry telemetry uploads with backoff (NAV-5)',
    'Add waypoint search to the console (NAV-3)',
    'Detect outdated sector packs at startup (MAP-4)',
    'Cache the last viewed sector (MAP-5)',
    'Cap the retry queue on slow relays (RELAY-4)',
    'Write an operator checklist for clean rollback (RELAY-10)',
)


def auth_fingerprint(header):
    """`none`, or the first 12 hex digits of the SHA-256 of an Authorization header."""
    return hashlib.sha256(header.encode()).hexdigest()[:12] if header else 'none'


def printable(text):
    """`text` for one log line: every non-printable character and backslash escaped."""
    return ''.join(ch if ch.isprintable() and ch != '\\' else ch.encode('unicode_escape').decode('ascii')
                   for ch in text)


def target_path(target):
    """The path of request target `target`, or None when it cannot be parsed."""
    try:
        return urlsplit(target or '').path
    except ValueError:
        return None


def frame(kind, data):
    return f'event: {kind}\ndata: {json.dumps(data, separators=(",", ":"))}\n\n'.encode()


class Handler(BaseHTTPRequestHandler):
    server_version = 'StargateFixture/1'
    protocol_version = 'HTTP/1.1'

    def log_message(self, *_):
        pass

    def log_request(self, code='-', size='-'):
        # The standard library logs an error it sends before the request
        # line is read (a malformed request), when there are no headers,
        # path or method yet: those log as `-` and `auth=none`.
        if self.server.log_requests:
            headers = getattr(self, 'headers', None)
            auth = auth_fingerprint(headers.get('Authorization', '') if headers else '')
            method = printable(getattr(self, 'command', None) or '-')
            path = printable(target_path(getattr(self, 'path', '')) or '-')
            print(f'request {method} {path} {code} auth={auth}', flush=True)

    def allowed_origin(self):
        origin = self.headers.get('Origin', '')
        return origin if any(pattern.fullmatch(origin) for pattern in ORIGINS) else None

    def cors(self):
        origin = self.allowed_origin()
        if origin:
            self.send_header('Access-Control-Allow-Origin', origin)
        self.send_header('Vary', 'Origin')

    def authorised(self):
        given = self.headers.get('Authorization', '')
        return hmac.compare_digest(given.encode(), f'Bearer {self.server.token}'.encode())

    def reply(self, status, value, head=False):
        body = json.dumps(value).encode() if value is not None else b''
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Connection', 'close')
        self.cors()
        self.end_headers()
        if not head:
            self.wfile.write(body)
        self.close_connection = True

    def do_OPTIONS(self):
        origin = self.allowed_origin()
        self.send_response(204 if origin else 403)
        if origin:
            self.send_header('Access-Control-Allow-Methods', 'GET, HEAD')
            self.send_header('Access-Control-Allow-Headers', 'Authorization, Last-Event-ID')
            self.send_header('Access-Control-Max-Age', '600')
        self.cors()
        self.send_header('Content-Length', '0')
        self.send_header('Connection', 'close')
        self.end_headers()
        self.close_connection = True

    def do_HEAD(self):
        self.do_GET(head=True)

    def do_GET(self, head=False):
        path = target_path(self.path)
        if path is None:
            return self.reply(400, dict(error='Malformed request target.'), head)
        if not self.authorised():
            return self.reply(401, dict(error='A valid bearer token is required.'), head)
        parts = [unquote(part) for part in path.split('/') if part]
        if parts[:2] != ['api', 'v1']:
            return self.reply(404, dict(error='Not found.'), head)
        route = parts[2:]
        fleet = self.server.fleet
        with self.server.lock:
            fleet.set_now(time.time() * 1000)
            if route == ['fleet']:
                return self.reply(200, fleet.overview(), head)
            if len(route) == 2 and route[0] == 'ships':
                value = fleet.ship(route[1])
                return self.reply(200 if value else 404, value or dict(error='Unknown ship.'), head)
            if len(route) == 4 and route[0] == 'ships' and route[2] == 'crew':
                value = fleet.crew(route[1], route[3])
                return self.reply(200 if value else 404, value or dict(error='Unknown crew member.'), head)
            if (len(route) >= 5 and route[0] == 'ships' and route[2] == 'crew'
                    and route[4:] in (['diff'], ['diff', 'file'])):
                status, value = crew_diff(fleet, route[1], route[3], len(route) == 6, urlsplit(self.path).query)
                return self.reply(status, value, head)
        if route == ['events'] and not head and self.server.refuse_events:
            # Stargate serves at most eight streams and answers 503 beyond.
            return self.busy()
        if route == ['events'] and not head:
            return self.stream()
        return self.reply(404, dict(error='Not found.'), head)

    def busy(self):
        body = json.dumps(dict(error='Too many event streams.')).encode()
        self.send_response(503)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Retry-After', '5')
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Connection', 'close')
        self.cors()
        self.end_headers()
        self.wfile.write(body)
        self.close_connection = True

    def stream(self):
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Connection', 'close')
        self.cors()
        self.end_headers()
        self.close_connection = True
        inbox = self.server.subscribe()
        try:
            with self.server.lock:
                self.server.fleet.set_now(time.time() * 1000)
                first = b''.join(frame(kind, data) for kind, data in self.server.fleet.initial())
            self.wfile.write(first)
            self.wfile.flush()
            while not self.server.closing.is_set():
                try:
                    chunk = inbox.get(timeout=0.5)
                except queue.Empty:
                    continue
                self.wfile.write(chunk)
                self.wfile.flush()
        except OSError:
            pass
        finally:
            self.server.unsubscribe(inbox)


class FleetServer(ThreadingHTTPServer):
    daemon_threads = True
    # socketserver's default listen queue of 5 overflows when a client's
    # burst arrives while the accept loop is behind, and macOS resets each
    # connection past it. Hold a burst instead (capped at somaxconn).
    request_queue_size = 128

    def __init__(self, port, token, tick=5.0, now=None, refuse_events=False, log_requests=False):
        if not token:
            raise ValueError('A non-empty fixture token is required.')
        self.token = token
        self.log_requests = log_requests
        self.fleet = Fleet(now)
        self.lock = threading.RLock()
        self.subscribers = set()
        self.closing = threading.Event()
        self.tick = tick
        self.refuse_events = refuse_events
        super().__init__(('127.0.0.1', port), Handler)
        self.origin = f'http://127.0.0.1:{self.server_address[1]}'
        threading.Thread(target=self.simulate, daemon=True).start()

    def subscribe(self):
        inbox = queue.Queue()
        with self.lock:
            self.subscribers.add(inbox)
        return inbox

    def unsubscribe(self, inbox):
        with self.lock:
            self.subscribers.discard(inbox)

    def publish(self, events):
        chunk = b''.join(frame(kind, data) for kind, data in events)
        if not chunk:
            return
        with self.lock:
            for inbox in self.subscribers:
                inbox.put(chunk)

    def simulate(self):
        started = time.monotonic()
        next_tick, next_host, next_beat = self.tick, HOST_INTERVAL, HEARTBEAT_INTERVAL
        while not self.closing.wait(0.25):
            elapsed = time.monotonic() - started
            events = []
            with self.lock:
                self.fleet.set_now(time.time() * 1000)
                if self.tick > 0 and elapsed >= next_tick:
                    next_tick += self.tick
                    events += self.fleet.tick()
                if elapsed >= next_host:
                    next_host += HOST_INTERVAL
                    events += self.fleet.host_sample()
                if elapsed >= next_beat:
                    next_beat += HEARTBEAT_INTERVAL
                    events.append(('heartbeat', dict(at=self.fleet.now)))
            self.publish(events)

    def shutdown(self):
        self.closing.set()
        super().shutdown()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--port', type=int, default=18484)
    parser.add_argument('--token', default='fixture-fleet', help='The fictional bearer token every request needs.')
    parser.add_argument('--tick', type=float, default=5.0, metavar='SECONDS',
                        help='Seconds between simulated crew changes; 0 keeps the fleet still.')
    parser.add_argument('--refuse-events', action='store_true',
                        help='Answer the event stream with 503, as Stargate does with eight streams open.')
    parser.add_argument('--log-requests', action='store_true',
                        help="Print each request's method, path, status and Authorization fingerprint.")
    parser.add_argument('--snapshot', action='store_true',
                        help='Print the fleet, every ship and every crew member as one JSON document and exit.')
    options = parser.parse_args()
    if options.snapshot:
        print(json.dumps(Fleet().snapshot(), indent=2))
        return
    server = FleetServer(options.port, options.token, options.tick, refuse_events=options.refuse_events,
                         log_requests=options.log_requests)
    try:
        print(f'Stargate Fleet API fixture on {server.origin}', flush=True)
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == '__main__':
    main()
