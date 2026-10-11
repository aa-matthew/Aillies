// Aillies window. Plain JS, no build step. All state comes from the node.
"use strict";

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const $app = document.getElementById("app");
const esc = (v) => String(v ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

const ui = {
  screen: "home",
  status: null,
  health: {},          // fp -> PeerHealth
  invite: null,        // code shown on the sharing side
  pairing: null,       // PairStarted on the requesting side
  waiting: false,
  switching: false,
  providerTarget: null,
  restart: null,
  restartError: null,
  paired: null,        // PeerView after success
  error: null,
  startupError: null,
  quitAfterSwitch: false,
  navigating: false,
};

const LABEL = { ready: "Connected", local: "Using this device", paused: "Paused", action: "Action required" };
const STATE_CLASS = { ready: "s-ready", action: "s-action" };
const OS = { macos: "Mac", windows: "PC", linux: "computer" };

function toast(msg) {
  const t = document.getElementById("toast");
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.hidden = true), 3500);
}

async function act(fn, okMsg) {
  try {
    const r = await fn();
    if (okMsg) toast(okMsg);
    await refresh();
    return r;
  } catch (e) {
    toast(String(e));
    throw e;
  }
}

function go(screen) {
  if (ui.screen === "codex" && screen !== "codex" && !ui.switching) { ui.restart = null; ui.providerTarget = null; ui.quitAfterSwitch = false; }
  ui.screen = screen === "pair" ? "pair-enter" : screen;
  ui.error = null;
  render();
}

async function refresh(fromPoll = false) {
  try {
    ui.status = await invoke("status");
  } catch (e) {
    ui.startupError = String(e);
  }
  const s = ui.status;
  if (s && s.destination.kind === "peer" && !ui.health[s.destination.id]) checkHealth(s.destination.id);
  render(fromPoll);
  await receiveNavigation();
}

async function receiveNavigation() {
  if (ui.navigating || ui.switching || ui.restart || ui.waiting || ui.pairing || !ui.status) return;
  ui.navigating = true;
  try {
    const intent = await invoke("take_navigation").catch(() => null);
    if (!intent) return;
    if (intent.screen === "codex") {
      go("codex");
      if (typeof intent.on === "boolean" && (intent.on !== ui.status.codex_installed || intent.on && !ui.status.routing)) await switchProvider(intent.on);
    } else if (intent.screen === "error") {
      go("home"); ui.error = intent.message; render();
    } else if (intent.screen === "warnings") {
      go("home");
      document.getElementById("recent-warnings")?.scrollIntoView({ block: "nearest" });
    } else go(intent.screen);
  } finally { ui.navigating = false; }
}

async function checkHealth(fp) {
  ui.health[fp] = { checking: true };
  try {
    ui.health[fp] = await invoke("peer_health", { fp });
  } catch {
    ui.health[fp] = { reachable: false };
  }
  render();
}

function peerName(fp) {
  return ui.status?.peers.find((p) => p.fp === fp)?.name ?? "the device";
}

function accountLine(h) {
  if (!h || h.checking) return '<span class="spin"></span>Checking…';
  if (!h.reachable) return "Unreachable";
  const acct = h.status?.providers?.[0]?.account;
  const path = `${esc(h.addr?.startsWith("127.") ? "This machine" : "Direct")} · ${esc(h.latency_ms)} ms`;
  if (acct?.state === "ready") return path;
  return `${path} · account not ready`;
}

// ---------- Screens ----------

function home(s) {
  const dev = OS[s.platform] ?? "device";
  const dest = s.destination.kind === "peer" ? s.destination.id : "local";
  const h = dest !== "local" ? ui.health[dest] : null;
  const destSub = dest === "local" ? (s.local_account.state === "ready" ? "Your own Codex login" : "No Codex login on this device") : accountLine(h);
  const active = s.codex_installed;
  const lead = !active ? "Codex uses your usual provider." : !s.routing ? "Choose Aillies again to enable this connection." : dest === "local" ? "Codex uses this device’s own login." : `Codex uses ${esc(s.destination_name)}’s account.`;
  const canFast = dest === "local" ? s.local_account.state === "ready" : h?.reachable && h.status?.sharing && h.status?.providers?.some((p) => p.provider === "codex" && p.allow_fast_mode && p.account?.state === "ready");
  const fail = active ? s.last_failure : null;
  const pending = s.pending[0];
  return `
    ${brand()}
    ${pending ? `<div class="callout info"><strong>${esc(pending.name)} wants to use this ${dev}’s account</strong>
       <p>Approve only if both devices show <b>${esc(pending.code)}</b>.</p>
       <div class="actions"><button class="btn" data-decide="${esc(pending.id)}" data-approve="0">Decline</button><button class="btn primary" data-decide="${esc(pending.id)}" data-approve="1">Codes match — approve</button></div></div>` : ""}
    ${fail ? `<div class="callout action" role="alert"><strong>${esc(fail.message)}</strong>
       <div class="actions"><button class="btn" data-do="dismiss">Dismiss</button>${dest !== "local" ? `<button class="btn primary" data-do="recheck">Check again</button>` : ""}</div></div>` : ""}
    ${ui.error ? `<div class="callout action" role="alert"><strong>${esc(ui.error)}</strong><div class="actions"><button class="btn" data-do="dismiss">Dismiss</button></div></div>` : ""}
    <h1 class="${active ? !s.routing ? "s-action" : STATE_CLASS[s.state] ?? "" : ""}"><span class="dot"></span>${active ? s.routing ? LABEL[s.state] : "Action required" : "Normal provider"}</h1>
    <p class="lead">${lead}</p>
    <div class="group">
      <button class="row" data-go="codex"><span>Use Aillies with<span class="sub">Choose your provider</span></span><span class="trail">Codex<span class="chev"></span></span></button>
      ${active ? `
      <div class="row"><span>Send through<span class="sub">${destSub}</span></span>
        <select data-dest aria-label="Send through">
          <option value="local" ${dest === "local" ? "selected" : ""}>This ${dev}</option>
          ${s.peers.map((p) => `<option value="${esc(p.fp)}" ${p.fp === dest ? "selected" : ""}>${esc(p.name)}</option>`).join("")}
        </select></div>
      <div class="row"><span>Use Fast mode<span class="sub">${s.fast_mode ? "Requested for new turns · uses more allowance" : canFast ? "Optional · uses the selected account’s allowance faster" : "The selected device must be ready and allow Fast mode"}</span></span>
        <button class="switch" role="switch" aria-label="Use Fast mode" aria-checked="${!!s.fast_mode}" data-toggle="fast" ${!s.fast_mode && !canFast ? "disabled" : ""}></button></div>` : ""}
    </div>
    <div class="eyebrow">Sharing</div><div class="group">
      <div class="row"><span>Share this ${dev}<span class="sub">${s.sharing ? `${s.local_account.state === "ready" ? "Codex account ready" : "Codex login not ready"} · <button class="btn link" data-go="devices">${s.clients.length} approved</button>` : "Off"}</span></span>
        <button class="switch" role="switch" aria-label="Share this device" aria-checked="${s.sharing}" data-toggle="sharing"></button></div>
      ${s.sharing ? `<div class="row"><span>Allow Fast mode<span class="sub">Let paired devices request faster responses</span></span>
        <button class="switch" role="switch" aria-label="Allow Fast mode" aria-checked="${!!s.allow_fast_mode}" data-toggle="allow-fast"></button></div>` : ""}
    </div>
    <div class="actions"><button class="btn primary" data-go="pair">Pair a device</button></div>
    ${recentWarnings(s)}
    <div class="group" style="margin-top:16px"><button class="row" data-go="details"><span>Connection details</span><span class="chev"></span></button></div>`;
}

function recentWarnings(s) {
  const warnings = (s.recent_warnings ?? []).slice(0, 4);
  if (!warnings.length) return "";
  return `<section id="recent-warnings" class="request-warnings" aria-label="Recent warnings">
    <div class="eyebrow">Recent warnings</div><div class="group">${warnings.map((warning) => `
      <div class="row" data-request-warning><span><span class="warning-title">${warning.retry ? `Retry ${esc(warning.retry)} of 3 scheduled` : "Request failed"}</span>
        <span class="sub">${esc(warning.message)}</span></span><span class="sub">${esc(ago(warning.at))}</span></div>`).join("")}</div></section>`;
}

function brand() {
  return `<div class="brand"><img src="logo.png" alt="">Aillies</div>`;
}

function welcome() {
  return `${brand()}<div class="steps"><span class="on"></span><span></span><span></span><span></span></div>
    <h1>Use another device’s account</h1>
    <p class="lead">Pair a trusted device to use its Codex login. Keep using Codex as usual.</p>
    <button class="btn primary block" data-go="pair-enter">Pair a device</button>
    <p class="note">The login never leaves the device that owns it. Requests are end-to-end encrypted between your devices.</p>
    <div class="actions" style="justify-content:center"><button class="btn link" data-do="share-only">I only want to share this device</button></div>`;
}

function pairEnter() {
  return `<button class="back" data-go="home">‹ Home</button>
    <h1>Pair a device</h1>
    <p class="lead">Enter the one-time code from the other device.</p>
    <label class="field-label" for="pair-code">Eight-digit pairing code</label>
    <input class="pairing-digits" id="pair-code" data-code inputmode="numeric" maxlength="9" placeholder="1234 5678" spellcheck="false" autocomplete="one-time-code" autofocus>
    <details class="pair-options"><summary>Use a full pairing code</summary><label class="field-label" for="full-pair-code">For private networks or older Aillies versions</label><textarea id="full-pair-code" data-full-code placeholder="aillies1:…" spellcheck="false" autocomplete="off"></textarea><button class="btn link" data-do="full-invite">Show a full code for this device</button></details>
    ${ui.error ? `<p class="note" style="color:var(--action)">${esc(ui.error)}</p>` : ""}
    <div class="actions"><button class="btn primary" data-do="start-pair" ${ui.waiting ? "disabled" : ""}>${ui.waiting ? '<span class="spin"></span>Connecting…' : "Continue"}</button></div>
    <div class="group" style="margin-top:24px"><button class="row" data-do="invite"><span>Show a pairing code<span class="sub">Let another device use this one</span></span><span class="chev"></span></button></div>
    <p class="note">Eight-digit codes work on the same local network. Use a full code over a private network such as Tailscale.</p>`;
}

function verify(s) {
  const p = ui.pairing;
  if (!p) return home(s);
  return `<div class="steps"><span class="on"></span><span class="on"></span><span class="on"></span><span></span></div>
    <h1>Check that the codes match</h1>
    <p class="lead">${esc(p.name)} shows a code too. Continue only if it is identical.</p>
    <div class="code">${esc(p.code)}</div>
    <div class="ids"><div><strong>${esc(s.name)}</strong><span>${esc(s.short_id)}</span></div><div><strong>${esc(p.name)}</strong><span>${esc(p.short_id)}</span></div></div>
    ${ui.error ? `<p class="note" style="color:var(--action)">${esc(ui.error)}</p>` : ""}
    <div class="actions">${ui.waiting ? `<span class="pending"><span class="spin"></span>Approve on ${esc(p.name)}…</span>` : `<button class="btn" data-do="cancel-pair">Cancel</button><button class="btn primary" data-do="confirm-pair">Codes match</button>`}</div>`;
}

function ready(s) {
  const p = ui.paired;
  if (!p) return home(s);
  const h = ui.health[p.fp];
  const acct = h?.status?.providers?.[0]?.account;
  const mark = (ok, wait) => (wait ? '<span class="spin"></span>' : ok ? '<span class="check">✓</span>' : '<span style="color:var(--action)">!</span>');
  const waiting = !h || h.checking;
  return `<div class="steps"><span class="on"></span><span class="on"></span><span class="on"></span><span class="on"></span></div>
    <h1>${esc(p.name)} is paired</h1><p class="lead">Each part is checked separately.</p>
    <div class="group">
      <div class="row"><span>Device identity<span class="sub">Approved on both devices</span></span>${mark(true)}</div>
      <div class="row"><span>Connection<span class="sub">${waiting ? "Checking…" : h.reachable ? `${esc(h.latency_ms)} ms` : "Unreachable"}</span></span>${mark(h?.reachable, waiting)}</div>
      <div class="row"><span>Codex account<span class="sub">${waiting ? "Checking…" : acct?.state === "ready" ? `Signed in on ${esc(p.name)}` : "Not ready on that device"}</span></span>${mark(acct?.state === "ready", waiting)}</div>
    </div>
    <p class="note">Codex requests through Aillies use ${esc(p.name)}’s account. The login stays on ${esc(p.name)}.</p>
    <div class="actions">${ui.status?.codex_installed ? "" : `<button class="btn" data-go="codex">Set up Codex</button>`}<button class="btn primary" data-go="home">Done</button></div>`;
}

function invite(s) {
  const p = s.pending[0];
  if (p) {
    return `<button class="back" data-go="home">‹ Home</button>
      <h1>Check that the codes match</h1><p class="lead">${esc(p.name)} (${esc(p.short_id)}) wants to use this device’s Codex account.</p>
      <div class="code">${esc(p.code)}</div>
      <div class="actions"><button class="btn" data-decide="${esc(p.id)}" data-approve="0">Decline</button><button class="btn primary" data-decide="${esc(p.id)}" data-approve="1">Codes match — approve</button></div>`;
  }
  return `<button class="back" data-go="home">‹ Home</button>
    <h1>Enter this code on the other device</h1>
    <p class="lead">It works once and expires in 10 minutes. Send it privately.</p>
    <div class="${/^\d{8}$/.test(ui.invite ?? "") ? "code pairing-number" : "mono"}" data-invite>${esc(/^\d{8}$/.test(ui.invite ?? "") ? ui.invite.slice(0,4)+" "+ui.invite.slice(4) : ui.invite ?? "")}</div>
    <div class="actions"><button class="btn" data-do="copy">Copy code</button><button class="btn" data-do="invite">New code</button></div>
    <p class="note">${/^\d{8}$/.test(ui.invite ?? "") ? 'Keep both devices on the same local network. <button class="btn link" data-do="full-invite">Use a full code instead</button>' : 'Use this full code on a shared or private network.'}</p>
    <p class="note"><span class="spin"></span>Waiting for the other device…</p>`;
}

function details(s) {
  const dest = s.destination.kind === "peer" ? s.destination.id : null;
  const h = dest ? ui.health[dest] : null;
  const peer = dest ? s.peers.find((p) => p.fp === dest) : null;
  return `<button class="back" data-go="home">‹ Home</button>
    <h1>Connection details</h1><p class="lead">Aillies uses the first address that answers with the paired identity. Changing paths never resends a request.</p>
    ${peer ? `<div class="eyebrow">${esc(peer.name)}</div><div class="group">
      <div class="row"><span>Path<span class="sub">${h?.reachable ? esc(h.addr) : "—"}</span></span><span class="trail">${h?.reachable ? `${esc(h.latency_ms)} ms` : h?.checking ? "…" : "Unreachable"}</span></div>
      <div class="row"><span>Known addresses<span class="sub">${peer.addrs.map(esc).join(" · ")}</span></span><span></span></div>
      <button class="row" data-do="recheck"><span>Check again</span><span class="chev"></span></button></div>` : ""}
    <div class="eyebrow">This device</div>
    <div class="group">
      <button class="row" data-go="codex"><span>Use Aillies with</span><span class="trail">Codex<span class="chev"></span></span></button>
      <button class="row" data-go="devices"><span>Devices</span><span class="trail">${s.peers.length + s.clients.length}<span class="chev"></span></span></button>
      <div class="row"><span>Name<span class="sub">ID ${esc(s.short_id)}</span></span><span class="trail"><input data-name value="${esc(s.name)}" maxlength="64" aria-label="Device name" style="width:150px"><button class="btn" data-do="rename">Save</button></span></div>
      <button class="row" data-do="export-diagnostics"><span>Export diagnostics<span class="sub">Latest 100 incoming requests and upstream attempts</span></span><span class="chev"></span></button>
      <div class="row"><span>Version<span class="sub">Aillies ${esc(s.version)} · ${esc(s.diagnostics_build ?? "")}</span></span><span></span></div>
    </div>
    <p class="note">Relay and remote pairing without a shared network are planned for a later release.</p>`;
}

function devices(s) {
  const dev = OS[s.platform] ?? "device";
  return `<button class="back" data-go="details">‹ Connection details</button>
    <h1>Devices</h1><p class="lead">Each device is approved and removed on its own.</p>
    <div class="eyebrow">You can send through</div>
    <div class="group">${s.peers.length ? s.peers.map((p) => `<div class="row"><span>${esc(p.name)}<span class="sub">${esc(p.platform)} · ${esc(p.short_id)}</span></span><button class="btn" data-forget="${esc(p.fp)}">Remove</button></div>`).join("") : '<div class="row"><span class="sub">None yet</span></div>'}</div>
    <div class="eyebrow">Allowed to use this ${dev}’s account</div>
    <div class="group">${s.clients.length ? s.clients.map((c) => `<div class="row"><span>${esc(c.name)}<span class="sub">${esc(c.platform)} · ${esc(c.short_id)}${c.last_used ? " · used " + ago(c.last_used) : ""}</span></span><button class="btn danger" data-revoke="${esc(c.fp)}">Revoke</button></div>`).join("") : '<div class="row"><span class="sub">None</span></div>'}</div>
    <div class="actions"><button class="btn primary" data-go="pair">Pair a device</button></div>`;
}

function codex(s) {
  const selected = ui.providerTarget ?? s.codex_installed;
  const progress = s.provider_switch;
  const progressLabel = progress?.phase === "scanning" ? "Checking your chats…"
    : progress?.phase === "preparing" ? `Preparing chat history…${progress.total ? ` ${progress.completed} of ${progress.total}` : ""}`
    : progress?.phase === "applying" ? "Saving your provider choice…" : "";
  const pending = ui.providerTarget === null ? "" : `Switch to ${selected ? "Aillies" : "Normal"}`;
  return `<button class="back" data-go="home">‹ Home</button>
    <h1>Use Aillies with</h1><p class="lead">Keep your chats. Choose how new requests are sent.</p>
    <div class="group"><div class="row"><span>Codex<span class="sub">${ui.switching ? (progressLabel || (ui.restart ? "Closing Codex and switching…" : "Updating your chats…")) : ui.restart ? pending : s.codex_installed ? "Through your selected Aillies device" : "Your normal Codex provider"}</span></span>
      <select data-provider aria-label="Codex provider" ${ui.switching || ui.restart ? "disabled" : ""}>
        <option value="normal" ${!selected ? "selected" : ""}>Normal</option>
        <option value="aillies" ${selected ? "selected" : ""}>Aillies</option>
      </select></div>
      <div class="row upcoming"><span>Claude Code</span><span class="badge">Upcoming</span></div>
    </div>
    ${s.codex_installed && !s.routing ? `<div class="callout action" style="margin-top:16px"><strong>Aillies needs to be enabled</strong><div class="actions"><button class="btn primary" data-do="migrate-chats" ${ui.switching || ui.restart ? "disabled" : ""}>Use Aillies</button></div></div>` : ""}
    ${ui.restartError ? `<div class="callout action" role="alert" style="margin-top:16px"><strong>Check Codex</strong><p>${esc(ui.restartError)}</p></div>` : ""}
    ${ui.restart ? `<div class="callout action" role="status" style="margin-top:16px"><strong>Ready to switch to ${ui.restart.on ? "Aillies" : "Normal"}?</strong>
      <p>${esc(ui.restart.apps.join(", ") || "Codex")}${ui.restart.terminals ? ` · ${ui.restart.terminals} terminal ${ui.restart.terminals === 1 ? "session" : "sessions"}` : ""} will close.</p>
      <p>Save editor changes and finish active work first. Your chat history stays intact. Only ChatGPT will reopen.</p>
      <div class="actions"><button class="btn" data-do="cancel-restart" ${ui.switching ? "disabled" : ""}>Cancel</button><button class="btn primary" data-do="restart-codex" ${ui.switching ? "disabled" : ""}>${ui.switching ? "Switching…" : "Quit, switch & reopen"}</button></div></div>` : ""}
    <p class="note">${s.platform === "macos" ? "Aillies can close Codex apps and terminal sessions, switch providers, then open ChatGPT." : "Quit Codex before switching, then reopen it."} Your messages stay intact. Aillies includes image creation through the selected device.</p>`;
}

function quit(s) {
  return `<button class="back" data-go="home">‹ Home</button><h1>Quit Aillies?</h1>
    <p class="lead">${s.codex_installed ? "Codex is using Aillies. Switch to Normal first to keep Codex available." : "Codex can keep using your normal provider."}</p>
    ${ui.restartError ? `<div class="callout action" role="alert">${esc(ui.restartError)}</div>` : ""}
    <div class="actions"><button class="btn" data-go="home">Cancel</button><button class="btn primary" data-do="${s.codex_installed ? "normal-and-quit" : "quit"}">${s.codex_installed ? "Switch to Normal…" : "Quit Aillies"}</button></div>`;
}

async function quitApp() {
  try { await invoke("quit_if_normal"); }
  catch (e) { ui.restartError = String(e); ui.quitAfterSwitch = false; go("quit"); }
}

async function switchProvider(on) {
  if (ui.switching || ui.restart) return;
  ui.restartError = null;
  ui.providerTarget = on;
  ui.switching = true;
  render();
  try {
    const result = await invoke("switch_codex_provider", { on });
    toast(result.reopen_required ? "Ready. Reopen Codex to continue your chats." : "Already up to date");
    await refresh();
    if (ui.quitAfterSwitch && !ui.status.codex_installed) await quitApp();
  } catch (e) {
    if (ui.status?.platform === "macos" && String(e).startsWith("Quit Codex, then switch providers here.")) {
      try { ui.restart = await invoke("prepare_codex_restart", { on }); }
      catch (reviewError) { ui.restartError = String(reviewError); toast(String(reviewError)); }
    } else { ui.restartError = String(e); toast(String(e)); }
  }
  finally { ui.switching = false; if (!ui.restart) { ui.providerTarget = null; ui.quitAfterSwitch = false; } render(); }
}

async function restartCodex() {
  if (ui.switching || !ui.restart) return;
  ui.switching = true;
  render();
  try {
    const result = await invoke("restart_codex_provider", { token: ui.restart.token });
    toast(result.reopen_failed ? "Provider switched. Open ChatGPT to continue your saved chats." : "Provider switched. Your saved chats are ready.");
  } catch (e) { ui.restartError = String(e); }
  finally { await refresh(); ui.restart = null; ui.providerTarget = null; ui.switching = false; if (ui.quitAfterSwitch && !ui.status.codex_installed) await quitApp(); ui.quitAfterSwitch = false; render(); }
}

function startupError() {
  return `${brand()}<h1 class="s-action"><span class="dot"></span>Aillies couldn’t start</h1><p class="lead">${esc(ui.startupError)}</p>
    <p class="note">If Aillies is already running from the terminal (<code>aillies run</code>), stop it and reopen the app.</p>`;
}

function ago(t) {
  const d = Math.max(0, Date.now() / 1000 - t);
  if (d < 90) return "just now";
  if (d < 3600) return `${Math.round(d / 60)} min ago`;
  if (d < 86400) return `${Math.round(d / 3600)} h ago`;
  return `${Math.round(d / 86400)} d ago`;
}

function render(fromPoll = false) {
  if (ui.startupError && !ui.status) return void ($app.innerHTML = startupError());
  const s = ui.status;
  if (!s) return;
  if (ui.screen === "home" && !s.peers.length && !s.sharing && !s.pending.length && !render.seenHome) ui.screen = "welcome";
  render.seenHome = true;
  const views = { home, welcome, "pair-enter": pairEnter, verify, ready, invite, details, devices, codex, quit };
  // Re-rendering every poll must not wipe what the person is typing.
  const active = document.activeElement;
  if (fromPoll && active && active.matches("textarea, input")) return;
  $app.innerHTML = (views[ui.screen] ?? home)(s);
}

// ---------- Events ----------

document.addEventListener("click", async (ev) => {
  const t = ev.target.closest("button");
  if (!t) return;
  if (t.dataset.go) return go(t.dataset.go);
  if (t.dataset.toggle) {
    const s = ui.status;
    if (t.dataset.toggle === "fast") return act(() => invoke("set_fast_mode", { on: !s.fast_mode }));
    if (t.dataset.toggle === "allow-fast") return act(() => invoke("set_allow_fast_mode", { on: !s.allow_fast_mode }));
    if (t.dataset.toggle === "sharing") return act(() => invoke("set_sharing", { on: !s.sharing }), s.sharing ? "Sharing off" : "Sharing on");
    if (t.dataset.toggle === "codex") return switchProvider(!s.codex_installed);
  }
  if (t.dataset.decide) {
    const approve = t.dataset.approve === "1";
    await act(() => invoke("decide_pairing", { id: t.dataset.decide, approve }), approve ? "Approved" : "Declined");
    if (ui.screen === "invite") go("home");
    return;
  }
  if (t.dataset.revoke) return act(() => invoke("revoke_client", { fp: t.dataset.revoke }), "Revoked");
  if (t.dataset.forget) return act(() => invoke("remove_peer", { fp: t.dataset.forget }), "Removed");
  switch (t.dataset.do) {
    case "cancel-restart":
      if (!ui.switching) { ui.restart = null; ui.providerTarget = null; ui.quitAfterSwitch = false; render(); }
      return;
    case "quit": return quitApp();
    case "normal-and-quit":
      go("codex"); ui.quitAfterSwitch = true; return switchProvider(false);
    case "restart-codex":
      return restartCodex();
    case "migrate-chats":
      return switchProvider(true);
    case "export-diagnostics":
      try {
        await invoke("export_diagnostics");
        toast("Diagnostics saved to Downloads");
      } catch (e) { toast(String(e)); }
      return;
    case "dismiss":
      ui.error = null;
      return act(() => invoke("dismiss_failure"));
    case "recheck": {
      const d = ui.status.destination;
      await invoke("dismiss_failure");
      if (d.kind === "peer") checkHealth(d.id);
      return refresh();
    }
    case "share-only":
      await act(() => invoke("set_sharing", { on: true }), "Sharing on").catch(() => {});
      return go("home");
    case "invite":
    case "full-invite":
      try {
        ui.invite = await invoke(t.dataset.do === "full-invite" ? "create_full_invite" : "create_invite");
        await refresh();
        go("invite");
      } catch (e) {
        toast(String(e));
      }
      return;
    case "copy":
      try {
        await navigator.clipboard.writeText(ui.invite);
        toast("Copied");
      } catch {
        toast("Select the code and copy it");
      }
      return;
    case "start-pair": {
      if (ui.waiting) return;
      const code = document.querySelector("[data-code]").value.trim() || document.querySelector("[data-full-code]")?.value.trim();
      if (!code) { ui.error = "Enter a pairing code to continue."; render(); return; }
      ui.waiting = true;
      ui.error = null;
      render();
      try {
        ui.pairing = await invoke("start_pairing", { code });
        ui.waiting = false;
        go("verify");
      } catch (e) {
        ui.waiting = false;
        ui.error = String(e);
        render();
      }
      return;
    }
    case "cancel-pair":
      await invoke("cancel_pairing", { id: ui.pairing.id }).catch(() => {});
      ui.pairing = null;
      return go("home");
    case "confirm-pair":
      if (ui.waiting) return;
      ui.waiting = true;
      ui.error = null;
      render();
      try {
        ui.paired = await invoke("confirm_pairing", { id: ui.pairing.id });
        ui.waiting = false;
        ui.pairing = null;
        ui.screen = "ready";
        checkHealth(ui.paired.fp);
        await refresh();
      } catch (e) {
        ui.waiting = false;
        ui.error = String(e);
        render();
      }
      return;
    case "rename": {
      const name = document.querySelector("[data-name]").value.trim();
      if (name) await act(() => invoke("set_name", { name }), "Renamed");
      return;
    }
  }
});

document.addEventListener("change", (ev) => {
  if (ev.target.matches("[data-provider]")) return switchProvider(ev.target.value === "aillies");
  if (ev.target.matches("[data-dest]")) {
    const target = ev.target.value;
    act(() => invoke("set_destination", { target }), "Your next request uses the selected device.");
    if (target !== "local") checkHealth(target);
  }
});

(async () => {
  ui.startupError = await invoke("startup_error").catch(() => null);
  await refresh();
  window.__TAURI__.event?.listen("aillies:navigate", () => refresh()).catch(() => {});
  setInterval(() => refresh(true), 2000);
  setInterval(() => {
    const d = ui.status?.destination;
    if (d?.kind === "peer" && ui.screen === "home") checkHealth(d.id);
  }, 15000);
})();
