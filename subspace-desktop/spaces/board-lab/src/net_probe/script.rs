pub(super) fn fixture_script(proxy_port: u16, origin_port: u16) -> String {
    format!(
        r#"import errno,http.client,json,socket
proxy_port={proxy_port}
origin_port={origin_port}
connection=http.client.HTTPConnection('127.0.0.1',proxy_port,timeout=3)
connection.request('GET',f'http://127.0.0.1:{{origin_port}}/data')
response=connection.getresponse()
assert response.status==200, response.status
data=json.loads(response.read())
assert data['fixture'] is True
connection.close()
connection=http.client.HTTPConnection('127.0.0.1',proxy_port,timeout=3)
connection.request('GET','http://forbidden.invalid/data')
response=connection.getresponse()
assert response.status==403, response.status
response.read(); connection.close()
try:
    socket.create_connection(('127.0.0.1',origin_port),1)
except OSError as error:
    assert error.errno in (errno.EPERM,errno.EACCES), error
else:
    raise RuntimeError('Direct origin connection escaped the proxy-only policy')
tunnel=http.client.HTTPConnection('127.0.0.1',proxy_port,timeout=3)
tunnel.set_tunnel('127.0.0.1',origin_port)
tunnel.request('GET','/data')
response=tunnel.getresponse()
assert response.status==200, response.status
response.read(); tunnel.close()
print(json.dumps(dict(allowed_request=True,forbidden_destination_denied=True,direct_connection_denied=True,connect_tunnel=True,fixture_only=True)))
"#
    )
}
