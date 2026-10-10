#!/usr/bin/env python3
"""Read shipped application code only. Emit fixed search evidence, never source fragments."""
import argparse
import hashlib
import json
import pathlib
import plistlib
import struct

PATTERNS = {
    "renderer_http_service": "HTTP fetch service is unavailable.",
    "conversation_http_submit": "streamPost(`/f/conversation`",
    "sse_parser": "text/event-stream",
    "sse_done_marker": "[DONE]",
    "pubsub_open": "chatgpt_pubsub_transport_opened",
    "pubsub_update": "conversation-update",
    "pubsub_history_update": "conversation-history-update",
    "pubsub_turn_handoff": "conversation-turn-handoff-control",
    "pubsub_topic_field": "topic_id",
    "pubsub_command_envelope": "command:e,id:t",
    "room_message_endpoint": "/messaging/rooms/",
    "workspace_backend_origin": "backendOrigin",
    "workspace_account_constraint": "X-OpenAI-Account-Routing-Override",
    "workspace_identity_check": "Authenticated workspace changed",
    "request_identity_check": "Authenticated principal changed",
    "cloud_app_server_origin": "wss://codex-cloud-backend.chatgpt.com/",
    "node_websocket_transport": "perMessageDeflate:!1",
    "electron_network_fetch": "return f.net.fetch(r,t)",
}

def inspect(app):
    archive = app / "Contents/Resources/app.asar"
    with (app / "Contents/Info.plist").open("rb") as handle:
        info = plistlib.load(handle)
    digest = hashlib.sha256()
    with archive.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
        handle.seek(0)
        header = handle.read(16)
        if len(header) != 16:
            raise ValueError("Invalid ASAR header")
        _, header_size, _, json_size = struct.unpack("<4I", header)
        if json_size > 64 * 1024 * 1024 or header_size < json_size:
            raise ValueError("Unsupported ASAR header size")
        tree = json.loads(handle.read(json_size))
        base = 8 + header_size
        def walk(node, prefix=""):
            for name, value in node.get("files", {}).items():
                path = prefix + "/" + name
                if "files" in value:
                    yield from walk(value, path)
                elif not value.get("unpacked") and path.endswith(".js"):
                    if any(marker in path for marker in ["/.vite/build/main-", "/.vite/build/bootstrap-", "/webview/assets/app-shared-", "/webview/assets/app-initial-"]):
                        yield path, value
        matches = {key: [] for key in PATTERNS}
        for path, entry in walk(tree):
            size = int(entry.get("size", 0))
            if not 0 < size <= 18 * 1024 * 1024:
                continue
            handle.seek(base + int(entry["offset"]))
            source = handle.read(size).decode("utf-8", errors="replace")
            for key, pattern in PATTERNS.items():
                if pattern in source:
                    matches[key].append({"module": pathlib.PurePosixPath(path).name, "matches": source.count(pattern)})
    return {
        "formatVersion": 1,
        "evidenceKind": "Static shipped-code search; not a live protocol capture or supported integration contract",
        "applicationVersion": info.get("CFBundleShortVersionString"),
        "archiveSHA256": digest.hexdigest(),
        "matches": matches,
        "limits": ["No application code executed or modified", "No private profiles, chats, cookies or tokens read", "Static presence does not establish runtime use", "Minified symbols and modules change across versions"],
    }

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=pathlib.Path("/Applications/ChatGPT.app"))
    args = parser.parse_args()
    try:
        print(json.dumps(inspect(args.app), indent=2, sort_keys=True))
    except Exception:
        raise SystemExit("Could not inspect the application package; no source fragments emitted.")
