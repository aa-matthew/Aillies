import importlib.util
import json
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("tls_probe", pathlib.Path(__file__).parents[1] / "tls_probe.py")
p = importlib.util.module_from_spec(spec); spec.loader.exec_module(p)

class ProbeTests(unittest.TestCase):
    def test_fixed_schema_never_exports_values_or_arbitrary_keys(self):
        secret = "SECRET-cookie-token-prompt-signed-url"
        value = {secret: secret, "message": {"content": {"parts": [secret]}, "id": secret}, "type": secret}
        result = json.dumps(p.shape(value))
        self.assertNotIn(secret, result)
        self.assertIn("otherKeyCount", result)
        self.assertEqual(p.shape({"type": "conversation-update"})["knownType"], "conversation-update")
        self.assertEqual(p.shape({"method": "turn/start", "params": {"message": secret}})["knownMethod"], "turn/start")
        self.assertNotIn(secret, json.dumps(p.shape({"method": secret, "params": {secret: secret}})))
        self.assertEqual(p.route("/?token=SECRET"), "/")
    def test_incremental_sse_and_unchanged_forwarding(self):
        events = []; parser = p.SSEShapes(events.append)
        source = b'data: {"type":"delta","delta":"SECRET"}\r\n\r\ndata: [DONE]\n\n'
        self.assertEqual(b"".join(parser.feed(source[i:i+3]) for i in range(0, len(source), 3)), source)
        self.assertEqual(len(events), 2)
        self.assertNotIn("SECRET", json.dumps(events))
        self.assertEqual(events[-1]["kind"], "sse-done")
    def test_oversized_sse_is_dropped_and_parser_recovers(self):
        events = []; parser = p.SSEShapes(events.append)
        parser.feed(b"data: " + b"x" * (p.LIMIT * 3) + b"\n\ndata: [DONE]\n\n")
        self.assertEqual(events, [{"kind": "sse-done"}])
        self.assertLessEqual(len(parser.line) + len(parser.data), p.LIMIT)
    def test_generation_evidence_survives_startup_flood(self):
        with tempfile.TemporaryDirectory() as d:
            report = p.Report(pathlib.Path(d) / "report.json")
            for _ in range(2000): report.add({"kind": "request", "path": "/backend-api/:redacted"})
            report.add({"kind": "request", "path": "/backend-api/f/conversation"})
            for _ in range(1000): report.add({"kind": "sse-done"})
            self.assertTrue(any(e.get("path") == "/backend-api/f/conversation" for e in report.events))
            self.assertEqual(next(e for e in report.events if e["kind"] == "sse-done")["occurrences"], 1000)
            self.assertEqual(report.important_dropped, 0)
            self.assertLess(len(report.events), 1200)
    def test_paths_caps_and_private_output(self):
        self.assertEqual(p.route("/backend-api/conversation/SECRET?token=SECRET"), "/backend-api/conversation/:redacted")
        with tempfile.TemporaryDirectory() as d:
            report = p.Report(pathlib.Path(d) / "report.json")
            for _ in range(1300): report.add({"kind": "fixture"})
            report.save()
            self.assertEqual(report.path.stat().st_mode & 0o777, 0o600)
            value = json.loads(report.path.read_text())
            self.assertTrue(value["truncated"])
            self.assertFalse(value["peerAccountEnabled"])
            self.assertLessEqual(len(value["events"]), 1200)
            self.assertTrue(value["backgroundLimited"])

if __name__ == "__main__": unittest.main()
