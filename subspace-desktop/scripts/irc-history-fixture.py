#!/usr/bin/env python3
"""Opens the aurora fleet's channels on a loopback IRC test server and posts a
fictional Fallow Reach conversation in the Lyra ship's channel.

For Channels screenshots: start `scripts/irc-test-server.sh`, run this, then
capture with `CAPTURE_IRC_CONFIG` naming that server and `CAPTURE_FLEET=1`, so
the issue identifiers resolve through the Issues Space and the pull request
links through the GitHub fixture. Each speaker connects under its own nick,
joins the channel, and stays connected (answering PING) until interrupted, so
the member list is populated and the server keeps the lines as chat history.

  python3 scripts/irc-history-fixture.py --server 127.0.0.1:16667
"""

import argparse
import socket
import threading
import time

# The aurora fleet's channels, as the Fleet API fixture names its ships: the
# Starbase, fleet command and one channel per ship (captain). Only the Lyra
# ship's channel has a conversation.
CHANNEL = '#aurora-lyra'
TOPICS = {
    '#aurora-starbase': 'Aurora Starbase: people, the rear admiral and every captain',
    '#aurora-fleet-command': 'Aurora fleet command: the rear admiral and its captains',
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
)
SPEAKERS = ('mira', 'tavi', 'sela', 'aurora-lyra', 'aurora-admiral', 'aurora-orion', 'aurora-vega', 'aurora-castor')


def channels_for(nick):
    """The channels each fixture nick sits in."""
    ship = f'#{nick}' if nick in ('aurora-lyra', 'aurora-orion', 'aurora-vega', 'aurora-castor') else None
    command = nick.startswith('aurora-')
    return [name for name in TOPICS if name == '#aurora-starbase' or (command and name == '#aurora-fleet-command')
            or name == ship or (name == CHANNEL and nick in dict(LINES))]


def connect(server, nick):
    host, port = server.rsplit(':', 1)
    sock = socket.create_connection((host, int(port)), timeout=10)
    sock.sendall(f'NICK {nick}\r\nUSER {nick} 0 * :{nick}\r\n'.encode())
    buffer = b''
    while b' 001 ' not in buffer:
        chunk = sock.recv(4096)
        if not chunk:
            raise SystemExit(f'{nick}: the server closed the connection')
        buffer += chunk
        for line in buffer.split(b'\r\n'):
            if line.startswith(b'PING'):
                sock.sendall(b'PONG' + line[4:] + b'\r\n')
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


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--server', default='127.0.0.1:16667')
    options = parser.parse_args()
    if not options.server.startswith('127.0.0.1:'):
        raise SystemExit('Use a loopback server, 127.0.0.1:<port>.')
    socks = {nick: connect(options.server, nick) for nick in SPEAKERS}
    time.sleep(1)
    for name, topic in TOPICS.items():
        # The first to join holds the channel's operator status, which +t needs.
        first = next(nick for nick in SPEAKERS if name in channels_for(nick))
        socks[first].sendall(f'TOPIC {name} :{topic}\r\n'.encode())
    for nick, text in LINES:
        socks[nick].sendall(f'PRIVMSG {CHANNEL} :{text}\r\n'.encode())
        time.sleep(1.2)
    print(f'posted {len(LINES)} lines to {CHANNEL}', flush=True)
    threads = [threading.Thread(target=keep_alive, args=(sock,), daemon=True) for sock in socks.values()]
    for thread in threads:
        thread.start()
    try:
        for thread in threads:
            thread.join()
    except KeyboardInterrupt:
        pass


if __name__ == '__main__':
    main()
