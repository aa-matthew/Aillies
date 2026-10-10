// Browser preview only: fakes the Tauri bridge when the page is opened
// outside the app (design reviews, screenshots). Ignored inside Aillies.
"use strict";
if (!window.__TAURI__) {
  const fp = "2b8e05d4" + "0".repeat(56);
  const q = new URLSearchParams(location.search);
  const state = {
    name: "Matthew’s Mac", platform: "macos", short_id: "7F3A·91C2", fingerprint: "7f3a91c2" + "1".repeat(56), version: "0.9.0-alpha.1",
    state: "ready", routing: true, sharing: true, destination: { kind: "peer", id: fp }, destination_name: "Dailyngocss",
    peers: [{ fp, short_id: "2B8E·05D4", name: "Dailyngocss", platform: "macos", addrs: ["192.168.1.20:18544", "100.101.7.3:18544", "dailyngocss.local:18544"] }],
    clients: [{ fp: "9" .repeat(64), short_id: "9999·9999", name: "Office PC", platform: "windows", approved_at: 0, last_used: Date.now() / 1000 - 7200 }],
    pending: [], codex_installed: true, local_account: { state: "ready" }, last_failure: null, loopback_port: 18522, peer_port: 18544,
  };
  if (q.get("state") === "action") { state.state = "action"; state.last_failure = { code: "oauth_expired", message: "The sharing device's Codex login expired. Open Codex there to renew it, then retry.", at: 0 }; }
  if (q.get("state") === "pending") state.pending = [{ id: "p1", name: "Lan’s MacBook", platform: "macos", code: "BLUE · 47 · MOON", short_id: "5C1D·88A0" }];
  if (q.get("state") === "new") { state.peers = []; state.clients = []; state.sharing = false; state.destination = { kind: "local" }; state.state = "local"; state.codex_installed = false; }
  const health = { reachable: true, latency_ms: 4, addr: "192.168.1.20:18544", status: { providers: [{ account: { state: "ready" } }] } };
  const handlers = {
    startup_error: () => null, status: () => structuredClone(state), peer_health: () => health,
    set_routing: ({ on }) => { state.routing = on; state.state = on ? "ready" : "paused"; },
    set_sharing: ({ on }) => { state.sharing = on; }, set_destination: () => {}, dismiss_failure: () => { state.last_failure = null; state.state = "ready"; },
    create_invite: () => "aillies1:eyJ2IjoxLCJuYW1lIjoiTWF0dGhld-KAmXMgTWFjIiwicGxhdGZvcm0iOiJtYWNvcyIsImFkZHJzIjpbIjE5Mi4xNjguMS4yMDoxODU0NCJdfQ",
    start_pairing: () => ({ id: "x", code: "BLUE · 47 · MOON", name: "Dailyngocss", platform: "macos", short_id: "2B8E·05D4" }),
    confirm_pairing: () => ({ fp, short_id: "2B8E·05D4", name: "Dailyngocss", platform: "macos", addrs: [] }), codex_enable: () => { state.codex_installed = true; }, codex_restore: () => { state.codex_installed = false; },
  };
  window.__TAURI__ = { core: { invoke: async (cmd, args) => (handlers[cmd] ?? (() => null))(args ?? {}) } };
  if (q.get("screen")) setTimeout(() => { document.dispatchEvent(new CustomEvent("x")); }, 0);
}
