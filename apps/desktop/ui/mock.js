// Browser preview only: fakes the Tauri bridge when the page is opened
// outside the app (design reviews, screenshots). Ignored inside Aillies.
"use strict";
if (!window.__TAURI__) {
  const fp = "2b8e05d4" + "0".repeat(56);
  const q = new URLSearchParams(location.search);
  const state = {
    name: "Matthew’s Mac", platform: "macos", short_id: "7F3A·91C2", fingerprint: "7f3a91c2" + "1".repeat(56), version: "0.9.0-alpha.12", diagnostics_build: "router-core-v4-pairing",
    state: "ready", routing: true, sharing: true, fast_mode: false, allow_fast_mode: false, destination: { kind: "peer", id: fp }, destination_name: "Dailyngocss",
    peers: [{ fp, short_id: "2B8E·05D4", name: "Dailyngocss", platform: "macos", addrs: ["192.168.1.20:18544", "100.101.7.3:18544", "dailyngocss.local:18544"] }],
    clients: [{ fp: "9" .repeat(64), short_id: "9999·9999", name: "Office PC", platform: "windows", approved_at: 0, last_used: Date.now() / 1000 - 7200 }],
    pending: [], codex_installed: true, local_account: { state: "ready" }, last_failure: null, loopback_port: 18522, peer_port: 18544,
  };
  if (q.get("state") === "action") { state.state = "action"; state.last_failure = { code: "oauth_expired", message: "The sharing device's Codex login expired. Open Codex there to renew it, then retry.", at: 0 }; }
  if (q.get("state") === "pending") state.pending = [{ id: "p1", name: "Lan’s MacBook", platform: "macos", code: "BLUE · 47 · MOON", short_id: "5C1D·88A0" }];
  if (q.get("state") === "new") { state.peers = []; state.clients = []; state.sharing = false; state.destination = { kind: "local" }; state.state = "local"; state.codex_installed = false; }
  const health = { reachable: true, latency_ms: 4, addr: "192.168.1.20:18544", status: { sharing: true, providers: [{ provider: "codex", allow_fast_mode: true, account: { state: "ready" } }] } };
  const handlers = {
    prepare_codex_restart: ({ on }) => ({ token: "preview", on, apps: ["ChatGPT"], terminals: 2 }),
    restart_codex_provider: () => { state.codex_installed = !state.codex_installed; return { chats: 12, reopen_failed: 0 }; },
    switch_codex_provider: ({ on }) => { if (q.get("state") === "restart") throw "Quit Codex, then switch providers here. Your chats will be preserved."; state.codex_installed = on; return { chats: 12, elapsed_ms: 28, reopen_required: true }; },
    take_navigation: () => null, quit_if_normal: () => { if (state.codex_installed) throw "Switch to Normal before quitting."; },
    export_diagnostics: () => "Downloads/Aillies-diagnostics-preview.json",
    startup_error: () => null, status: () => structuredClone(state), peer_health: () => health,
    set_routing: ({ on }) => { state.routing = on; state.state = on ? "ready" : "paused"; },
    set_fast_mode: ({ on }) => { state.fast_mode = on; },
    set_allow_fast_mode: ({ on }) => { state.allow_fast_mode = on; },
    set_sharing: ({ on }) => { state.sharing = on; }, set_destination: () => {}, dismiss_failure: () => { state.last_failure = null; state.state = "ready"; },
    create_invite: () => "00123456",
    create_full_invite: () => "aillies1:eyJ2IjoxLCJuYW1lIjoiTWF0dGhld-KAmXMgTWFjIiwicGxhdGZvcm0iOiJtYWNvcyIsImFkZHJzIjpbIjE5Mi4xNjguMS4yMDoxODU0NCJdfQ",
    start_pairing: () => ({ id: "x", code: "BLUE · 47 · MOON", name: "Dailyngocss", platform: "macos", short_id: "2B8E·05D4" }),
    confirm_pairing: () => ({ fp, short_id: "2B8E·05D4", name: "Dailyngocss", platform: "macos", addrs: [] }),
  };
  window.__TAURI__ = { core: { invoke: async (cmd, args) => (handlers[cmd] ?? (() => null))(args ?? {}) } };
  if (q.get("screen")) setTimeout(() => { document.dispatchEvent(new CustomEvent("x")); }, 0);
}
