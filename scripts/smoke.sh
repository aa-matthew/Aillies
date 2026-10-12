#!/usr/bin/env bash
# Two Aillies nodes on one machine + mock OpenAI backend (+ real Codex if installed).
# Proves: pairing, pinned routing, B's account used, no upstream sends while offline.
# Usage: scripts/smoke.sh            (builds debug binary first)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/aillies-smoke.XXXXXX")"
BIN="$ROOT/target/debug/aillies"
MOCK_PORT=19999
cargo build -q -p aillies-cli --manifest-path "$ROOT/Cargo.toml"

cleanup() { kill ${PIDS:-} 2>/dev/null || true; }
trap cleanup EXIT

python3 - "$WORK/b-auth.json" <<'PY'
import base64, json, sys
b = lambda x: base64.urlsafe_b64encode(json.dumps(x).encode()).decode().rstrip("=")
tok = f"{b({'alg':'none'})}.{b({'exp': 4e9, 'https://api.openai.com/auth': {'chatgpt_account_id': 'acct_b'}})}.sig"
json.dump({"OPENAI_API_KEY": None, "tokens": {"access_token": tok, "account_id": "acct_b"}}, open(sys.argv[1], "w"))
PY

python3 "$ROOT/scripts/mock-upstream.py" $MOCK_PORT "$WORK/upstream.log" & PIDS="$!"
export AILLIES_CODEX_UPSTREAM="http://127.0.0.1:$MOCK_PORT/backend-api/codex/responses" AILLIES_ADVERTISE_LOOPBACK=1
AILLIES_TEST_CLAUDE_HOME="$WORK/b-claude" AILLIES_HOME="$WORK/b" AILLIES_LOOPBACK_PORT=18622 AILLIES_PEER_PORT=18644 AILLIES_CODEX_AUTH="$WORK/b-auth.json" "$BIN" run > "$WORK/b.log" 2>&1 & PIDS="$PIDS $!"
mkdir -p "$WORK/a-codex"
AILLIES_TEST_CLAUDE_HOME="$WORK/a-claude" CODEX_HOME="$WORK/a-codex" AILLIES_HOME="$WORK/a" AILLIES_LOOPBACK_PORT=18722 AILLIES_PEER_PORT=18745 AILLIES_CODEX_AUTH="$WORK/none.json" "$BIN" run > "$WORK/a.log" 2>&1 & PIDS="$PIDS $!"
A() { AILLIES_HOME="$WORK/a" "$BIN" "$@"; }
B() { AILLIES_HOME="$WORK/b" "$BIN" "$@"; }

# Initialization can take longer while release builds are running. Wait for
# authenticated control readiness before issuing any pairing or inference call.
SMOKE_READY=0
for _ in $(seq 100); do
  if A status >/dev/null 2>&1 && B status >/dev/null 2>&1; then SMOKE_READY=1; break; fi
  sleep 0.25
done
if [ "$SMOKE_READY" != 1 ]; then
  echo "Smoke nodes did not become ready. Startup logs: $WORK" >&2
  exit 1
fi

# The Codex smoke must never consult the owner's Claude Keychain/login.
python3 - "$WORK" <<'PY'
import json, pathlib, sys, urllib.request
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
for name, port in [("a", 18722), ("b", 18622)]:
    token = json.loads((pathlib.Path(sys.argv[1]) / name / "secrets.json").read_text())["control_token"]
    request = urllib.request.Request(f"http://127.0.0.1:{port}/control/status", headers={"Authorization": f"Bearer {token}"})
    with opener.open(request, timeout=5) as response:
        account = json.load(response)["claude"]["account"]
    assert account == {"state": "failed", "code": "claude_login_unavailable"}
PY

B share on
yes | B invite --full > "$WORK/invite.out" 2>&1 &
for _ in $(seq 20); do CODE=$(grep -o 'aillies1:[A-Za-z0-9_-]*' "$WORK/invite.out" || true); [ -n "$CODE" ] && break; sleep 0.25; done
echo y | A pair "$CODE"
A check
A codex on

KEY=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['codex_key'])" "$WORK/a/secrets.json")
curl -sf -H "Authorization: Bearer $KEY" -H 'content-type: application/json' \
  -d '{"model":"gpt-5.5","input":"hi"}' http://127.0.0.1:18722/v1/responses | grep -q response.completed
echo "✓ curl turn routed through B"

if command -v codex >/dev/null; then
  CODEX_HOME="$WORK/a-codex" codex exec --skip-git-repo-check "Say hello" < /dev/null > "$WORK/codex.out" 2> "$WORK/codex.err"
  grep -q "Hello from device B" "$WORK/codex.out"
  echo "✓ real Codex turn routed through B"
fi
grep -q '"acct": "acct_b"' "$WORK/upstream.log"
echo "✓ B's account used upstream"

B share off
COUNT=$(wc -l < "$WORK/upstream.log")
curl -s -H "Authorization: Bearer $KEY" -H 'content-type: application/json' -d '{"model":"gpt-5.5","input":"hi"}' \
  http://127.0.0.1:18722/v1/responses | grep -q peer_unavailable
echo "✓ B offline fails closed"
[ "$(wc -l < "$WORK/upstream.log")" = "$COUNT" ]
echo "✓ nothing sent while B was offline"
echo "Smoke test passed. Logs: $WORK"
