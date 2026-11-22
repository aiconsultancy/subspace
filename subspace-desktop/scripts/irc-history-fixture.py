#!/usr/bin/env python3
"""Opens the aurora fleet's channels on a loopback IRC test server and posts a
fictional Fallow Reach conversation in the Lyra ship's channel.

For Channels screenshots: start `scripts/irc-test-server.sh`, run this, then
capture with `CAPTURE_IRC_CONFIG` naming that server and `CAPTURE_FLEET=1`, so
the issue identifiers resolve through the Issues Space and the pull request
links through the GitHub fixture. Each speaker connects under its own nick,
joins the channel, and stays connected (answering PING) until interrupted, so
the member list is populated and the server keeps the lines as chat history.
The agents (`aurora-*`) register and sign in to the account of their own nick
before they join, as fleet servers require, so their lines carry the account
that Channels needs before it marks a rank; people stay signed out.

  python3 scripts/irc-history-fixture.py --server 127.0.0.1:16667

With --late-after <s>, three more lines reach `#aurora-lyra` that many seconds
after the capture build's nick (--nick, default `subspace`) joins it, so a
capture that has another channel in front meanwhile shows them as new. With
--status, `aurora-lyra` sets its nick-wide status through `draft/metadata-2`,
as an agent does while it works.
"""

import argparse
import re
import socket
import threading
import time

# The aurora fleet's channels, as the Fleet API fixture names its ships: the
# Starbase, the command channel and one channel per ship (captain). Only the Lyra
# ship's channel has a conversation.
CHANNEL = '#aurora-lyra'
TOPICS = {
    '#aurora-starbase': 'Aurora Starbase: people, the rear admiral and every captain',
    '#aurora-command': 'Aurora command: the rear admiral and its captains',
    '#aurora-lyra': 'Lyra: navigation console, Fallow Reach pilot review on Thursday',
    '#aurora-orion': 'Orion: sector atlas and relay handover',
    '#aurora-vega': 'Vega: Fallow Reach rehearsal scripts and runbooks',
    '#aurora-castor': 'Castor: chart performance and relay status panel',
}
LINES = (
    ('mira', 'Morning, aurora-lyra. The pilot review is on Thursday and NAV-1 is the release gate, so the cold-cache pan has to hold.'),
    ('aurora-lyra', 'Wren has NAV-7 up for review: https://github.com/acme/web/pull/492 Decode jobs now carry the viewport epoch.'),
    ('tavi', 'The baseline from NAV-6 still stands: 41 ms warm, 87 ms on the first seam crossing.'),
    ('aurora-lyra', 'Castor re-ran the cold pass on the pilot laptop with the worker pool from NAV-1. The trace is on the issue.'),
    ('sela', 'Focus order for NAV-11 looks right in the preview. Approved https://github.com/acme/web/pull/489'),
    ('mira', 'Thanks. NAV-2 is next once NAV-7 merges; the preview still ignores relay drift.'),
    ('aurora-lyra', 'Queued NAV-2 for Juniper behind NAV-8. Both will follow the drift overlay decision.'),
    ('aurora-admiral', 'Noted for the Thursday review. Raise NAV-1 in the Starbase if the cold pass slips.'),
)
# With --late-after: what arrives after the capture build has joined.
LATE = (
    ('sela', 'Focus order for NAV-11 still looks right after the rebase.'),
    ('mira', '@aurora-lyra can you take NAV-2 next, once NAV-7 merges?'),
    ('aurora-lyra', 'Accepted. Handing NAV-2 to Juniper now.'),
)
# With --repeats: NAV-1 and pull request 492 named again, and two references
# that cannot be resolved.
REPEATS = (
    ('tavi', 'Pulled https://github.com/acme/web/pull/492 onto the pilot laptop: NAV-1 pans hold at 41 ms warm.'),
    ('sela', 'I could not open NAV-99 or https://github.com/acme/vault/pull/7 from here.'),
    ('mira', 'Then https://github.com/acme/web/pull/492 merges first, and NAV-1 closes after the review.'),
)
# With --repeats and --late-after: a late pull request, whose tile loads in place.
LATE_REPEATS = (
    ('aurora-lyra', 'Juniper opened https://github.com/acme/web/pull/491 for the worker pool.'),
)
STATUS = 'Lyra is delegating NAV-2 to Juniper'
# With --thread: two requests to the captain, the steps it reports on each
# (seconds before each), and the replies in each thread. A reply to `reply`
# answers the earlier reply of that index instead of the request.
PROGRESS = '+stargate/progress'
THREADS = (
    {
        'ask': 'aurora-lyra: capture a chart-pan baseline on the pilot laptop before the review.',
        'steps': (('accepted', 1), ('reasoning', 2), ('tool', 3), ('reply', 3), ('completed', 2)),
        'replies': (
            ('aurora-lyra', 'Baseline captured: 41 ms warm, 87 ms on the first seam crossing. Wren closed NAV-6.', None),
            ('tavi', 'Matches what I saw on the pilot laptop this morning.', None),
            ('mira', 'Thanks. Then NAV-6 is settled for the review.', 0),
        ),
    },
    {
        'ask': 'aurora-lyra: can you take NAV-2 next, once NAV-7 merges? The preview still ignores relay drift.',
        'steps': (('accepted', 1), ('model', 2), ('tool', 3)),
        'replies': (
            ('aurora-lyra', 'Accepted. Juniper starts on NAV-2 once the drift overlay lands.', None),
        ),
    },
)

SPEAKERS = ('mira', 'tavi', 'sela', 'aurora-lyra', 'aurora-admiral', 'aurora-orion', 'aurora-vega', 'aurora-castor')


def channels_for(nick):
    """The channels each fixture nick sits in."""
    ship = f'#{nick}' if nick in ('aurora-lyra', 'aurora-orion', 'aurora-vega', 'aurora-castor') else None
    command = nick.startswith('aurora-')
    return [name for name in TOPICS if name == '#aurora-starbase' or (command and name == '#aurora-command')
            or name == ship or (name == CHANNEL and nick in dict(LINES))]


def is_agent(nick):
    return nick.startswith('aurora-')


def sign_in(nick):
    """NickServ lines that register `nick`'s account on the fresh test server,
    or sign in to it when a previous run registered it. Fixture-only words."""
    secret = f'{nick}-fixture'
    return f'PRIVMSG NickServ :REGISTER {secret}\r\nPRIVMSG NickServ :IDENTIFY {nick} {secret}\r\n'


def connect(server, nick, caps=()):
    host, port = server.rsplit(':', 1)
    sock = socket.create_connection((host, int(port)), timeout=10)
    # Requested before NICK, so registration waits for CAP END.
    request = f'CAP REQ :{" ".join(caps)}\r\n' if caps else ''
    finish = 'CAP END\r\n' if caps else ''
    sock.sendall(f'{request}NICK {nick}\r\nUSER {nick} 0 * :{nick}\r\n{finish}'.encode())
    buffer = b''
    while b' 001 ' not in buffer:
        chunk = sock.recv(4096)
        if not chunk:
            raise SystemExit(f'{nick}: the server closed the connection')
        buffer += chunk
        for line in buffer.split(b'\r\n'):
            if line.startswith(b'PING'):
                sock.sendall(b'PONG' + line[4:] + b'\r\n')
    if is_agent(nick):
        sock.sendall(sign_in(nick).encode())
    sock.sendall(f'JOIN {",".join(channels_for(nick))}\r\n'.encode())
    sock.settimeout(None)
    return sock


def keep_alive(sock):
    try:
        while chunk := sock.recv(4096):
            for line in chunk.split(b'\r\n'):
                if line.startswith(b'PING'):
                    sock.sendall(b'PONG' + line[4:] + b'\r\n')
    except OSError:
        pass


def late_lines(sock, socks, nick, delay, late):
    """Waits for `nick` to join the ship channel on `sock`, then, after
    `delay` seconds, posts the late lines there."""
    joined = f' JOIN {CHANNEL}'.encode()
    buffer = b''
    try:
        while chunk := sock.recv(4096):
            buffer += chunk
            lines = buffer.split(b'\r\n')
            buffer = lines.pop()
            for line in lines:
                if line.startswith(b'PING'):
                    sock.sendall(b'PONG' + line[4:] + b'\r\n')
                elif line.startswith(f':{nick}!'.encode()) and line.endswith(joined):
                    time.sleep(delay)
                    for speaker, text in late:
                        socks[speaker].sendall(f'PRIVMSG {CHANNEL} :{text}\r\n'.encode())
                        time.sleep(0.4)
                    print(f'posted {len(late)} late lines to {CHANNEL}', flush=True)
    except OSError:
        pass


def await_msgid(sock, text):
    """Reads `sock` until the server echoes `text` back, and returns the
    message ID it gave it. Needs `echo-message` and `message-tags`."""
    buffer = b''
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            raise SystemExit('the server closed the connection before echoing the request')
        buffer += chunk
        lines = buffer.split(b'\r\n')
        buffer = lines.pop()
        for line in lines:
            if line.startswith(b'PING'):
                sock.sendall(b'PONG' + line[4:] + b'\r\n')
                continue
            decoded = line.decode(errors='replace')
            if f'PRIVMSG {CHANNEL} :{text}' in decoded and (found := re.search(r'msgid=([^; ]+)', decoded)):
                return found.group(1)


def play_threads(socks):
    """Each request in THREADS, the captain's progress on it and its replies."""
    for thread in THREADS:
        socks['mira'].sendall(f'PRIVMSG {CHANNEL} :{thread["ask"]}\r\n'.encode())
        request = await_msgid(socks['mira'], thread['ask'])
        for stage, wait in thread['steps']:
            time.sleep(wait)
            tags = f'@{PROGRESS}={stage};+draft/reply={request}'
            socks['aurora-lyra'].sendall(f'{tags} NOTICE {CHANNEL} :Progress: {stage}\r\n'.encode())
        replies = []
        for speaker, text, parent in thread['replies']:
            time.sleep(1.2)
            to = replies[parent] if parent is not None else request
            socks[speaker].sendall(f'@+draft/reply={to} PRIVMSG {CHANNEL} :{text}\r\n'.encode())
            replies.append(await_msgid(socks[speaker], text))
    print(f'played {len(THREADS)} threads in {CHANNEL}', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--server', default='127.0.0.1:16667')
    parser.add_argument('--nick', default='subspace', help="the capture build's nick")
    parser.add_argument('--late-after', type=float, help='seconds after the capture build joins')
    parser.add_argument('--status', action='store_true', help="set aurora-lyra's status")
    parser.add_argument('--repeats', action='store_true', help='name NAV-1 and pull request 492 again')
    parser.add_argument('--thread', action='store_true', help='play two requests with progress and replies')
    options = parser.parse_args()
    if not options.server.startswith('127.0.0.1:'):
        raise SystemExit('Use a loopback server, 127.0.0.1:<port>.')
    caps = {'aurora-lyra': ('draft/metadata-2',)} if options.status else {}
    if options.thread:
        tagged = ('message-tags', 'echo-message')
        for nick in ('mira', 'tavi', 'aurora-lyra'):
            caps[nick] = tuple(caps.get(nick, ())) + tagged
    socks = {nick: connect(options.server, nick, caps.get(nick, ())) for nick in SPEAKERS}
    if options.status:
        socks['aurora-lyra'].sendall(f'METADATA * SET status :{STATUS}\r\n'.encode())
    time.sleep(1)
    for name, topic in TOPICS.items():
        # The first to join holds the channel's operator status, which +t needs.
        first = next(nick for nick in SPEAKERS if name in channels_for(nick))
        socks[first].sendall(f'TOPIC {name} :{topic}\r\n'.encode())
    for nick, text in LINES + (REPEATS if options.repeats else ()):
        socks[nick].sendall(f'PRIVMSG {CHANNEL} :{text}\r\n'.encode())
        time.sleep(1.2)
    print(f'posted the lines to {CHANNEL}', flush=True)
    if options.thread:
        play_threads(socks)
    watcher = 'mira' if options.late_after is not None else None
    threads = [threading.Thread(target=keep_alive, args=(sock,), daemon=True)
               for nick, sock in socks.items() if nick != watcher]
    if watcher:
        threads.append(threading.Thread(target=late_lines, daemon=True,
                                        args=(socks[watcher], socks, options.nick, options.late_after,
                                              LATE + (LATE_REPEATS if options.repeats else ()))))
    for thread in threads:
        thread.start()
    try:
        for thread in threads:
            thread.join()
    except KeyboardInterrupt:
        pass


if __name__ == '__main__':
    main()
