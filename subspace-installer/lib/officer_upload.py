"""Publish a file explicitly staged in the officer's private /tmp to the configured Subspace API."""
import base64
import hashlib
import http.client
import json
import os
import re
import socket
import stat
import sys
import uuid
from urllib.parse import urlsplit

TYPES = {
    'md': 'text/markdown', 'markdown': 'text/markdown', 'txt': 'text/plain', 'text': 'text/plain',
    'png': 'image/png', 'jpg': 'image/jpeg',
    'jpeg': 'image/jpeg', 'gif': 'image/gif', 'webp': 'image/webp', 'pdf': 'application/pdf',
    'docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    'xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', 'zip': 'application/zip',
}
TOOL = {'name': 'upload_file', 'description': 'Upload a file explicitly staged at /tmp/subspace-share-<fileName> to temporary chat storage. '
        'All active members can read it for 30 days. Include the returned UPL reference only in the authorised IRC reply. Reuse uploadId on retry.',
        'inputSchema': {'type': 'object', 'properties': {'fileName': {'type': 'string'}, 'uploadId': {'type': 'string', 'format': 'uuid'}},
                        'required': ['fileName', 'uploadId'], 'additionalProperties': False}}


def staged(name, limit, directory='/tmp'):
    if not isinstance(name, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9 _.-]{0,179}', name):
        raise ValueError('Choose a plain file name without a directory.')
    mime = TYPES.get(name.rsplit('.', 1)[-1].lower())
    if not mime:
        raise ValueError('Choose a supported chat file type.')
    fd = os.open(os.path.join(directory, 'subspace-share-' + name), os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as f:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1:
            raise ValueError('Stage an officer-owned regular file in private /tmp.')
        if not 0 < info.st_size <= limit:
            raise ValueError(f'Stage a file of 1 to {limit} bytes.')
        data = f.read(info.st_size + 1)
        if len(data) != info.st_size or os.fstat(fd).st_mtime_ns != info.st_mtime_ns:
            raise ValueError('The staged file changed. Finish writing it before upload.')
    return mime, data


def post(mode, endpoint, actor, path, payload, extra=None):
    headers = {'Content-Type': 'application/json', 'Accept': 'application/json, text/event-stream',
               'User-Agent': 'subspace-officer-uploads/1', **(extra or {})}
    if mode == 'socket':
        connection = http.client.HTTPConnection('localhost', timeout=90)
        connection.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        connection.sock.settimeout(90)
        connection.sock.connect(endpoint)
    else:
        url = urlsplit(endpoint)
        factory = {'http': http.client.HTTPConnection, 'https': http.client.HTTPSConnection}[url.scheme]
        connection = factory(url.hostname, url.port, timeout=90)
        headers['X-Subspace-Agent'] = actor
    try:
        connection.request('POST', path, json.dumps(payload).encode(), headers)
        answer = connection.getresponse()
        if answer.status == 409:
            raise ValueError('Check the upload UUID with subspace_get_chat_upload. Reuse a matching ready reference; if the ID is still unavailable after checking, use a new UUID.')
        if not 200 <= answer.status < 300:
            raise ValueError(f'Subspace returned HTTP {answer.status}. Check the upload ID before retrying.')
        raw = answer.read(65537)
        if len(raw) > 65536:
            raise ValueError('Subspace returned an oversized receipt.')
        if not raw:
            return {}
        if answer.getheader('Content-Type','').startswith('text/event-stream'):
            raw = next(line[5:].strip() for line in raw.splitlines() if line.startswith(b'data:'))
        return json.loads(raw)
    finally:
        connection.close()


def upload(args, mode, endpoint, actor):
    name, upload_id = args['fileName'], str(uuid.UUID(str(args['uploadId'])))
    mime, data = staged(name, 6_000_000 if mode == 'socket' else 25_000_000)
    payload = {'fileName': name, 'uploadId': upload_id, 'contentType': mime, 'base64': base64.b64encode(data).decode()}
    receipt = post(mode, endpoint, actor, '/api/v1/chat/uploads', payload)
    reference = 'UPL-' + base64.urlsafe_b64encode(uuid.UUID(upload_id).bytes).decode().rstrip('=')
    if (not isinstance(receipt, dict) or receipt.get('reference') != reference or receipt.get('sizeBytes') != len(data)
            or str(receipt.get('sha256','')).lower() != hashlib.sha256(data).hexdigest()):
        raise ValueError('Upload receipt could not be verified. Check the upload ID before retrying.')
    return {'reference': reference, 'fileName': name, 'sizeBytes': len(data)}


def serve(mode, endpoint, actor):
    while line := sys.stdin.buffer.readline(8193):
        if len(line) > 8192:
            return
        request = {}
        try:
            request = json.loads(line)
            if 'id' not in request:
                continue
            method = request['method']
            if method == 'initialize':
                version = request['params'].get('protocolVersion')
                result = {'protocolVersion': version if version in ('2025-11-25','2025-06-18','2025-03-26') else '2025-03-26',
                          'capabilities': {'tools': {}}, 'serverInfo': {'name': 'officer-files', 'version': '1'}}
            elif method == 'tools/list':
                result = {'tools': [TOOL]}
            elif method == 'ping':
                result = {}
            elif method == 'tools/call' and request['params'].get('name') == TOOL['name']:
                try:
                    result = {'content': [{'type': 'text', 'text': json.dumps(upload(request['params']['arguments'], mode, endpoint, actor))}]}
                except (OSError, ValueError, KeyError, TypeError, StopIteration, http.client.HTTPException) as error:
                    # Remote payloads and local bytes are never sent back in diagnostics.
                    detail = str(error) if type(error) is ValueError else 'Check the staged file and Subspace connection.'
                    result = {'isError': True, 'content': [{'type': 'text', 'text': detail}]}
            else:
                raise ValueError('Unknown method')
            response = {'result': result}
        except (KeyError, TypeError, ValueError, AttributeError):
            response = {'error': {'code': -32600, 'message': 'Invalid request'}}
        print(json.dumps({'jsonrpc': '2.0', 'id': request.get('id') if isinstance(request, dict) else None, **response}), flush=True)


if __name__ == '__main__':
    serve(*sys.argv[1:])
