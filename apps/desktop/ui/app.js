// Aillies window. Plain JS, no build step. All state comes from the node.
"use strict";

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const $app = document.getElementById("app");
const esc = (v) => String(v ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

const ui = {
  screen: "home",
  claudeClient: "desktop",
  claudeError: null,
  claudeSharing: false,
  claudeConnect: null,
  claudeConnecting: false,
  status: null,
  picker: null, discovering: false, discoveryEpoch: 0, diagnostics: null, diagnosticTab: "incoming", cleanup: null, exitError: null,
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
  ui.screen = screen === "pair" ? "pair-enter" : screen === "codex" ? "home" : screen;
  ui.picker = null;
  if (screen === "diagnostics") loadDiagnostics();
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
      go("home");
      if (typeof intent.on === "boolean" && (intent.on !== ui.status.codex_installed || intent.on && !ui.status.routing)) await switchProvider(intent.on);
    } else if (intent.screen === "activate_claude") {
      go("home"); await reviewActivation("claude", intent.on);
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

function offered(fp, service) {
  const h = ui.health[fp];
  return !!(h?.reachable && h.status?.sharing && h.status.providers?.some(p => p.provider === service && p.account?.state === "ready"));
}
function displayName(p) { return ui.health[p.fp]?.status?.name || p.name; }
function devicePicker(s, service) {
  const dest = service === "codex" ? s.destination : s.claude?.destination;
  const selected = dest?.kind === "peer" ? dest.id : "local";
  const peer = s.peers.find(p => p.fp === selected);
  const label = selected === "local" ? `This ${OS[s.platform] ?? "device"}` : peer ? displayName(peer) : "Unavailable device";
  const localReady = service === "codex" ? s.local_account.state === "ready" : s.claude?.connected;
  return `<div class="device-picker"><button class="device-select" data-picker="${service}" aria-expanded="${ui.picker === service}" aria-label="${service === "codex" ? "Codex" : "Claude"} device" ${ui.switching || ui.restart ? "disabled" : ""}>${esc(label)}<span class="chev"></span></button>
    ${ui.picker === service ? `<div class="device-menu" aria-label="${service} device choices">
      <button class="row" data-select-service="${service}" data-target="local" ${!localReady ? "disabled" : ""}><span>This ${OS[s.platform] ?? "device"}<span class="sub">${localReady ? "Your own account" : service === "claude" ? "Connect an account first" : "Sign in to Codex first"}</span></span>${selected === "local" ? '<span class="check">✓</span>' : ""}</button>
      ${s.peers.map(p => `<button class="row" data-select-service="${service}" data-target="${esc(p.fp)}" ${!offered(p.fp, service) ? "disabled" : ""}><span>${esc(displayName(p))}<span class="sub">${offered(p.fp, service) ? `${service === "codex" ? "Codex" : "Claude"} shared` : ui.health[p.fp]?.checking || !ui.health[p.fp] ? "Checking…" : "Not available"}</span></span>${selected === p.fp ? '<span class="check">✓</span>' : ""}</button>`).join("")}
      <p class="note picker-note">${ui.discovering ? "Checking approved paired devices…" : "Only approved devices appear here."}</p></div>` : ""}</div>`;
}
async function discoverDevices(service) {
  ui.picker = ui.picker === service ? null : service;
  if (!ui.picker) return render();
  if (ui.discovering) return render();
  ui.discovering = true; render();
  const peers = [...ui.status.peers];
  for (let i = 0; i < peers.length; i += 4) await Promise.all(peers.slice(i, i + 4).map(p => checkHealth(p.fp)));
  ui.discovering = false; render();
}
function activationReview() {
  if (!ui.restart && !ui.switching && !ui.restartError) return "";
  const p = ui.restart, quit = p?.service === "quit";
  return `<div class="callout action activation-review" role="status">
    <strong>${ui.switching ? "Updating your provider…" : quit ? "Disconnect & quit?" : p ? `${p.on ? "Activate Aillies" : "Use Normal"} for ${p.service === "codex" ? "Codex" : "Claude"}?` : "The change stopped"}</strong>
    ${ui.restartError ? `<p role="alert">${esc(ui.restartError)}</p>` : ""}
    ${p?.manual_close ? '<p>Save your work and close the affected desktop apps and code sessions before continuing.</p><p class="note">Reopen desktop apps yourself after switching.</p>' : p ? `<p>${esc(p.apps.join(", ") || "Affected clients")}${p.terminals ? ` and ${p.terminals} code sessions` : ""} will close. Save your work first.</p>
      <p class="note">Only desktop apps reopen. Terminal and VS Code stay closed.${p.service === "claude" ? " Desktop Chat and Claude Code switch together." : ""}</p>` : ""}
    ${ui.status?.provider_switch ? `<p class="note">Updating chat history… ${esc(ui.status.provider_switch.completed)} of ${esc(ui.status.provider_switch.total)}</p>` : ""}
    ${p ? `<div class="actions"><button class="btn" data-do="cancel-restart" ${ui.switching ? "disabled" : ""}>Cancel</button><button class="btn primary" data-do="restart-codex" ${ui.switching ? "disabled" : ""}>${quit ? "Disconnect & quit" : p.manual_close ? "Switch provider" : "Quit, switch & reopen"}</button></div>` : ""}</div>`;
}
function home(s) {
  const c = s.claude ?? {}, claudeOn = !!(c.installed || c.desktop_installed);
  const incoming = s.peers.filter(p => offered(p.fp, "codex") || offered(p.fp, "claude")).length;
  const outgoing = s.clients.filter(p => s.sharing && p.codex !== false || c.sharing && p.claude).length;
  const pending = s.pending[0];
  const selected = (service, current) => ui.restart?.service === service ? ui.restart.on : current;
  return `${brand()}<h1>Aillies</h1><p class="lead connection-state"><span class="dot ${s.last_failure ? "s-action" : s.codex_installed || claudeOn || s.sharing || c.sharing ? "s-ready" : ""}"></span>${s.last_failure ? "Action required" : s.codex_installed || claudeOn || s.sharing || c.sharing ? "Connected" : "Ready to connect"}</p>
    ${pending ? `<div class="callout info"><strong>${esc(pending.name)} wants to pair</strong><p>Approve only if both devices show ${esc(pending.code)}.</p><div class="actions"><button class="btn" data-decide="${esc(pending.id)}" data-approve="0">Decline</button><button class="btn primary" data-decide="${esc(pending.id)}" data-approve="1">Codes match — approve</button></div></div>` : ""}
    ${ui.error ? `<div class="callout action" role="alert">${esc(ui.error)}</div>` : ""}
    <h2 class="section-title">Aillies Use</h2><p class="lead">Use your own account or one shared with you.</p>
    <div class="group use-group"><div class="row activation-row"><span>Activate Aillies</span><span class="activation-toggles">
      <span class="toggle-label">Codex<button class="switch" role="switch" aria-label="Activate Aillies for Codex" aria-checked="${selected("codex", s.codex_installed)}" data-toggle="codex" ${ui.switching || ui.restart ? "disabled" : ""}></button></span>
      <span class="toggle-label">Claude<button class="switch" role="switch" aria-label="Activate Aillies for Claude" aria-checked="${selected("claude", claudeOn)}" data-toggle="claude" ${ui.switching || ui.restart ? "disabled" : ""}></button></span></span></div>
      <div class="row"><span>Codex device</span>${devicePicker(s, "codex")}</div>
      <div class="row"><span>Claude device</span>${devicePicker(s, "claude")}</div></div>
    <p class="note">${s.platform === "macos" ? "Changing activation quits and reopens the affected desktop apps." : "Close affected apps before switching, then reopen them yourself."}</p>
    <p class="note" data-sharing-in>${incoming === 1 ? "1 Ailly is" : `${incoming} Aillies are`} sharing with you.</p>
    ${activationReview()}
    <h2 class="section-title">Aillies Share</h2><p class="lead">Share accounts signed in on this ${OS[s.platform] ?? "device"}.</p>
    <div class="group"><div class="row"><span>Codex<span class="sub ${s.local_account.state === "ready" ? "account-ready" : ""}">${s.local_account.state === "ready" ? "Signed in on this device" : "Sign in to Codex on this device"}</span></span><span class="toggle-label">Share<button class="switch" role="switch" aria-label="Share Codex" aria-checked="${s.sharing}" data-toggle="sharing" ${!s.sharing && s.local_account.state !== "ready" ? "disabled" : ""}></button></span></div>
    ${s.sharing ? `<div class="row indented"><span>Allow Fast mode</span><button class="switch" role="switch" aria-label="Allow Fast mode" aria-checked="${!!s.allow_fast_mode}" data-toggle="allow-fast"></button></div>` : ""}
    <div class="row"><span>Claude<span class="sub ${c.connected ? "account-ready" : ""}">${c.connected ? "Connected · saved on this device" : c.reconnect_required ? "Reconnect to use private local storage" : "No account connected"}</span></span><span class="toggle-label">${c.connected || c.sharing ? `Share<button class="switch" role="switch" aria-label="Share Claude" aria-checked="${!!c.sharing}" data-toggle="claude-sharing" ${ui.claudeSharing || ui.claudeConnecting ? "disabled" : ""}></button>` : `<button class="btn" data-go="claude">${c.reconnect_required ? "Reconnect" : "Connect account"}</button>`}</span></div></div>
    ${ui.claudeError ? `<div class="callout action" role="alert">${esc(ui.claudeError)}</div>` : ""}
    <p class="note" data-sharing-out>You’re sharing with ${outgoing} ${outgoing === 1 ? "Ailly" : "Aillies"}.</p>
    <div class="actions paired-actions"><button class="btn primary" data-go="pair">Pair a device…</button><button class="btn link" data-go="devices">Manage paired devices ›</button></div>
    <div class="group details-entry"><button class="row" data-go="details"><span>Connection details</span><span class="chev"></span></button></div>
    ${recentWarnings(s)}
    <div class="actions home-footer"><button class="btn link" data-go="claude">Manage Claude account</button><button class="btn" data-go="quit">Disconnect & quit…</button></div>`;
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
    <p class="lead">Pair a trusted device to use its shared Codex or Claude account. Choose a device for each service in Aillies Use.</p>
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
  const providers = h?.status?.providers ?? [];
  const mark = (ok, wait) => (wait ? '<span class="spin"></span>' : ok ? '<span class="check">✓</span>' : '<span style="color:var(--action)">!</span>');
  const waiting = !h || h.checking;
  return `<div class="steps"><span class="on"></span><span class="on"></span><span class="on"></span><span class="on"></span></div>
    <h1>${esc(p.name)} is paired</h1><p class="lead">Each part is checked separately.</p>
    <div class="group">
      <div class="row"><span>Device identity<span class="sub">Approved on both devices</span></span>${mark(true)}</div>
      <div class="row"><span>Connection<span class="sub">${waiting ? "Checking…" : h.reachable ? `${esc(h.latency_ms)} ms` : "Unreachable"}</span></span>${mark(h?.reachable, waiting)}</div>
      ${["codex", "claude"].map(service => {
        const offered = providers.find(provider => provider.provider === service);
        const available = offered?.account?.state === "ready";
        return `<div class="row"><span>${service === "codex" ? "Codex" : "Claude"}<span class="sub">${waiting ? "Checking…" : available ? `Shared by ${esc(p.name)}` : "Not shared with this device"}</span></span>${waiting ? mark(false, true) : available ? mark(true) : '<span class="trail">Off</span>'}</div>`;
      }).join("")}
    </div>
    <p class="note">Choose this device for Codex or Claude in Aillies Use. Its account login stays on ${esc(p.name)}.</p>
    <div class="actions"><button class="btn primary" data-go="home">Open Aillies Use</button></div>`;
}

function invite(s) {
  const p = s.pending[0];
  if (p) {
    return `<button class="back" data-go="home">‹ Home</button>
      <h1>Check that the codes match</h1><p class="lead">${esc(p.name)} (${esc(p.short_id)}) wants to pair with this device. Service permissions are managed in paired devices.</p>
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
  return `<button class="back" data-go="home">‹ Overview</button><h1>Connection details</h1>
    <div class="group details-group"><div class="row"><label for="device-name">This ${OS[s.platform] ?? "device"}</label><span class="trail rename"><input id="device-name" data-name value="${esc(s.name)}" maxlength="64" aria-label="Device name"><button class="btn" data-do="rename">Save</button></span></div>
    <div class="row"><span>Version</span><span class="trail">${esc(s.version)}</span></div>
    <button class="row" data-go="diagnostics"><span>Diagnostics</span><span class="trail">Open<span class="chev"></span></span></button></div>`;
}
async function loadDiagnostics() {
  try { ui.diagnostics = await invoke("diagnostics"); }
  catch (_) { ui.diagnostics = null; ui.error = "Cannot load diagnostics. Try again."; }
  if (ui.screen === "diagnostics") render();
}
function diagnostics(s) {
  const d = ui.diagnostics, tab = ui.diagnosticTab;
  const entries = [...(d?.[tab] ?? [])].reverse();
  return `<button class="back" data-go="details">‹ Connection details</button><h1>Diagnostics</h1><p class="lead">Latest 100 in each log. Credentials and chat content are excluded.</p>
    ${ui.error ? `<div class="callout action" role="alert">${esc(ui.error)}</div>` : ""}
    <div class="actions diagnostics-actions"><button class="btn" data-do="refresh-diagnostics">Refresh</button><button class="btn primary" data-do="export-diagnostics">Export diagnostics</button></div>
    ${recentWarnings(s)}<div class="diagnostic-tabs" aria-label="Request log">${[["incoming","Incoming"],["upstream","Upstream"],["entries","Warnings & errors"]].map(([key,label])=>`<button class="btn ${tab === key ? "primary" : ""}" data-log-tab="${key}" aria-pressed="${tab === key}">${label}</button>`).join("")}</div>
    ${!d ? '<p class="note">Loading diagnostics…</p>' : !entries.length ? '<p class="note">No activity recorded yet.</p>' : `<div class="group diagnostic-log">${entries.map(e=>`<details><summary class="row"><span>${esc(e.request?.provider === "claude" ? "Claude" : "Codex")} · ${esc(e.event?.phase?.replaceAll("_"," ") || e.code?.replaceAll("_"," "))}<span class="sub">${esc(ago(e.at_unix_seconds))}</span></span><span class="chev"></span></summary><dl class="log-fields">${Object.entries({Model:e.request?.model,Stage:tab === "entries" ? e.level : tab,Attempt:e.request?.attempt,Retry:e.retry,Route:e.request?.route,"Reasoning effort":e.request?.reasoning_effort,"Service tier":e.request?.service_tier,"Input items":e.request?.input_items,Tools:e.request?.tools,"Image tool":e.request?.image_tool,Lite:e.request?.lite,"Trace ID":e.request?.trace_id,Result:e.event?.failure||e.code,Status:e.event?.status||e.upstream_status,"Elapsed (ms)":e.event?.elapsed_ms,"First byte (ms)":e.event?.first_byte_ms}).filter(([,v])=>v!==undefined&&v!==null).map(([k,v])=>`<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join("")}</dl></details>`).join("")}</div>`}
    ${d?.persistence_ok === false ? '<p class="note">Some activity could not be saved. This view includes the current in-memory log.</p>' : ""}`;
}

function devices(s) {
  const dev = OS[s.platform] ?? "device";
  return `<button class="back" data-go="home">‹ Overview</button>
    <h1>Paired devices</h1><p class="lead">Each device is approved and removed on its own.</p>
    <div class="eyebrow">You can send through</div>
    <div class="group">${s.peers.length ? s.peers.map((p) => `<div class="row"><span>${esc(p.name)}<span class="sub">${esc(p.platform)} · ${esc(p.short_id)}</span></span><button class="btn" data-forget="${esc(p.fp)}">Remove</button></div>`).join("") : '<div class="row"><span class="sub">None yet</span></div>'}</div>
    <div class="eyebrow">Allowed to use this ${dev}’s account</div>
    <div class="group">${s.clients.length ? s.clients.map((c) => `<div class="row"><span>${esc(c.name)}<span class="sub">${esc(c.platform)} · ${esc(c.short_id)}${c.last_used ? " · used " + ago(c.last_used) : ""}</span></span><span class="device-grants"><span class="toggle-label">Codex<button class="switch" role="switch" aria-label="Allow ${esc(c.name)} to use Codex" aria-checked="${c.codex !== false}" data-codex-grant="${esc(c.fp)}" data-on="${c.codex === false}"></button></span><span class="toggle-label">Claude<button class="switch" role="switch" aria-label="Allow ${esc(c.name)} to use Claude" aria-checked="${!!c.claude}" data-claude-grant="${esc(c.fp)}" data-on="${!c.claude}"></button></span><button class="btn danger" data-revoke="${esc(c.fp)}">Revoke</button></span></div>`).join("") : '<div class="row"><span class="sub">None</span></div>'}</div>
    <div class="actions"><button class="btn primary" data-go="pair">Pair a device</button></div>`;
}

function claude(s) {
  const c = s.claude ?? {}, canConnect = s.platform === "macos";
  return `<button class="back" data-go="home">‹ Overview</button><h1>Claude account</h1><p class="lead">Connect once to use or share this account from this device.</p>
    <div class="group"><div class="row"><span>Claude<span class="sub">${c.connected ? "Connected · private local storage" : c.reconnect_required ? "Reconnect once to move away from Keychain" : "No account connected"}</span></span><button class="btn" data-do="${c.connected ? "claude-disconnect" : "claude-connect"}" ${ui.claudeConnecting || !!ui.claudeConnect || c.sharing || !canConnect ? "disabled" : ""}>${c.connected ? "Disconnect" : "Connect account"}</button></div></div>
    ${c.sharing ? '<p class="note">Turn off Claude sharing before changing this account.</p>' : ""}
    ${ui.claudeError ? `<div class="callout action" role="alert">${esc(ui.claudeError)}</div>` : ""}
    ${ui.claudeConnect ? `<div class="callout oauth-form" role="status"><strong>Authorize in your browser</strong><p>Approve the Claude account you want to use, then paste the complete one-time code here.</p>
      <input type="password" data-claude-oauth-code aria-label="Claude authorization code" autocomplete="off" spellcheck="false" maxlength="8192" placeholder="Paste authorization code" ${ui.claudeConnecting ? "disabled" : ""}>
      <div class="actions"><button class="btn" data-do="claude-connect-cancel">Cancel</button><button class="btn" data-do="claude-connect-open" ${ui.claudeConnecting ? "disabled" : ""}>Open browser</button><button class="btn primary" data-do="claude-connect-finish" ${ui.claudeConnecting ? "disabled" : ""}>${ui.claudeConnecting ? "Connecting…" : "Connect"}</button></div>
      ${ui.claudeConnect.manual ? `<p class="note">Open this link manually:</p><input aria-label="Claude authorization link" readonly value="${esc(ui.claudeConnect.url)}">` : ""}</div>` : ""}
    <p class="note">Aillies saves this connection in its private data folder, without Keychain prompts. Your normal Claude login and chats stay unchanged.</p>
    ${!canConnect ? '<p class="note">Connect accounts on a Mac, or use an account shared by a paired Mac.</p>' : ""}`;
}

function quit(s) {
  const cleanup = ui.cleanup;
  return `<button class="back" data-go="home">‹ Overview</button><h1>Disconnect & quit</h1><p class="lead">Stop sharing and return your clients to their normal providers. Keep your other settings and chats.</p>
    ${ui.exitError ? `<div class="callout action" role="alert">${esc(ui.exitError)}</div>` : ""}
    ${cleanup ? `<div class="group">${[["codex","Codex"],["claude_code","Claude Code"],["claude_desktop","Claude desktop"]].map(([key,label])=>`<div class="row"><span>${label}</span><span class="${cleanup[key] ? "account-ready" : "warning-title"}">${cleanup[key] ? "Disconnected" : "Cleanup incomplete"}</span></div>`).join("")}</div><p class="note">Recovery files were kept. Affected clients stay closed when cleanup is incomplete.</p>` : ""}
    ${activationReview()}<div class="actions"><button class="btn" data-go="home">Cancel</button>${!ui.restart && !ui.switching ? `<button class="btn primary" data-do="disconnect-quit">${cleanup || ui.exitError ? "Try cleanup again" : "Disconnect & quit"}</button>` : ""}${(cleanup && !cleanup.complete || ui.exitError) && !ui.switching ? '<button class="btn" data-do="quit-anyway">Quit anyway</button>' : ""}</div>`;
}
async function switchProvider(on) { return reviewActivation("codex", on); }
async function reviewActivation(service, on) {
  if (ui.switching || ui.restart) return;
  ui.picker = null; ui.switching = true; ui.restartError = null; ui.exitError = null; render();
  try { ui.restart = await invoke("prepare_activation", {service, on}); }
  catch (e) { if(service === "quit") ui.exitError = String(e); else ui.restartError = String(e); }
  finally { ui.switching = false; render(); }
}
async function restartCodex() {
  if (ui.switching || !ui.restart) return;
  const service = ui.restart.service;
  ui.switching = true; ui.restartError = null; render();
  try {
    const report = await invoke("apply_activation", {token:ui.restart.token});
    if (service === "quit") {
      ui.cleanup = report;
      if (report.complete) { await invoke("quit_if_normal"); return; }
    } else if (report.reopen_failed) toast("Provider changed. Open the desktop app to continue.");
    else toast(ui.restart.manual_close ? "Provider switched. Reopen your desktop app to continue." : "Provider switched. Your desktop app is ready.");
  } catch (e) { if(service === "quit") ui.exitError = String(e); else ui.restartError = String(e); }
  finally { ui.restart = null; ui.switching = false; await refresh(); render(); }
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
  render.seenHome = true;
  const views = { home, welcome, "pair-enter": pairEnter, verify, ready, invite, details, diagnostics, devices, claude, quit };
  // Re-rendering every poll must not wipe what the person is typing.
  const active = document.activeElement;
  if (fromPoll && ((ui.screen === "claude" && ui.claudeConnect) || (active && active.matches("textarea, input, select")))) return;
  $app.innerHTML = (views[ui.screen] ?? home)(s);
}

// ---------- Events ----------

document.addEventListener("click", async (ev) => {
  const t = ev.target.closest("button");
  if (!t) return;
  if (t.dataset.go) return go(t.dataset.go);
  if (t.dataset.picker) return discoverDevices(t.dataset.picker);
  if (t.dataset.selectService) {
    if (ui.switching || ui.restart) return;
    const service=t.dataset.selectService, target=t.dataset.target;
    ui.picker=null;
    return act(() => invoke(service === "codex" ? "set_destination" : "set_claude_destination", {target}));
  }
  if (t.dataset.logTab) { ui.diagnosticTab=t.dataset.logTab; return render(); }
  if (t.dataset.toggle) {
    const s = ui.status;
    if (t.dataset.toggle === "fast") return act(() => invoke("set_fast_mode", { on: !s.fast_mode }));
    if (t.dataset.toggle === "allow-fast") return act(() => invoke("set_allow_fast_mode", { on: !s.allow_fast_mode }));
    if (t.dataset.toggle === "sharing") return act(() => invoke("set_sharing", { on: !s.sharing }), s.sharing ? "Sharing off" : "Sharing on");
    if (t.dataset.toggle === "claude-sharing") {
      if (ui.claudeSharing) return;
      ui.claudeSharing = true; ui.claudeError = null; render();
      try { await invoke("set_claude_sharing", { on: !s.claude.sharing, source: "connected" }); await refresh(); }
      catch (e) { ui.claudeError = String(e); }
      finally { ui.claudeSharing = false; render(); }
      return;
    }
    if (t.dataset.toggle === "codex") return switchProvider(!s.codex_installed);
    if (t.dataset.toggle === "claude") return reviewActivation("claude", !(s.claude?.installed || s.claude?.desktop_installed));
  }
  if (t.dataset.decide) {
    const approve = t.dataset.approve === "1";
    await act(() => invoke("decide_pairing", { id: t.dataset.decide, approve }), approve ? "Approved" : "Declined");
    if (ui.screen === "invite") go("home");
    return;
  }
  if (t.dataset.codexGrant) return act(() => invoke("grant_codex", { fp: t.dataset.codexGrant, on: t.dataset.on === "true" }));
  if (t.dataset.claudeGrant) return act(() => invoke("grant_claude", { fp: t.dataset.claudeGrant, on: t.dataset.on === "true" }));
  if (t.dataset.revoke) return act(() => invoke("revoke_client", { fp: t.dataset.revoke }), "Revoked");
  if (t.dataset.forget) return act(() => invoke("remove_peer", { fp: t.dataset.forget }), "Removed");
  switch (t.dataset.do) {
    case "claude-connect":
      if (ui.claudeConnecting || ui.claudeConnect) return;
      ui.claudeConnecting = true; ui.claudeError = null; render();
      try {
        ui.claudeConnect = { url: await invoke("begin_claude_connect"), manual: false };
        try { await invoke("open_claude_authorization"); } catch (_) { ui.claudeConnect.manual = true; }
      } catch (e) { ui.claudeError = String(e); }
      finally { ui.claudeConnecting = false; render(); }
      return;
    case "claude-connect-open":
      try { await invoke("open_claude_authorization"); } catch (_) { if(ui.claudeConnect) ui.claudeConnect.manual = true; render(); }
      return;
    case "claude-connect-cancel":
      await invoke("cancel_claude_connect"); ui.claudeConnect = null; ui.claudeError = null; render(); return;
    case "claude-connect-finish": {
      if (ui.claudeConnecting) return;
      const input = document.querySelector("[data-claude-oauth-code]");
      const code = input.value.trim(); input.value = "";
      if (!code) { ui.claudeError = "Paste the authorization code from Claude first."; render(); return; }
      ui.claudeConnecting = true; ui.claudeError = null; render();
      try { await invoke("finish_claude_connect", { code }); ui.claudeConnect = null; await refresh(); }
      catch (e) { ui.claudeError = String(e); ui.claudeConnect = null; }
      finally { ui.claudeConnecting = false; render(); }
      return;
    }
    case "claude-disconnect":
      if (ui.claudeConnecting) return;
      ui.claudeConnecting = true; ui.claudeError = null; render();
      try { await invoke("disconnect_claude_account"); await refresh(); }
      catch (e) { ui.claudeError = String(e); await refresh(); }
      finally { ui.claudeConnecting = false; render(); }
      return;


    case "cancel-restart":
      if (!ui.switching) { await invoke("cancel_activation"); ui.restart=null; ui.restartError=null; render(); } return;
    case "disconnect-quit": return reviewActivation("quit", false);
    case "quit-anyway": return invoke("quit_anyway");
    case "refresh-diagnostics": return loadDiagnostics();
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


(async () => {
  ui.startupError = await invoke("startup_error").catch(() => null);
  await refresh();
  if(ui.status?.peers.length) { await discoverDevices("codex"); ui.picker=null; render(); }
  window.__TAURI__.event?.listen("aillies:navigate", () => refresh()).catch(() => {});
  setInterval(() => refresh(true), 2000);
  setInterval(() => {
    const d = ui.status?.destination;
    if (d?.kind === "peer" && ui.screen === "home") checkHealth(d.id);
  }, 15000);
})();
