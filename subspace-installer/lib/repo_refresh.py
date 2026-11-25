"""Refresh approved Git origin refs outside the engine's read-only mount."""
from fnmatch import fnmatchcase
import json
import os
import pwd
from pathlib import Path
import re
import socket
import struct
import subprocess
import sys

from workspaces import private_credentials

SOCKET = '/run/subspace-fleet-repo-refresh.sock'
STATE = Path('/var/lib/subspace-fleet')
POLICY = Path('/etc/subspace-fleet/repo-refresh.json')
OWNER = 'ss-fleet-repos'
GITHUB_HELPER = Path(__file__).resolve().with_name('github_app.py')
REFRESH = 'refresh_origin'
READ = 'read_origin_file'
REPOSITORY = re.compile(r'[a-z][a-z0-9_-]{0,39}')
REFSPEC = '+refs/heads/*:refs/remotes/origin/*'


def valid_repository(value):
    return isinstance(value, str) and REPOSITORY.fullmatch(value) is not None


def validate_arguments(tool, args):
    if not isinstance(args, dict) or not valid_repository(args.get('repository')):
        return False
    if tool == REFRESH:
        return set(args) == {'repository'}
    branch = args.get('branch')
    file = args.get('path')
    line = args.get('startLine', 1)
    return (
        tool == READ and set(args) <= {'repository', 'branch', 'path', 'startLine'}
        and isinstance(branch, str) and 0 < len(branch) <= 200 and branch != "HEAD"
        and isinstance(file, str) and 0 < len(file) <= 500
        and all(part not in ('', '.', '..') for part in file.split('/'))
        and not any(ord(c) < 32 or c in (':', chr(92)) for c in file)
        and type(line) is int and 0 < line <= 500_000
    )


def read_origin_file(git, branch, path, start_line=1):
    for part in path.split('/'):
        if part != '.env.example' and any(fnmatchcase(part.lower(), pattern)
                                          for pattern in ('*.env', '*.env.*', '*.pem', '*.key')):
            raise ValueError('The file matches a restricted credential filename')
    ref = f'refs/remotes/origin/{branch}'
    git('check-ref-format', ref)
    commit = git('rev-parse', '--verify', f'{ref}^{{commit}}')
    blob = f'{commit}:{path}'
    if git('cat-file', '-t', blob) != 'blob' or int(git('cat-file', '-s', blob)) > 500_000:
        raise ValueError('The path must name a text file no larger than 500 KB')
    content = git('cat-file', 'blob', blob)
    if chr(0) in content:
        raise ValueError('The file is binary')
    lines = content.splitlines()
    chunk = []
    remaining = 4000
    for text in lines[start_line - 1:start_line + 78]:
        line = text if len(text) <= 2000 else text[:2000] + ' [line shortened]'
        encoded_length = len(json.dumps(line))
        if chunk and encoded_length > remaining:
            break
        chunk.append(line)
        remaining -= encoded_length
    following = start_line + len(chunk)
    return {'commit': commit, 'startLine': start_line, 'lines': chunk,
            'nextLine': following if following <= len(lines) else None}


def refresh_refs(git, origin, *, expected_origin=None):
    if git('remote', 'get-url', 'origin') != (expected_origin or origin):
        raise ValueError('The checkout origin does not match the configured repository')
    head = git('rev-parse', 'HEAD')
    refs = ('for-each-ref', '--format=%(if)%(symref)%(then)%(else)%(refname:strip=2) %(objectname)%(end)', 'refs/remotes/origin/')
    before = dict(line.split(' ', 1) for line in git(*refs).splitlines() if line)
    git('fetch', '--atomic', '--no-tags', '--no-recurse-submodules',
        '--no-write-fetch-head', '--no-auto-maintenance', '--prune', '--no-prune-tags',
        '--refmap=', '--', origin, REFSPEC)
    if git('rev-parse', 'HEAD') != head:
        raise ValueError('The checkout changed during the refresh')
    after = dict(line.split(' ', 1) for line in git(*refs).splitlines() if line)
    updated = [f'{name} {commit}' for name, commit in after.items() if before.get(name) != commit]
    deleted = sorted(before.keys() - after.keys())
    return {'checkoutHead': head, 'updatedCount': len(updated), 'updatedRefs': updated[:25],
            'deletedCount': len(deleted), 'deletedRefs': deleted[:25]}


def run_request(policy, tool, args):
    repository = args['repository']
    repo = policy['repositories'].get(repository)
    if repo is None:
        raise ValueError('Unknown configured repository')
    rear = policy['rearAdmiral']
    checkout = STATE / 'references' / rear / repository
    origin = repo['url']
    expected_origin = origin if repo['private'] else str(STATE / 'repositories' / f'{repository}.git')
    git_dir = checkout / '.git'
    if not git_dir.is_dir() or checkout.resolve() != checkout or git_dir.resolve() != git_dir:
        raise ValueError('The configured reference checkout is missing or redirected')
    owner = pwd.getpwnam(OWNER).pw_uid
    if checkout.stat().st_uid != owner or git_dir.stat().st_uid != owner:
        raise ValueError('The reference checkout belongs to another account')
    if repo['private'] and tool == REFRESH:
        credentials = private_credentials(GITHUB_HELPER, origin, f'ss-{rear}')
    else:
        credentials = {'GIT_CONFIG_COUNT': '1', 'GIT_CONFIG_KEY_0': 'credential.helper', 'GIT_CONFIG_VALUE_0': ''}
    env = {
        'PATH': '/usr/bin:/bin', 'GIT_TERMINAL_PROMPT': '0',
        'GIT_CONFIG_NOSYSTEM': '1', 'GIT_CONFIG_GLOBAL': '/dev/null', **credentials,
    }

    def git(*args):
        result = subprocess.check_output(
            ['/usr/sbin/runuser', '-u', OWNER, '--', '/usr/bin/git',
             '-c', 'core.hooksPath=/dev/null', '-c', 'core.fsmonitor=false',
             '-c', 'http.followRedirects=false', '-c', 'protocol.allow=never',
             '-c', 'protocol.https.allow=always', '-c', 'protocol.http.allow=always',
             '-c', 'fetch.writeCommitGraph=false', '-C', str(checkout), *args],
            env=env, stderr=subprocess.DEVNULL, text=True, timeout=90, umask=0o022,
        )
        return result if args[:2] == ('cat-file', 'blob') else result.strip()

    if git('rev-parse', '--absolute-git-dir') != str(git_dir):
        raise ValueError('The checkout does not own its Git metadata')
    result = (refresh_refs(git, origin, expected_origin=expected_origin) if tool == REFRESH else
              read_origin_file(git, args['branch'], args['path'], args.get('startLine', 1)))
    return {'repository': repository, **result}


def serve():
    policy = json.loads(POLICY.read_bytes())
    with socket.socket(fileno=os.dup(0)) as connection:
        credentials = connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12)
        _, uid, _ = struct.unpack('3i', credentials)
        if uid != pwd.getpwnam(f"ss-{policy['rearAdmiral']}").pw_uid:
            return
        connection.settimeout(5)
        with connection.makefile('rb') as stream:
            try:
                line = stream.readline(4097)
                if len(line) > 4096 or not line.endswith(b'\n'):
                    raise ValueError('Invalid refresh request')
                request = json.loads(line)
                tool, args = request['tool'], request['args']
                if not validate_arguments(tool, args):
                    raise ValueError('Invalid repository request')
                result = run_request(policy, tool, args)
            except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError):
                result = {'error': 'Repository operation failed. Check the repository key, branch and path or report the error to the operator.'}
        try:
            connection.sendall(json.dumps(result).encode() + b'\n')
        except OSError:
            pass


def ask(tool, args):
    if not validate_arguments(tool, args):
        return {'error': 'Use a configured repository key and a relative file path for reads.'}
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(105)
            connection.connect(SOCKET)
            connection.sendall(json.dumps({'tool': tool, 'args': args}).encode() + b'\n')
            with connection.makefile('rb') as stream:
                return json.loads(stream.readline(32_769))
    except (OSError, ValueError):
        return {'error': 'The repository tool is unavailable. Check the origin refs before repeating a refresh.'}


def message(request):
    method = request.get('method')
    params = request.get('params') or {}
    if method == 'initialize':
        version = params.get('protocolVersion')
        return {'protocolVersion': version if version in ('2024-11-05', '2025-03-26', '2025-06-18') else '2025-06-18',
                'capabilities': {'tools': {}}, 'serverInfo': {'name': 'repository-refresh', 'version': '1'}}
    if method == 'ping':
        return {}
    if method == 'tools/list':
        string = {'type': 'string'}
        return {'tools': [
            {'name': REFRESH, 'description': 'Fetch current origin refs for an assigned repository.',
             'inputSchema': {'type': 'object', 'properties': {'repository': string},
                             'required': ['repository'], 'additionalProperties': False}},
            {'name': READ, 'description': 'Read lines from a file at a fetched origin branch. Pass nextLine as startLine for more.',
             'inputSchema': {'type': 'object',
                             'properties': {'repository': string, 'branch': string, 'path': string,
                                            'startLine': {'type': 'integer', 'minimum': 1}},
                             'required': ['repository', 'branch', 'path'], 'additionalProperties': False}},
        ]}
    if method == 'tools/call' and params.get('name') in (REFRESH, READ):
        result = ask(params['name'], params.get('arguments'))
        return {'content': [{'type': 'text', 'text': json.dumps(result)}], 'isError': 'error' in result}
    raise ValueError('Unsupported request method')


def mcp():
    while line := sys.stdin.buffer.readline(32_769):
        if len(line) > 32_768:
            return
        request = None
        try:
            request = json.loads(line)
            if isinstance(request, dict) and 'id' not in request:
                continue
            result = message(request)
            response = {'result': result}
        except (AttributeError, TypeError, ValueError):
            response = {'error': {'code': -32600, 'message': 'Invalid request'}}
        print(json.dumps({'jsonrpc': '2.0', 'id': request.get('id') if isinstance(request, dict) else None,
                          **response}), flush=True)


if __name__ == '__main__':
    if sys.argv[1:] == ['serve']:
        serve()
    elif sys.argv[1:] == ['mcp']:
        mcp()
    else:
        raise SystemExit('Choose serve or mcp.')
