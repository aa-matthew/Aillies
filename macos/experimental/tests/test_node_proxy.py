"""Test standard Node proxy handling for HTTPS upgrades; not a live ChatGPT test."""
import datetime
import http.server
import ipaddress
import os
import pathlib
import select
import socket
import socketserver
import ssl
import subprocess
import tempfile
import threading
import time
import unittest


class NodeProxyFixture(unittest.TestCase):
    def test_https_upgrade_streams_through_proxy_and_fails_closed(self):
        node = os.environ.get("PEER_ROUTE_NODE_FIXTURE")
        if not node: self.skipTest("Set PEER_ROUTE_NODE_FIXTURE to Node 24.5+ for this optional runtime fixture")
        from cryptography import x509
        from cryptography.hazmat.primitives import hashes, serialization
        from cryptography.hazmat.primitives.asymmetric import rsa
        from cryptography.x509.oid import NameOID
        with tempfile.TemporaryDirectory() as directory:
            d = pathlib.Path(directory)
            key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
            name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "Local Node fixture")])
            now = datetime.datetime.now(datetime.timezone.utc)
            cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key())
                    .serial_number(x509.random_serial_number()).not_valid_before(now-datetime.timedelta(minutes=1))
                    .not_valid_after(now+datetime.timedelta(days=1))
                    .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address("127.0.0.1"))]), False)
                    .add_extension(x509.BasicConstraints(ca=True, path_length=None), True).sign(key, hashes.SHA256()))
            certfile = d / "cert.pem"; keyfile = d / "key.pem"
            certfile.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
            keyfile.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
            seen = []; connects = []
            class Origin(http.server.BaseHTTPRequestHandler):
                protocol_version = "HTTP/1.1"
                def log_message(self, *args): pass
                def do_GET(self):
                    seen.append(self.headers.get("Authorization") == "Bearer LOCAL-fixture-secret")
                    self.send_response(101); self.send_header("Connection", "Upgrade"); self.send_header("Upgrade", "websocket"); self.end_headers()
                    self.wfile.write(b"\x81\x05hello"); self.wfile.flush(); time.sleep(.1)
                    self.wfile.write(b"\x81\x05world"); self.wfile.flush()
                    self.close_connection = True
            origin = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Origin)
            ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); ctx.load_cert_chain(certfile, keyfile)
            origin.socket = ctx.wrap_socket(origin.socket, server_side=True)
            threading.Thread(target=origin.serve_forever, daemon=True).start()
            class Proxy(socketserver.StreamRequestHandler):
                def handle(self):
                    first = self.rfile.readline(16384)
                    connects.append(first.startswith(b"CONNECT 127.0.0.1:"))
                    while self.rfile.readline(16384) not in (b"\r\n", b""): pass
                    with socket.create_connection(("127.0.0.1", origin.server_port), timeout=3) as target:
                        self.wfile.write(b"HTTP/1.1 200 Connection Established\r\n\r\n"); self.wfile.flush()
                        while True:
                            ready, _, _ = select.select([self.connection, target], [], [], 3)
                            if not ready: return
                            for source in ready:
                                chunk = source.recv(65536)
                                if not chunk: return
                                (target if source is self.connection else self.connection).sendall(chunk)
            proxy = socketserver.ThreadingTCPServer(("127.0.0.1", 0), Proxy)
            proxy.daemon_threads = True
            threading.Thread(target=proxy.serve_forever, daemon=True).start()
            script = d / "client.cjs"
            script.write_text('''const https = require('node:https');
const fail = process.argv[3] === 'fail';
const timer = setTimeout(() => process.exit(2), 4000);
const req = https.request(process.argv[2], {headers: {Connection:'Upgrade', Upgrade:'websocket', Authorization:'Bearer LOCAL-fixture-secret'}});
req.on('upgrade', (res, socket, head) => {
  if (fail) process.exit(3);
  let data = head;
  const check = () => { if (data.length >= 14) {
    if (data.toString('hex') !== '810568656c6c6f8105776f726c64') process.exit(4);
    clearTimeout(timer); socket.destroy(); console.log('STREAM_OK');
  }};
  socket.on('data', chunk => { data = Buffer.concat([data, chunk]); check(); }); check();
});
req.on('error', () => { clearTimeout(timer); if (fail) console.log('FAILED_CLOSED'); else process.exit(5); });
req.end();
''')
            env = {k:v for k,v in os.environ.items() if k not in {"NODE_OPTIONS", "SSLKEYLOGFILE", "MITMPROXY_SSLKEYLOGFILE"}}
            proxy_url = f"http://127.0.0.1:{proxy.server_address[1]}"
            env.update({"NODE_USE_ENV_PROXY":"1", "NODE_EXTRA_CA_CERTS":str(certfile),
                        "HTTP_PROXY":proxy_url, "http_proxy":proxy_url, "HTTPS_PROXY":proxy_url,
                        "https_proxy":proxy_url, "NO_PROXY":"", "no_proxy":""})
            url = f"https://127.0.0.1:{origin.server_port}/"
            try:
                result = subprocess.run([node, str(script), url], env=env, capture_output=True, timeout=6)
                self.assertEqual(result.returncode, 0, "Node HTTPS upgrade fixture failed")
                self.assertEqual(result.stdout.strip(), b"STREAM_OK")
                self.assertEqual(connects, [True]); self.assertEqual(seen, [True])
                proxy.shutdown(); proxy.server_close()
                result = subprocess.run([node, str(script), url, "fail"], env=env, capture_output=True, timeout=6)
                self.assertEqual(result.returncode, 0, "Node proxy failure fixture failed")
                self.assertEqual(result.stdout.strip(), b"FAILED_CLOSED")
                self.assertEqual(seen, [True], "An unavailable proxy must not cause direct origin access")
            finally:
                proxy.shutdown(); proxy.server_close(); origin.shutdown(); origin.server_close()


if __name__ == "__main__": unittest.main()
