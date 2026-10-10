#!/usr/bin/env python3
"""Mock of the Codex ChatGPT backend for local smoke tests.

Usage: mock-upstream.py PORT LOGFILE
Logs each request (account id, auth prefix, body) and replies with a short
Responses SSE stream. Never point real tokens at anything but loopback.
"""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer
LOG = sys.argv[2]
class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def do_POST(self):
        n = int(self.headers.get("content-length", 0))
        body = self.rfile.read(n)
        with open(LOG, "a") as f:
            f.write(json.dumps({"path": self.path, "auth": self.headers.get("authorization","")[:20], "acct": self.headers.get("chatgpt-account-id"), "body": json.loads(body)}) + "\n")
        ev = [
            ("response.created", {"type":"response.created","response":{"id":"resp_1"}}),
            ("response.output_item.done", {"type":"response.output_item.done","output_index":0,"item":{"type":"message","role":"assistant","id":"msg_1","content":[{"type":"output_text","text":"Hello from device B via Aillies"}]}}),
            ("response.completed", {"type":"response.completed","response":{"id":"resp_1","usage":{"input_tokens":5,"input_tokens_details":None,"output_tokens":7,"output_tokens_details":None,"total_tokens":12}}}),
        ]
        data = "".join(f"event: {k}\ndata: {json.dumps(v)}\n\n" for k, v in ev).encode()
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def log_message(self, *a): pass
HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
