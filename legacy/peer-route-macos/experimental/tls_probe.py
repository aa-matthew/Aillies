#!/usr/bin/env python3
"""Opt-in, local TLS compatibility experiment. Never saves flows or credential values."""
import argparse
import asyncio
import json
import os
import pathlib
import re
import signal
import tempfile

HOSTS = {"chatgpt.com", "ws.chatgpt.com", "codex-cloud-backend.chatgpt.com"}
SEGMENTS = frozenset("backend-api f conversation prepare sentinel chat-requirements celsius ws user messaging rooms messages files download images bootstrap imagegen library status upload me accounts check models auth session api".split())
KEYS = frozenset("jsonrpc method params result error code type message messages content parts metadata status conversation_id parent_message_id id author role event delta data payload topic_id command reply offset p o v c model action end_turn finish_details recipient channel asset_pointer file_id image_url tool_calls tools client_prepare_state app_attest_challenge".split())
METHODS = frozenset("initialize initialized thread/start thread/resume thread/read turn/start turn/interrupt turn/started turn/completed item/started item/completed item/agentMessage/delta account/read getAuthStatus configRequirements/read".split())
TYPES = frozenset("message delta patch ping pong done message_stream_complete conversation_detail_metadata conversation-update conversation-history-update conversation-turn-handoff-control text image_asset_pointer multimodal_text image_generation response.created response.output_text.delta response.completed response.failed".split())
HEADER_NAMES = frozenset({"authorization", "cookie", "x-sentinel-dc", "openai-sentinel-chat-requirements-token", "openai-sentinel-proof-token", "openai-sentinel-turnstile-token", "x-conduit-token", "x-openai-account-routing-override"})
LIMIT = 65536

def route(path):
    path = path.split("?", 1)[0]
    if not path.strip("/"): return "/"
    return "/" + "/".join(s if s in SEGMENTS else ":redacted" for s in path.strip("/").split("/")[:12])

def shape(value, depth=0):
    """Fixed vocabulary only: never copy arbitrary keys, string values or identifiers."""
    if depth >= 4:
        return "depth-limit"
    if isinstance(value, dict):
        result = {k: shape(v, depth + 1) for k, v in list(value.items())[:64] if k in KEYS}
        result["otherKeyCount"] = min(64, sum(k not in KEYS for k in value))
        if isinstance(value.get("type"), str) and value["type"] in TYPES:
            result["knownType"] = value["type"]
        if isinstance(value.get("method"), str) and value["method"] in METHODS:
            result["knownMethod"] = value["method"]
        return result
    if isinstance(value, list):
        return {"arraySample": [shape(v, depth + 1) for v in value[:3]], "countBand": band(len(value))}
    if isinstance(value, str):
        return "string"
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    return "number" if isinstance(value, (int, float)) else "other"

def band(n):
    if n == 0: return "empty"
    if n < 1024: return "under-1KiB"
    if n < 16384: return "1-16KiB"
    if n <= LIMIT: return "16-64KiB"
    return "over-64KiB"

class SSEShapes:
    """Bounded incremental SSE parser. Oversized lines/events are discarded until a delimiter."""
    def __init__(self, emit):
        self.emit = emit; self.line = bytearray(); self.data = bytearray(); self.discard = False
        self.line_overflow = False; self.events = 0
    def feed(self, chunk):
        # Process by line rather than retain an unbounded HTTP chunk or entire response.
        for byte in chunk:
            if byte != 10:
                if not self.line_overflow:
                    if len(self.line) < LIMIT: self.line.append(byte)
                    else: self.line.clear(); self.line_overflow = True; self.discard = True
                continue
            line = bytes(self.line).rstrip(b"\r"); self.line.clear()
            if self.line_overflow:
                self.line_overflow = False; continue
            if not line:
                if not self.discard and self.data and self.events < 200:
                    self.events += 1
                    payload = bytes(self.data).strip()
                    if payload == b"[DONE]": self.emit({"kind": "sse-done"})
                    else:
                        try: self.emit({"kind": "sse-json-shape", "shape": shape(json.loads(payload))})
                        except (ValueError, RecursionError): self.emit({"kind": "sse-unparsed"})
                self.data.clear(); self.discard = False
            elif line.startswith(b"data:") and not self.discard:
                data = line[5:].lstrip(b" ")
                if len(self.data) + len(data) + 1 > LIMIT: self.data.clear(); self.discard = True
                else: self.data.extend(data); self.data.extend(b"\n")
        return chunk

class Report:
    def __init__(self, path):
        self.path = pathlib.Path(path); self.events = []; self.truncated = False; self.bytes = 0; self.ready = False
        self.background = 0; self.background_limited = False; self.important_dropped = 0; self.shapes = {}
    def add(self, event):
        encoded = json.dumps(event, sort_keys=True)
        if len(encoded) > 12000: event = {"kind": "shape-limit"}; encoded = json.dumps(event)
        aggregate = event.get("kind") in {"websocket-shape", "sse-json-shape", "sse-done", "sse-unparsed", "connect-destination"}
        if aggregate and encoded in self.shapes:
            row = self.shapes[encoded]; row["occurrences"] = min(1_000_000, row.get("occurrences", 1) + 1); return
        important = (aggregate or event.get("kind") == "client-tls-failed" or
                     event.get("responseKind") in {"sse", "image", "websocket"} or
                     event.get("path", "").startswith(("/backend-api/f/conversation", "/backend-api/messaging/rooms")))
        # Reserve 900 slots for generation/stream evidence instead of startup chatter.
        if not important and self.background >= 300:
            self.background_limited = True; self.truncated = True; return
        if len(self.events) >= 1200 or self.bytes + len(encoded) + 32 > 1_000_000:
            self.truncated = True
            if important: self.important_dropped += 1
            return
        self.bytes += len(encoded) + 32
        self.events.append(event)
        if not important: self.background += 1
        if aggregate: self.shapes[encoded] = event
    def save(self):
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        document = {"version": 1, "mode": "local-tls-compatibility-only", "peerAccountEnabled": False, "listenerReady": self.ready,
                    "truncated": self.truncated, "backgroundLimited": self.background_limited, "importantEventsDropped": self.important_dropped, "events": self.events,
                    "limits": ["No raw bodies, credentials, arbitrary keys, identifiers, query values or timestamps saved", "Only fixed schema vocabulary exported", "Uses current local ChatGPT account", "No application patching or certificate pinning bypass"]}
        # Temp file starts private, before any data is written.
        fd, name = tempfile.mkstemp(dir=self.path.parent, prefix=".probe-")
        try:
            with os.fdopen(fd, "w") as f: json.dump(document, f, separators=(",", ":"))
            os.replace(name, self.path)
        finally:
            if os.path.exists(name): os.unlink(name)

class Probe:
    def __init__(self, report, hosts=HOSTS):
        self.report = report; self.hosts = hosts; self.requests = 0
    def scoped(self, flow):
        return flow.request.host.lower() in self.hosts and flow.request.scheme == "https"
    def http_connect(self, flow):
        # Count known destinations even when their TLS is deliberately not intercepted.
        if flow.request.host in HOSTS | {"api.openai.com", "chat.openai.com", "api.chatgpt.com"} and flow.request.port == 443:
            self.report.add({"kind": "connect-destination", "host": flow.request.host})
    def requestheaders(self, flow):
        # Stream all bodies; no full-flow capture, request buffering or automatic replay.
        flow.request.stream = True
        if not self.scoped(flow): return
        self.requests += 1
        flow.metadata["peer_probe_scoped"] = True
        path = flow.request.path.split("?", 1)[0]
        if flow.request.host not in {"ws.chatgpt.com", "codex-cloud-backend.chatgpt.com"} and not path.startswith("/backend-api/"): return
        self.report.add({"kind": "request", "host": flow.request.host if flow.request.host in HOSTS else "fixture",
                         "method": flow.request.method if flow.request.method in {"GET", "POST", "PUT", "DELETE", "OPTIONS"} else "other",
                         "path": route(flow.request.path),
                         "authHeaderNamesPresent": sorted(k for k in HEADER_NAMES if k in flow.request.headers)})
    def responseheaders(self, flow):
        flow.response.stream = True
        if not flow.metadata.get("peer_probe_scoped"): return
        mime = flow.response.headers.get("content-type", "").split(";", 1)[0].lower()
        kind = "sse" if mime == "text/event-stream" else "json" if mime == "application/json" else "image" if mime.startswith("image/") else "other"
        path = flow.request.path.split("?", 1)[0]
        if not path.startswith("/backend-api/") and kind not in {"sse", "image"} and flow.response.status_code != 101: return
        self.report.add({"kind": "response", "path": route(flow.request.path), "status": flow.response.status_code, "responseKind": "websocket" if flow.response.status_code == 101 else kind})
        if kind == "sse":
            parser = SSEShapes(self.report.add)
            flow.response.stream = parser.feed
    def websocket_message(self, flow):
        if not flow.metadata.get("peer_probe_scoped") or not flow.websocket: return
        message = flow.websocket.messages[-1]
        event = {"kind": "websocket-shape", "direction": "sent" if message.from_client else "received", "sizeBand": band(len(message.content))}
        if message.is_text and len(message.content) <= LIMIT:
            try: event["shape"] = shape(json.loads(message.content))
            except (ValueError, RecursionError): event["shape"] = "unparsed"
        else: event["shape"] = "opaque"
        self.report.add(event)
        # mitmproxy 12.2.3 forwards its local message reference after the hook; drop stored history.
        flow.websocket.messages.clear()
    def tls_failed_client(self, data):
        self.report.add({"kind": "client-tls-failed", "meaning": "Client did not complete local TLS; trust or compatibility may be missing"})
    async def running(self):
        self.report.ready = True
        self.report.save()
        self.writer = asyncio.create_task(self.periodic())
    async def periodic(self):
        while True:
            await asyncio.sleep(1)
            self.report.save()
    def done(self):
        if hasattr(self, "writer"): self.writer.cancel()
        self.report.ready = False
        self.report.save()

async def run(directory, report_path, port=18320, hosts=HOSTS, upstream_ca=None):
    os.environ.pop("SSLKEYLOGFILE", None)
    os.environ.pop("MITMPROXY_SSLKEYLOGFILE", None)
    from mitmproxy import options
    from mitmproxy.tools.dump import DumpMaster
    directory = pathlib.Path(directory); directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(directory, 0o700)
    config = options.Options(listen_host="127.0.0.1", listen_port=port, confdir=str(directory),
                             allow_hosts=[r"^(?:" + "|".join(re.escape(h) for h in sorted(hosts)) + r"):443$"],
                             ssl_insecure=False)
    # Fixtures may use random ports, but production only intercepts exact port 443.
    if hosts != HOSTS: config.allow_hosts = [r"^127\.0\.0\.1:\d+$"]
    master = DumpMaster(config, with_termlog=False, with_dumper=False)
    if upstream_ca: master.options.update(ssl_verify_upstream_trusted_ca=str(upstream_ca))
    master.addons.add(Probe(Report(report_path), hosts))
    loop = asyncio.get_running_loop()
    loop.add_signal_handler(signal.SIGTERM, master.shutdown)
    try:
        await master.run()
    finally:
        loop.remove_signal_handler(signal.SIGTERM)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=pathlib.Path, required=True)
    parser.add_argument("--report", type=pathlib.Path, required=True)
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--consent-local-tls", action="store_true")
    args = parser.parse_args()
    # Never inherit a key-log sink from a developer's environment.
    os.environ.pop("SSLKEYLOGFILE", None)
    os.environ.pop("MITMPROXY_SSLKEYLOGFILE", None)
    os.umask(0o077)
    if args.prepare:
        from mitmproxy.certs import CertStore
        args.directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(args.directory, 0o700)
        CertStore.from_store(args.directory, "mitmproxy", 2048)
        Report(args.report).save()
    elif args.consent_local_tls:
        asyncio.run(run(args.directory, args.report))
    else:
        parser.error("Explicit --consent-local-tls required; no certificate trust changes are made.")

if __name__ == "__main__": main()
