"""Real local TLS interception with a disposable CA; never changes system trust or uses OpenAI."""
import datetime
import http.client
import http.server
import ipaddress
import json
import os
import pathlib
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import unittest

class TLSFixture(unittest.TestCase):
    def test_local_sse_and_binary_forwarding_with_private_schema_export(self):
        try:
            import mitmproxy
            from cryptography import x509
            from cryptography.hazmat.primitives import hashes, serialization
            from cryptography.hazmat.primitives.asymmetric import rsa
            from cryptography.x509.oid import NameOID
        except ImportError:
            self.skipTest("Use the locked experimental environment for the TLS integration fixture")
        project = pathlib.Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as directory:
            d = pathlib.Path(directory)
            key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
            name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "Loopback fixture")])
            now = datetime.datetime.now(datetime.timezone.utc)
            cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key())
                    .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(minutes=1))
                    .not_valid_after(now + datetime.timedelta(days=1))
                    .add_extension(x509.SubjectAlternativeName([x509.IPAddress(ipaddress.ip_address("127.0.0.1"))]), critical=False)
                    .add_extension(x509.BasicConstraints(ca=True, path_length=None), critical=True).sign(key, hashes.SHA256()))
            certfile = d / "origin.pem"; keyfile = d / "origin-key.pem"
            certfile.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
            keyfile.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
            image = bytes(range(256)) * 800
            secret = "SECRET-local-fixture-cookie-prompt"
            seen_auth = []
            class Handler(http.server.BaseHTTPRequestHandler):
                protocol_version = "HTTP/1.1"
                def log_message(self, *args): pass
                def do_GET(self):
                    seen_auth.append(self.headers.get("Authorization") == "Bearer " + secret)
                    if self.path.startswith("/images/"):
                        self.send_response(200); self.send_header("Content-Type", "image/png")
                        self.send_header("Content-Length", str(len(image))); self.end_headers()
                        self.wfile.write(image); self.wfile.flush(); return
                    self.send_response(200); self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Transfer-Encoding", "chunked"); self.end_headers()
                    for payload in [b'data: {"type":"delta","delta":"' + secret.encode() + b'"}\n\n', b'data: [DONE]\n\n']:
                        self.wfile.write(f"{len(payload):x}\r\n".encode() + payload + b"\r\n"); self.wfile.flush(); time.sleep(.15)
                    self.wfile.write(b"0\r\n\r\n"); self.wfile.flush()
            origin = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER); ctx.load_cert_chain(certfile, keyfile)
            origin.socket = ctx.wrap_socket(origin.socket, server_side=True)
            thread = threading.Thread(target=origin.serve_forever, daemon=True); thread.start()
            try:
                with socket.socket() as s: s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]
                conf = d / "ca"; report = d / "report.json"
                subprocess.run([sys.executable, str(project / "tls_probe.py"), "--directory", str(conf), "--report", str(report), "--prepare"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                code = "import asyncio,sys;sys.path.insert(0,sys.argv[1]);import tls_probe;asyncio.run(tls_probe.run(sys.argv[2],sys.argv[3],int(sys.argv[4]),{'127.0.0.1'},sys.argv[5]))"
                proxy = subprocess.Popen([sys.executable, "-c", code, str(project), str(conf), str(report), str(port), str(certfile)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env={k:v for k,v in os.environ.items() if "KEYLOGFILE" not in k})
                try:
                    ready = False
                    for _ in range(100):
                        if proxy.poll() is not None: self.fail("TLS fixture helper exited early")
                        try:
                            with socket.create_connection(("127.0.0.1", port), timeout=.1): ready = True; break
                        except OSError: time.sleep(.05)
                    self.assertTrue(ready)
                    client_ca = ssl.create_default_context(cafile=str(conf / "mitmproxy-ca-cert.pem"))
                    def get(path):
                        c = http.client.HTTPSConnection("127.0.0.1", port, context=client_ca, timeout=5)
                        c.set_tunnel("127.0.0.1", origin.server_port)
                        c.request("GET", path, headers={"Authorization": "Bearer " + secret, "Cookie": secret})
                        return c, c.getresponse()
                    c, response = get("/backend-api/f/conversation?token=" + secret)
                    self.assertEqual(response.status, 200)
                    first = response.readline(); self.assertIn(secret.encode(), first)
                    self.assertFalse(response.isclosed())
                    self.assertIn(b"[DONE]", response.read()); c.close()
                    c, response = get("/images/" + secret)
                    self.assertEqual(response.read(), image); c.close()
                    for _ in range(40):
                        value = json.loads(report.read_text())
                        if any(e.get("kind") == "sse-done" for e in value["events"]): break
                        time.sleep(.05)
                    self.assertTrue(any(e.get("kind") == "sse-done" for e in value["events"]))
                    self.assertTrue(any(e.get("responseKind") == "image" for e in value["events"]))
                    self.assertNotIn(secret, report.read_text())
                    self.assertEqual(seen_auth, [True, True])
                finally:
                    proxy.terminate()
                    try: proxy.wait(timeout=5)
                    except subprocess.TimeoutExpired: proxy.kill(); proxy.wait()
                    self.assertFalse(json.loads(report.read_text())["listenerReady"])
            finally:
                origin.shutdown(); origin.server_close()

if __name__ == "__main__": unittest.main()
