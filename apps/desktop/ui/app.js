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
  inviteCreated: 0,
  creatingInvite: false,
  invitePermissions: {codex:true,claude:false},
  inviteConfigured: false,
  activationIntent: null, activationMode:"now", lastActivationNotice:null,
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
  setupDone: false, pairCode: "", fullPairCode: "", pairingEpoch: 0,
  deviceRemoval: null,
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
  if (screen === "invite" && !ui.inviteConfigured) {
    const codex=ui.status?.local_account?.state === "ready";
    ui.invitePermissions={codex,claude:!codex && !!ui.status?.claude?.connected};
    ui.inviteConfigured=true;
  }
  if (screen === "home") {
    ui.setupDone = true;
    try { localStorage.setItem("aillies.setup.v1", "done"); } catch (_) { /* Optional local UI preference. */ }
  }
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
  const operation=s?.activation;
  if (operation && operation.mode !== "now" && ["completed","failed"].includes(operation.phase) && ui.lastActivationNotice !== `${operation.id}:${operation.phase}`) {
    ui.lastActivationNotice=`${operation.id}:${operation.phase}`;
    toast(operation.phase === "completed" ? operation.history_pending ? historyNotice(operation) : operation.reopen_failed ? "Provider switched. Open the desktop app to continue." : operation.mode === "manual" || s.platform !== "macos" ? "Provider switched. Reopen your desktop app to continue." : "Provider switched. Your desktop app is ready." : "The scheduled switch stopped. Open Aillies to review it.");
    if (ui.screen === "waiting" && operation.phase === "completed") ui.screen="home";
  }
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
  render(true);
}

function peerName(fp) {
  return ui.status?.peers.find((p) => p.fp === fp)?.name ?? "the device";
}

// ---------- Screens ----------

function offered(fp, service) {
  const h = ui.health[fp];
  return !!(h?.reachable && h.status?.sharing && h.status.providers?.some(p => p.provider === service && p.account?.state === "ready"));
}
function displayName(p) { return ui.health[p.fp]?.status?.name || p.name; }
function serviceReadiness(s, service, fp) {
  if (fp === "local") {
    const ready = service === "codex" ? s.local_account?.state === "ready" : s.claude?.connected && !s.claude?.reconnect_required;
    return { ready, label: ready ? "Your own account" : service === "claude" ? "Connect an account first" : "Sign in to Codex first", tone: ready ? "ready" : "idle" };
  }
  const h = ui.health[fp];
  if (!h || h.checking) return { ready: false, label: "Checking…", tone: "idle" };
  if (!h.reachable) return { ready: false, label: "Unreachable", tone: "action" };
  const provider = h.status?.providers?.find(p => p.provider === service);
  if (!h.status?.sharing || !provider) return { ready: false, label: "Not shared", tone: "idle" };
  if (provider.account?.state !== "ready") return { ready: false, label: provider.account?.state === "expired" ? "Login expired" : "Account needs attention", tone: "action" };
  return { ready: true, label: `Ready${h.latency_ms != null ? ` · ${h.latency_ms} ms` : ""}`, tone: "ready" };
}
function devicePicker(s, service) {
  const dest = service === "codex" ? s.destination : s.claude?.destination;
  const selected = dest?.kind === "peer" ? dest.id : "local";
  const peer = s.peers.find(p => p.fp === selected);
  const label = selected === "local" ? `This ${OS[s.platform] ?? "device"}` : peer ? displayName(peer) : "Unavailable device";
  const readiness = serviceReadiness(s, service, selected);
  const choice = (fp, name) => {
    const r = serviceReadiness(s, service, fp);
    return `<button class="row device-option" role="menuitem" data-select-service="${service}" data-target="${esc(fp)}" ${!r.ready ? "disabled" : ""}><span>${esc(name)}<span class="sub">${fp === "local" && r.ready ? "Your own account" : esc(r.label)}</span></span>${selected === fp ? '<span class="check">✓</span>' : ""}</button>`;
  };
  return `<div class="device-picker"><button class="device-select" data-picker="${service}" aria-haspopup="menu" aria-expanded="${ui.picker === service}" aria-label="${service === "codex" ? "Codex" : "Claude"} device" ${ui.switching || ui.restart ? "disabled" : ""}><span class="device-caption">Device</span><span class="device-name">${esc(label)}</span><span class="readiness ${readiness.tone}"><span class="dot s-${readiness.tone}"></span>${esc(readiness.label)}</span><span class="chev"></span></button>
    ${ui.picker === service ? `<div class="device-menu" role="menu" aria-label="${service} device choices">${choice("local", `This ${OS[s.platform] ?? "device"}`)}${s.peers.map(p => choice(p.fp, displayName(p))).join("")}
      <p class="note picker-note">${ui.discovering ? "Checking approved paired devices…" : "Only approved devices appear here."}</p><button class="row" role="menuitem" data-go="pair"><span>Pair a device…</span><span class="chev"></span></button></div>` : ""}</div>`;
}
function fastAllowed(s) {
  if (s.destination.kind === "local") return s.local_account?.state === "ready" && s.allow_fast_mode;
  const h = ui.health[s.destination.id];
  return offered(s.destination.id, "codex") && h?.status?.providers?.find(p => p.provider === "codex")?.allow_fast_mode === true;
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
function activationOptions(service, on) {
  const mode=ui.activationMode, mac=ui.status.platform === "macos";
  const choices=mac ? [["now","Quit, switch & reopen","Aillies closes affected clients, switches, then reopens desktop apps only."],["later",`When I next quit ${service}`,"Keep working. Aillies switches after every affected client is closed."],["manual","I’ll close them myself","No clients are closed for you. Aillies switches once they’re closed."]] : [["manual","I’ll close them myself","Close affected clients yourself. Aillies switches when they’re closed; reopen desktop apps manually."]];
  return `<div class="activation-options" role="radiogroup" aria-label="When to switch">${choices.map(([id,title,desc])=>`<button class="card ${mode === id ? "selected" : ""}" role="radio" aria-checked="${mode === id}" data-activation-mode="${id}" ${ui.switching ? "disabled" : ""}><span class="radio-dot"></span><span><strong>${esc(title)}</strong><span>${esc(desc)}</span></span></button>`).join("")}</div>`;
}
function activationTimeline(v) {
  if (!v) return "";
  const codex=v.service === "codex", h=ui.status.provider_switch;
  const steps=[["Closing affected clients",v.closed,v.phase === "closing"],...(codex?[["Saved recovery journal",v.recovery_saved,v.phase === "history" && !v.recovery_saved],["Updating chat history",v.history_done,v.phase === "history"]]:[]),["Updating provider settings",v.settings_done,v.phase === "settings"],[v.mode === "manual" || ui.status.platform !== "macos" ? "Desktop apps ready for you to reopen" : "Reopening desktop apps only",v.phase === "completed",v.phase === "reopening"]];
  return `<ol class="timeline">${steps.map(([label,done,active])=>`<li class="${done ? "done" : active ? "current" : ""}"><span class="step-mark">${done ? "✓" : active ? '<span class="spin"></span>' : "○"}</span><span>${label}${active && label === "Updating chat history" && h ? `<span class="sub">${esc(h.completed)} of ${esc(h.total)}</span>` : ""}</span></li>`).join("")}</ol>`;
}
function activationFailure() {
  const v=ui.status.activation, i=ui.activationIntent ?? v ?? {service:"codex",on:true};
  const service=i.service === "claude" ? "Claude" : "Codex";
  return `${brand()}<button class="back" data-do="keep-provider">‹ Home</button><div class="eyebrow">Action required</div><h1>${v?.failed_stage === "closing" ? `${service} didn’t quit` : "The switch stopped"}</h1>
    <div class="callout action" role="alert"><p>${esc(ui.restartError || v?.error || "Review the switch again before continuing.")}</p>${v?.failed_stage === "closing" ? '<p class="note">The provider was not switched. Answer any running-task or save dialog, then try again.</p>' : ""}</div>
    ${ui.status.platform === "macos" ? '<div class="group"><div class="row"><span>If macOS blocked Aillies<span class="sub">System Settings → Privacy & Security → Automation</span></span><button class="btn" data-do="open-automation">Open Automation settings</button></div></div>' : ""}
    <div class="eyebrow">Switch without closing clients for you</div><div class="choice">${ui.status.platform === "macos" ? '<button class="card" data-recover-mode="later"><span><strong>When I next quit</strong><span>Keep working for now.</span></span><span class="chev"></span></button>' : ""}<button class="card" data-recover-mode="manual"><span><strong>I’ll close them myself</strong><span>Switch once affected clients are closed.</span></span><span class="chev"></span></button></div>
    <div class="actions screen-actions"><button class="btn" data-do="keep-provider">Keep current provider</button><button class="btn primary" data-do="retry-activation">Try again</button></div>`;
}
function activationReview() {
  const p=ui.restart, v=ui.status?.activation, quit=p?.service === "quit";
  if (!p && !ui.switching && !ui.restartError) return "";
  const service=(p?.service || ui.activationIntent?.service) === "claude" ? "Claude" : "Codex";
  const d=service === "Claude" ? ui.status.claude?.destination : ui.status.destination;
  const target=d?.kind === "peer" ? peerName(d.id) : "this device";
  const mode=ui.activationMode;
  return `<section class="activation-review" aria-label="Activation review"><h1>${ui.switching ? "Updating your provider…" : quit ? "Disconnect & quit?" : `${p?.on ? "Turn on Aillies" : "Use Normal"} for ${service}`}</h1>
    <p class="lead">${ui.switching ? "Keep affected clients closed until this finishes." : quit ? "Stop sharing and remove Aillies-owned settings." : `${service} will ${p?.on ? `run through ${esc(target)}` : "use its normal provider"} after the change.`}</p>
    ${ui.restartError ? `<div class="callout action" role="alert">${esc(ui.restartError)}</div>` : ""}
    ${p && !quit && !ui.switching ? activationOptions(service,p.on) : ""}
    ${p && !ui.switching ? `<div class="eyebrow">What happens to open apps</div><div class="group">${p.apps.map(app=>`<div class="row"><span>${esc(app)}</span><span class="sub">${mode === "manual" ? "You close it" : mode === "later" ? "Stays open until you quit" : ["ChatGPT","Claude"].includes(app) ? "Quits, then reopens" : "Closes, stays closed"}</span></div>`).join("")}${p.terminals ? `<div class="row"><span>${esc(p.terminals)} code session${p.terminals === 1 ? "" : "s"}</span><span class="sub">${mode === "now" ? "Closes, stays closed" : "You close them"}</span></div>` : ""}</div><p class="note">${mode === "now" ? "Save your work first. Only desktop apps reopen. Terminal and VS Code stay closed." : "No app is closed for you. The switch waits until all affected clients are closed."}${p.service === "claude" ? " Desktop Chat and Claude Code switch together." : ""}${p.chats != null ? ` About ${esc(p.chats)} saved chats will be reviewed; the count is checked again after closing clients.` : p.service === "codex" ? " Saved chats are counted after clients close." : ""}${p.manual_close ? " Reopen desktop apps yourself after switching." : ""}</p>` : ""}
    ${ui.switching ? activationTimeline(v) : ""}
    ${p ? `<div class="actions screen-actions"><button class="btn" data-do="cancel-restart" ${ui.switching ? "disabled" : ""}>Cancel</button><button class="btn primary" data-do="restart-codex" ${ui.switching ? "disabled" : ""}>${quit ? "Disconnect & quit" : mode === "later" ? "Switch when I quit" : mode === "manual" ? "Wait for me" : "Quit, switch & reopen"}</button></div>${ui.switching ? '<p class="note">The active switch cannot be cancelled. Keep clients closed until it finishes.</p>' : ""}` : ""}</section>`;
}
function waiting(s) {
  const v=s.activation;
  if (!v || v.phase === "cancelled") return home(s);
  if (v.phase === "failed") return activationFailure();
  if (v.phase !== "waiting") return `${brand()}<h1>${v.phase === "completed" ? "Provider switched" : "Applying the switch"}</h1><p class="lead">Keep affected clients closed until this finishes.</p>${activationTimeline(v)}${v.phase === "completed" ? '<button class="btn primary" data-go="home">Done</button>' : ""}`;
  const service=v.service === "claude" ? "Claude" : "Codex";
  const known=v.reviewed_clients?.length ? v.reviewed_clients : v.open_clients ?? [];
  const count=(v.open_clients ?? []).reduce((n,c)=>n+c.count,0),total=known.reduce((n,c)=>n+c.count,0);
  return `${brand()}<div class="eyebrow">Switch scheduled · current provider still active</div><h1>Waiting for ${service} to close</h1><p class="lead">Keep working. When the last affected client closes, Aillies switches and tells you.</p><div class="group">${known.length ? known.map(c=>{const open=v.open_clients?.find(p=>p.label === c.label)?.count ?? 0;return `<div class="row"><span>${esc(c.label)}${open > 1 ? ` · ${esc(open)} sessions` : ""}</span><span class="${open ? "trail" : "account-ready"}">${open ? "Open" : "Closed"}</span></div>`;}).join("") : '<div class="row"><span>Checking closure…</span><span class="spin"></span></div>'}</div><p class="note">${v.checked || total ? `${count} of ${total} affected clients still open.` : "Checking open clients…"} No provider settings have changed for this scheduled switch.</p><div class="actions screen-actions">${s.platform === "macos" ? '<button class="btn primary" data-do="switch-now">Quit, switch & reopen</button>' : ""}<button class="btn" data-do="cancel-scheduled">Cancel the switch</button><button class="btn link" data-go="home">Back to Home</button></div>`;
}
function home(s) {
  const c = s.claude ?? {}, claudeOn = !!(c.installed || c.desktop_installed);
  const incoming = s.peers.filter(p => offered(p.fp, "codex") || offered(p.fp, "claude")).length;
  const outgoing = s.clients.filter(p => s.sharing && p.codex !== false || c.sharing && p.claude).length;
  const service = (id, on) => `<div class="service-card"><div class="service-heading"><span class="service-title">${id === "codex" ? "Codex" : 'Claude <span class="badge experimental">EXPERIMENTAL</span>'}</span><span class="toggle-label"><span>${on ? "Through Aillies" : "Normal"}</span><button class="switch" role="switch" aria-label="Activate Aillies for ${id === "codex" ? "Codex" : "Claude"}" aria-checked="${on}" data-toggle="${id}" ${ui.switching || ui.restart ? "disabled" : ""}></button></span></div>${devicePicker(s,id)}${id === "codex" && on && fastAllowed(s) ? `<div class="fast-row"><span>Use Fast mode<span class="sub">Allowed by ${esc(s.destination.kind === "peer" ? peerName(s.destination.id) : "this device")}</span></span><button class="switch small" role="switch" aria-label="Use Fast mode" aria-checked="${s.fast_mode}" data-toggle="fast"></button></div>` : ""}</div>`;
  return `${brand(s)}
    ${s.activation?.phase === "waiting" ? '<div class="callout info"><strong>Provider switch scheduled</strong><p>Your current provider stays active until affected clients close.</p><div class="actions"><button class="btn" data-go="waiting">View waiting clients</button><button class="btn link" data-do="cancel-scheduled">Cancel switch</button></div></div>' : ""}
    ${s.activation?.phase === "failed" && !ui.restartError ? '<div class="callout action"><strong>The scheduled switch needs attention</strong><button class="btn" data-go="activation-error">Review the switch</button></div>' : ""}
    ${s.pending[0] ? `<div class="callout info"><strong>${esc(s.pending[0].name)} wants to pair</strong><p>Compare the words on both devices before approving.</p><div class="actions"><button class="btn primary" data-go="invite">Review pairing</button></div></div>` : ""}
    ${ui.error ? `<div class="callout action" role="alert">${esc(ui.error)}</div>` : ""}
    <section aria-label="Aillies Use"><div class="section-heading"><h2>Use</h2><span>Run your apps through a device</span></div>
      <div class="group use-group">${service("codex",s.codex_installed)}${service("claude",claudeOn)}</div>
      ${s.last_failure ? `<div class="callout action service-failure" role="alert"><strong>Action required</strong><p>${esc(s.last_failure.message)}</p><div class="actions"><button class="btn" data-do="recheck">Check again</button><button class="btn link" data-do="dismiss">Dismiss</button></div></div>` : ""}
      <p class="note" data-sharing-in>${incoming === 1 ? "1 Ailly is" : `${incoming} Aillies are`} sharing with you.</p></section>
    <section aria-label="Aillies Share"><div class="section-heading"><h2>Share</h2><span>Let approved devices use this ${OS[s.platform] ?? "device"}</span></div>
    <div class="group"><div class="row account-row"><span><strong>Codex</strong><span class="sub ${s.local_account.state === "ready" ? "account-ready" : ""}">${s.local_account.state === "ready" ? "Signed in on this device" : "Sign in to Codex on this device"}</span></span><button class="switch" role="switch" aria-label="Share Codex" aria-checked="${s.sharing}" data-toggle="sharing" ${!s.sharing && s.local_account.state !== "ready" ? "disabled" : ""}></button></div>
    ${s.sharing ? `<div class="row indented"><span>Allow Fast mode</span><button class="switch small" role="switch" aria-label="Allow Fast mode" aria-checked="${!!s.allow_fast_mode}" data-toggle="allow-fast"></button></div>` : ""}
    <div class="row account-row"><span><strong>Claude</strong><span class="sub ${c.connected ? "account-ready" : ""}">${c.connected ? "Connected · saved on this device" : c.reconnect_required ? "Reconnect to use private local storage" : "No account connected"}</span></span><span class="toggle-label">${c.connected || c.sharing ? `<button class="switch" role="switch" aria-label="Share Claude" aria-checked="${!!c.sharing}" data-toggle="claude-sharing" ${ui.claudeSharing || ui.claudeConnecting ? "disabled" : ""}></button>` : `<button class="btn" data-go="claude">${c.reconnect_required ? "Reconnect" : "Connect account"}</button>`}</span></div></div>
    ${ui.claudeError ? `<div class="callout action" role="alert">${esc(ui.claudeError)}</div>` : ""}
    <p class="note" data-sharing-out>You’re sharing with ${outgoing} ${outgoing === 1 ? "Ailly" : "Aillies"}.</p></section>
    <div class="actions paired-actions"><button class="btn primary" data-go="pair-choice">Pair a device</button><button class="btn" data-go="devices">Manage paired devices</button><button class="btn link exit-link" data-go="quit">Disconnect & quit…</button></div>
    <div class="group details-entry"><button class="row" data-go="details"><span>Connection details</span><span class="chev"></span></button></div>
    ${recentWarnings(s)}<div class="actions home-footer"><button class="btn link" data-go="claude">Manage Claude account</button><button class="btn link" data-go="welcome">Setup guide</button></div>`;
}
function recentWarnings(s) {
  const warnings = [...(s.recent_warnings ?? []), ...(s.history_warnings ?? []).map(e=>({...e,at:e.at_unix_seconds,message:e.code.replaceAll("_"," "),history:true,stopped:e.stage === "commit" || e.stage === "recovery"}))].sort((a,b)=>b.at-a.at).slice(0,4);
  if (!warnings.length) return "";
  return `<section id="recent-warnings" class="request-warnings" aria-label="Recent warnings">
    <div class="eyebrow">Recent warnings</div><div class="group">${warnings.map((warning) => `
      <div class="row" data-request-warning><span><span class="warning-title">${warning.history ? warning.stopped ? "History update stopped" : "History kept unchanged" : warning.retry ? `Retry ${esc(warning.retry)} of 3 scheduled` : "Request failed"}</span>
        <span class="sub">${esc(warning.message)}</span></span><span class="sub">${esc(ago(warning.at))}</span></div>`).join("")}</div></section>`;
}

function brand(s) {
  return `<header class="app-header"><div class="brand"><img src="logo.png" alt=""><span>Aillies</span></div>${s ? `<button class="name-button" data-go="details" aria-label="Connection details for ${esc(s.name)}">${esc(s.name)}<span class="chev"></span></button>` : ""}</header>`;
}
function welcome(s) {
  return `${brand()}<section class="welcome-content"><h1>One plan.<br>Every device you own.</h1>
    <p class="lead">Run Codex or Claude on this computer with an account on another computer you trust. The login stays there.</p>
    <div class="eyebrow">What should this ${OS[s.platform] ?? "device"} do?</div>
    <div class="choice"><button class="card selected" data-go="pair-enter"><span><strong>Use another computer’s account</strong><span>Enter a code from the computer that has the login.</span></span><span class="chev"></span></button>
    <button class="card" data-go="invite"><span><strong>Share this ${OS[s.platform] ?? "device"}’s account</strong><span>Show a code for your other computer.</span></span><span class="chev"></span></button></div>
    <p class="note">You can do both later. Using and sharing are separate.</p>
    <ol class="assurances"><li>The login stays on the computer that owns it.</li><li>Both computers compare words and approve each other.</li><li>Disconnect & quit returns your apps to normal, or shows what needs cleanup.</li></ol>
    <div class="actions centered"><button class="btn link" data-go="home">Skip setup</button></div></section>`;
}
function pairChoice(s) {
  return `<button class="back" data-go="home">‹ Home</button>${brand()}<h1>Pair a device</h1><p class="lead">Choose which side of the connection to set up.</p><div class="choice"><button class="card selected" data-go="pair-enter"><span><strong>Use a shared account</strong><span>Enter a code from your other computer.</span></span><span class="chev"></span></button><button class="card" data-go="invite"><span><strong>Share from this ${OS[s.platform] ?? "device"}</strong><span>Show a code for your other computer.</span></span><span class="chev"></span></button></div>`;
}
function pairEnter() {
  return `<button class="back" data-go="pair-choice">‹ Back</button><div class="eyebrow">Use · 1 of 3</div>
    <h1>Enter the code from<br>your other computer</h1><p class="lead">On the computer with the login, open Aillies and choose to share its account.</p>
    <label class="field-label" for="pair-code">Pairing code</label><input class="pairing-digits" id="pair-code" data-code value="${esc(ui.pairCode)}" inputmode="numeric" maxlength="9" placeholder="0000 0000" spellcheck="false" autocomplete="one-time-code" ${ui.waiting ? "disabled" : ""}>
    <details class="pair-options" ${ui.fullPairCode ? "open" : ""}><summary>Not on the same Wi-Fi? Use a full pairing code</summary><p class="note">Paste an aillies1: code to connect across networks with Iroh. Both devices need this version.</p><label class="field-label" for="full-pair-code">Full pairing code</label><textarea id="full-pair-code" data-full-code placeholder="aillies1:…" spellcheck="false" autocomplete="off" ${ui.waiting ? "disabled" : ""}>${esc(ui.fullPairCode)}</textarea></details>
    ${ui.error ? `<div class="callout action" role="alert">${esc(ui.error)}</div>` : ""}
    <div class="actions screen-actions"><button class="btn primary block" data-do="start-pair" ${ui.waiting ? "disabled" : ""}>${ui.waiting ? '<span class="spin"></span>Connecting…' : "Continue"}</button></div><p class="note centered">The code works once and expires after 10 minutes.</p>`;
}
function verify(s) {
  const p = ui.pairing;
  if (!p) return home(s);
  return `<div class="eyebrow">Use · 2 of 3</div>
    <h1>${ui.waiting ? `Waiting for ${esc(p.name)} to approve` : "Check that the words match"}</h1>
    <p class="lead">${esc(p.name)} shows a code too. Continue only if it is identical.</p>
    <div class="code">${esc(p.code)}</div>
    <div class="ids"><div><strong>${esc(s.name)}</strong><span>${esc(s.short_id)}</span></div><div><strong>${esc(p.name)}</strong><span>${esc(p.short_id)}</span></div></div>
    ${ui.error ? `<p class="note" style="color:var(--action)">${esc(ui.error)}</p>` : ""}
    <p class="note">Nothing is paired until both computers approve.</p><div class="actions screen-actions"><button class="btn" data-do="cancel-pair">Cancel pairing</button>${ui.waiting ? `<span class="pending"><span class="spin"></span>Approve on ${esc(p.name)}…</span>` : `<button class="btn primary" data-do="confirm-pair">Words match — continue</button>`}</div>`;
}

function ready(s) {
  const p = ui.paired;
  if (!p) return home(s);
  const h = ui.health[p.fp];
  const providers = h?.status?.providers ?? [];
  const mark = (ok, wait) => (wait ? '<span class="spin"></span>' : ok ? '<span class="check">✓</span>' : '<span style="color:var(--action)">!</span>');
  const waiting = !h || h.checking;
  return `<div class="eyebrow">Use · 3 of 3</div>
    <h1>${esc(p.name)} is paired</h1><p class="lead">Each part is checked separately.</p>
    <div class="group">
      <div class="row"><span>Device identity<span class="sub">Approved on both devices</span></span>${mark(true)}</div>
      <div class="row"><span>Connection<span class="sub">${waiting ? "Checking…" : h.reachable ? `${esc(h.latency_ms)} ms` : "Unreachable"}</span></span>${mark(h?.reachable, waiting)}</div>
      ${["codex", "claude"].map(service => {
        const offered = providers.find(provider => provider.provider === service);
        const available = h?.reachable && h.status?.sharing && offered?.account?.state === "ready";
        return `<div class="row"><span>${service === "codex" ? "Codex" : "Claude"}<span class="sub">${waiting ? "Checking…" : available ? `Shared by ${esc(p.name)}` : "Not shared with this device"}</span></span>${waiting ? mark(false, true) : available ? mark(true) : '<span class="trail">Off</span>'}</div>`;
      }).join("")}
    </div>
    <p class="note">Pairing does not activate routing. Choose this device for Codex or Claude in Aillies Use. Its account login stays on ${esc(p.name)}.</p>
    <div class="actions screen-actions">${["codex","claude"].filter(service=>offered(p.fp,service)).map(service=>`<button class="btn primary" data-use-paired="${service}">Use ${esc(p.name)} for ${service === "codex" ? "Codex" : "Claude"}…</button>`).join("")}<button class="btn" data-go="home">Not now</button></div>`;
}

function invite(s) {
  const p = s.pending[0];
  if (p) return `<button class="back" data-go="home">‹ Home</button><div class="eyebrow">Share · 2 of 2</div>
    <h1>${esc(p.name)} wants to pair</h1><p class="lead">Approve only if the other computer shows exactly these words.</p><div class="code">${esc(p.code)}</div>
    <div class="ids"><div><span>Requesting</span><strong>${esc(p.name)}</strong><span>${esc(p.short_id)}</span></div><div><span>This device</span><strong>${esc(s.name)}</strong><span>${esc(s.short_id)}</span></div></div>
    <div class="group"><div class="row"><span>Codex permission</span><span class="trail">${p.permissions?.codex ? "Allowed" : "Not allowed"}</span></div><div class="row"><span>Claude permission</span><span class="trail">${p.permissions?.claude ? "Allowed" : "Not allowed"}</span></div></div>
    <p class="note">Allowed requests run here and count against your plan. Revoke access any time in Paired devices.</p><div class="actions screen-actions"><button class="btn" data-decide="${esc(p.id)}" data-approve="0">Decline</button><button class="btn primary" data-decide="${esc(p.id)}" data-approve="1">Words match — approve</button></div>`;
  const active=ui.invite && Date.now()-ui.inviteCreated < 600000;
  const permissions=ui.invitePermissions;
  return `<button class="back" data-go="pair-choice">‹ Back</button><div class="eyebrow">Share · 1 of 2</div><h1>Invite your other computer</h1><p class="lead">Choose what it may use first. You can change this later in Paired devices.</p>
    <div class="eyebrow">It may use</div><div class="group">${["codex","claude"].map(service=>{
      const ready=service === "codex" ? s.local_account?.state === "ready" : s.claude?.connected;
      return `<div class="row"><span>${service === "codex" ? "Codex" : "Claude"}<span class="sub">${ready ? "Account linked on this device" : service === "claude" ? "Connect a Claude account first" : "Sign in to Codex first"}</span></span>${!ready && service === "claude" ? '<button class="btn" data-go="claude">Connect</button>' : `<button class="permission-check" role="checkbox" aria-label="Allow ${service === "codex" ? "Codex" : "Claude"} for this invitation" aria-checked="${permissions[service]}" data-invite-permission="${service}" ${!ready && !permissions[service] || ui.creatingInvite ? "disabled" : ""}>${permissions[service] ? "✓" : ""}</button>`}</div>`;
    }).join("")}</div>
    ${ui.claudeError ? `<div class="callout action" role="alert">${esc(ui.claudeError)}</div>` : ""}
    ${active ? `<div class="eyebrow centered">Enter this on the other computer</div><div class="${/^\d{8}$/.test(ui.invite) ? "code pairing-number" : "mono"}" data-invite>${esc(/^\d{8}$/.test(ui.invite) ? ui.invite.slice(0,4)+" "+ui.invite.slice(4) : ui.invite)}</div><p class="note centered" role="timer" data-invite-timer>${countdown(ui.inviteCreated+600000)} left</p><div class="actions centered"><button class="btn" data-do="copy">Copy code</button><button class="btn" data-do="${/^\d{8}$/.test(ui.invite) ? "invite" : "full-invite"}" ${ui.creatingInvite ? "disabled" : ""}>New code</button></div><p class="note centered"><span class="spin"></span>Waiting for the other computer…</p>` : `<div class="actions"><button class="btn primary block" data-do="invite" ${!permissions.codex && !permissions.claude || ui.creatingInvite ? "disabled" : ""}>${ui.creatingInvite ? "Creating code…" : "Show pairing code"}</button></div>${ui.invite ? '<p class="note">That code has expired. Show a new code to continue.</p>' : ""}`}
    ${active && !/^\d{8}$/.test(ui.invite) && s.internet_ready === false ? '<div class="callout action" role="alert">Internet is not connected yet. Keep Aillies open, then generate a new full code. This code can still work over a reachable local or private network.</div>' : ""}
    <p class="note">Works once, on the same local network. <button class="btn link" data-do="full-invite" ${!permissions.codex && !permissions.claude || ui.creatingInvite ? "disabled" : ""}>Show a full code</button> to connect across networks with Iroh.</p><p class="note">Generating a code turns on sharing for the selected accounts. This invitation grants only what you ticked, after both computers approve.</p>`;
}
function countdown(until) {
  const seconds=Math.max(0,Math.ceil((until-Date.now())/1000));
  return `${Math.floor(seconds/60)}:${String(seconds%60).padStart(2,"0")}`;
}

function details(s) {
  return `<button class="back" data-go="home">‹ Home</button><h1>Connection details</h1>
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
    ${recentWarnings(s)}<div class="diagnostic-tabs" aria-label="Request log">${[["incoming","Incoming"],["upstream","Upstream"],["entries","Warnings & errors"],["history","History"]].map(([key,label])=>`<button class="btn ${tab === key ? "primary" : ""}" data-log-tab="${key}" aria-pressed="${tab === key}">${label}</button>`).join("")}</div>
    ${!d ? '<p class="note">Loading diagnostics…</p>' : !entries.length ? '<p class="note">No activity recorded yet.</p>' : `<div class="group diagnostic-log">${entries.map(e=>`<details><summary class="row"><span>${esc(e.request?.provider === "claude" ? "Claude" : "Codex")} · ${esc(e.event?.phase?.replaceAll("_"," ") || e.code?.replaceAll("_"," "))}<span class="sub">${esc(ago(e.at_unix_seconds))}</span></span><span class="chev"></span></summary><dl class="log-fields">${Object.entries({Model:e.request?.model,Stage:e.stage || (tab === "entries" ? e.level : tab),"Chat reference":e.chat_ref,Record:e.record,Attempt:e.request?.attempt,Retry:e.retry,Route:e.request?.route,"Reasoning effort":e.request?.reasoning_effort,"Service tier":e.request?.service_tier,"Input items":e.request?.input_items,Tools:e.request?.tools,"Image tool":e.request?.image_tool,Lite:e.request?.lite,"Trace ID":e.request?.trace_id,Result:e.event?.failure||e.code,Status:e.event?.status||e.upstream_status,"Elapsed (ms)":e.event?.elapsed_ms,"First byte (ms)":e.event?.first_byte_ms}).filter(([,v])=>v!==undefined&&v!==null).map(([k,v])=>`<dt>${esc(k)}</dt><dd>${esc(v)}</dd>`).join("")}</dl></details>`).join("")}</div>`}
    ${d?.persistence_ok === false ? '<p class="note">Some activity could not be saved. This view includes the current in-memory log.</p>' : ""}`;
}

function devices(s) {
  const dev = OS[s.platform] ?? "device";
  return `<button class="back" data-go="home">‹ Home</button>
    <h1>Paired devices</h1><p class="lead">Trust goes one way. Each direction is approved and removed on its own.</p>
    ${ui.deviceRemoval ? `<div class="callout action" role="alert"><strong>${ui.deviceRemoval.kind === "revoke" ? "Revoke" : "Remove"} ${esc(ui.deviceRemoval.name)}?</strong><p>${ui.deviceRemoval.kind === "revoke" ? "This device will lose access to your accounts. It must pair again to regain access." : "This Mac will forget the device. Requests routed to it stop until you pair again."}</p><div class="actions"><button class="btn" data-do="cancel-removal">Cancel</button><button class="btn primary" data-do="confirm-removal">${ui.deviceRemoval.kind === "revoke" ? "Revoke access" : "Remove device"}</button></div></div>` : ""}
    <div class="eyebrow">This device uses</div>
    <div class="group">${s.peers.length ? s.peers.map((p) => `<div class="row"><span>${esc(p.name)}<span class="sub">${esc(p.platform)} · ${esc(p.short_id)}</span></span><button class="btn" data-forget="${esc(p.fp)}">Remove</button></div>`).join("") : '<div class="row"><span class="sub">None yet</span></div>'}</div>
    <div class="eyebrow">May use this ${dev}</div>
    <div class="group">${s.clients.length ? s.clients.map((c) => `<div class="row"><span>${esc(c.name)}<span class="sub">${esc(c.platform)} · ${esc(c.short_id)}${c.last_used ? " · used " + ago(c.last_used) : ""}</span></span><span class="device-grants"><span class="toggle-label">Codex<button class="switch" role="switch" aria-label="Allow ${esc(c.name)} to use Codex" aria-checked="${c.codex !== false}" data-codex-grant="${esc(c.fp)}" data-on="${c.codex === false}"></button></span><span class="toggle-label">Claude<button class="switch" role="switch" aria-label="Allow ${esc(c.name)} to use Claude" aria-checked="${!!c.claude}" data-claude-grant="${esc(c.fp)}" data-on="${!c.claude}"></button></span><button class="btn danger" data-revoke="${esc(c.fp)}">Revoke</button></span></div>`).join("") : '<div class="row"><span class="sub">None</span></div>'}</div>
    <div class="actions"><button class="btn primary" data-go="pair">Pair a device</button></div>`;
}

function claude(s) {
  const c = s.claude ?? {}, canConnect = s.platform === "macos";
  return `<button class="back" data-go="home">‹ Home</button><h1>Claude account <span class="badge experimental">EXPERIMENTAL</span></h1><p class="lead">Connect once to use or share this account from this device.</p>
    <div class="group"><div class="row"><span>Claude<span class="sub">${c.connected ? "Connected · private local storage" : c.reconnect_required ? "Reconnect once to move away from Keychain" : "No account connected"}</span></span><button class="btn" data-do="${c.connected ? "claude-disconnect" : "claude-connect"}" ${ui.claudeConnecting || !!ui.claudeConnect || c.sharing || !canConnect ? "disabled" : ""}>${c.connected ? "Disconnect" : "Connect account"}</button></div></div>
    ${c.sharing ? '<p class="note">Turn off Claude sharing before changing this account.</p>' : ""}
    ${ui.claudeError ? `<div class="callout action" role="alert">${esc(ui.claudeError)}</div>` : ""}
    <div class="oauth-steps"><section class="oauth-step"><span class="step-number">1</span><div><h2>Approve in your browser</h2><p class="note">${ui.claudeConnect && !ui.claudeConnect.manual ? "Opened claude.ai. Sign in with the account you want and choose Authorize." : "Open Claude’s authorization page and approve the account you want."}</p>${ui.claudeConnect ? `<button class="btn" data-do="claude-connect-open" ${ui.claudeConnecting ? "disabled" : ""}>Open the browser again</button>` : ""}${ui.claudeConnect?.manual ? `<label class="field-label">Authorization link<input aria-label="Claude authorization link" readonly value="${esc(ui.claudeConnect.url)}"></label>` : ""}</div></section>
      <section class="oauth-step"><span class="step-number">2</span><div><h2>Paste the code Claude shows</h2><p class="note">After authorizing, copy the complete one-time code, including its state.</p>${ui.claudeConnect ? `<input type="password" data-claude-oauth-code aria-label="Claude authorization code" autocomplete="off" spellcheck="false" maxlength="8192" placeholder="Paste authorization code" ${ui.claudeConnecting ? "disabled" : ""}><div class="actions"><button class="btn" data-do="claude-connect-cancel">Cancel</button><button class="btn primary" data-do="claude-connect-finish" ${ui.claudeConnecting ? "disabled" : ""}>${ui.claudeConnecting ? "Connecting…" : "Connect"}</button></div>` : '<p class="note">Start Connect account above to get your code.</p>'}</div></section></div>
    <p class="note">Aillies saves this connection in its private data folder, without Keychain prompts. Your normal Claude login and chats stay unchanged.</p>
    ${!canConnect ? '<p class="note">Connect accounts on a Mac, or use an account shared by a paired Mac.</p>' : ""}`;
}

function quit(s) {
  const cleanup = ui.cleanup;
  return `<button class="back" data-go="home">‹ Home</button><h1>${cleanup && !cleanup.complete ? "Cleanup incomplete" : "Disconnect & quit"}</h1><p class="lead">Stop sharing and return your clients to their normal providers. Keep your other settings and chats.</p>
    ${ui.exitError ? `<div class="callout action" role="alert">${esc(ui.exitError)}</div>` : ""}
    ${cleanup ? `<div class="group">${[["codex","Codex"],["claude_code","Claude Code"],["claude_desktop","Claude desktop"]].map(([key,label])=>`<div class="row"><span>${label}</span><span class="${cleanup[key] ? "account-ready" : "warning-title"}">${cleanup[key] ? "Disconnected" : "Cleanup incomplete"}</span></div>`).join("")}</div><p class="note">Recovery files were kept. Affected clients stay closed when cleanup is incomplete. Quit anyway may leave a client pointed at Aillies until you reopen Aillies and finish cleanup.</p>` : ""}
    ${activationReview()}<div class="actions"><button class="btn" data-go="home">Cancel</button>${!ui.restart && !ui.switching ? `<button class="btn primary" data-do="disconnect-quit">${cleanup || ui.exitError ? "Try cleanup again" : "Disconnect & quit"}</button>` : ""}${(cleanup && !cleanup.complete || ui.exitError) && !ui.switching ? '<button class="btn" data-do="quit-anyway">Quit anyway</button>' : ""}</div>`;
}
function historyNotice(report) {
  const n=Number(report.history_skipped)||0;
  return `Provider switched. ${n ? n+" older histories were" : "Older history was"} kept unchanged. Use a new chat for Aillies. See Diagnostics → History.`;
}
async function switchProvider(on) { return reviewActivation("codex", on); }
async function reviewActivation(service, on, mode) {
  if (ui.switching || ui.restart) return;
  ui.activationIntent={service,on}; ui.activationMode=mode ?? (ui.status.platform === "macos" ? "now" : "manual");
  ui.picker = null; ui.switching = true; ui.restartError = null; ui.exitError = null; render();
  try { ui.restart = await invoke("prepare_activation", {service, on, mode:ui.activationMode}); }
  catch (e) { if(service === "quit") ui.exitError = String(e); else ui.restartError = String(e); }
  finally { ui.switching = false; render(); }
}
async function restartCodex() {
  if (ui.switching || !ui.restart) return;
  const service = ui.restart.service;
  ui.switching = true; ui.restartError = null; render();
  try {
    if (service !== "quit" && ui.activationMode !== "now") {
      await invoke("schedule_activation",{token:ui.restart.token,mode:ui.activationMode});
      ui.screen="waiting"; return;
    }
    const report = await invoke("apply_activation", {token:ui.restart.token});
    if (service === "quit") {
      ui.cleanup = report;
      if (report.complete) { await invoke("quit_if_normal"); return; }
    } else if (report.history_pending) toast(historyNotice(report));
    else if (report.reopen_failed) toast("Provider changed. Open the desktop app to continue.");
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
  const views = { home, welcome, waiting, "activation-error":activationFailure, "pair-choice": pairChoice, "pair-enter": pairEnter, verify, ready, invite, details, diagnostics, devices, claude, quit };
  // Re-rendering every poll must not wipe what the person is typing.
  const active = document.activeElement;
  if (fromPoll && ((ui.screen === "claude" && ui.claudeConnect) || (active && (active.matches("textarea, input, select") || ui.picker && active.closest(".device-picker"))))) return;
  $app.dataset.screen = ui.screen;
  if (ui.screen === "home" && (ui.restart || ui.switching || ui.restartError)) {
    $app.innerHTML = ui.restartError ? activationFailure() : `${brand()}${activationReview()}`;
    return;
  }
  $app.innerHTML = (views[ui.screen] ?? home)(s);
}

// ---------- Events ----------
document.addEventListener("keydown", ev => {
  if (!ui.picker) return;
  if (ev.key === "Escape") {
    const service = ui.picker; ui.picker = null; render();
    document.querySelector(`[data-picker="${service}"]`)?.focus();
  } else if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
    const choices = [...document.querySelectorAll('.device-menu [role="menuitem"]:not(:disabled)')];
    if (!choices.length) return;
    ev.preventDefault();
    const current = choices.indexOf(document.activeElement);
    const next = current < 0 ? ev.key === "ArrowDown" ? 0 : choices.length - 1 : (current + (ev.key === "ArrowDown" ? 1 : -1) + choices.length) % choices.length;
    choices[next].focus();
  }
});
document.addEventListener("input", ev => {
  if (ev.target.matches("[data-code]")) ui.pairCode = ev.target.value;
  if (ev.target.matches("[data-full-code]")) ui.fullPairCode = ev.target.value;
});

document.addEventListener("click", async (ev) => {
  const t = ev.target.closest("button");
  if (ui.picker && !ev.target.closest(".device-picker")) { ui.picker = null; render(); }
  if (!t || t.disabled) return;
  if (t.dataset.go) return go(t.dataset.go);
  if (t.dataset.picker) return discoverDevices(t.dataset.picker);
  if (t.dataset.selectService) {
    if (ui.switching || ui.restart) return;
    const service=t.dataset.selectService, target=t.dataset.target;
    ui.picker=null;
    return act(() => invoke(service === "codex" ? "set_destination" : "set_claude_destination", {target}));
  }
  if (t.dataset.activationMode || t.dataset.recoverMode) {
    if(ui.switching) return;
    const mode=t.dataset.activationMode || t.dataset.recoverMode;
    const intent=ui.activationIntent || ui.status.activation;
    if(!intent) return;
    await invoke("cancel_scheduled_activation"); ui.restart=null; ui.restartError=null;
    go("home"); return reviewActivation(intent.service,intent.on,mode);
  }
  if (t.dataset.invitePermission) {
    const service=t.dataset.invitePermission;
    if(!["codex","claude"].includes(service) || ui.creatingInvite) return;
    await invoke("cancel_invitation"); ui.invite=null;
    ui.invitePermissions[service]=!ui.invitePermissions[service]; return render();
  }
  if(t.dataset.usePaired) {
    const service=t.dataset.usePaired;
    if(!ui.paired || !offered(ui.paired.fp,service)) return;
    await invoke(service === "codex" ? "set_destination" : "set_claude_destination",{target:ui.paired.fp});
    await refresh(); go("home"); return reviewActivation(service,true);
  }
  if (t.dataset.logTab) { ui.diagnosticTab=t.dataset.logTab; return render(); }
  if (t.dataset.toggle) {
    const s = ui.status;
    if (t.dataset.toggle === "fast") return act(() => invoke("set_fast_mode", { on: !s.fast_mode }));
    if (t.dataset.toggle === "allow-fast") return act(() => invoke("set_allow_fast_mode", { on: !s.allow_fast_mode }));
    if (t.dataset.toggle === "sharing") {
      await act(() => invoke("set_sharing", { on: !s.sharing }), s.sharing ? "Sharing off" : "Sharing on");
      if (!ui.status.sharing && !ui.status.claude?.sharing) { ui.invite = null; render(); }
      return;
    }
    if (t.dataset.toggle === "claude-sharing") {
      if (ui.claudeSharing) return;
      ui.claudeSharing = true; ui.claudeError = null; render();
      try { await invoke("set_claude_sharing", { on: !s.claude.sharing, source: "connected" }); await refresh(); if (!ui.status.sharing && !ui.status.claude?.sharing) ui.invite = null; }
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
    ui.invite = null;
    if (ui.screen === "invite") go("home");
    return;
  }
  if (t.dataset.codexGrant) return act(() => invoke("grant_codex", { fp: t.dataset.codexGrant, on: t.dataset.on === "true" }));
  if (t.dataset.claudeGrant) return act(() => invoke("grant_claude", { fp: t.dataset.claudeGrant, on: t.dataset.on === "true" }));
  if (t.dataset.revoke || t.dataset.forget) {
    const fp = t.dataset.revoke || t.dataset.forget;
    const kind = t.dataset.revoke ? "revoke" : "remove";
    const device = (kind === "revoke" ? ui.status.clients : ui.status.peers).find(p => p.fp === fp);
    if (!device) return;
    ui.deviceRemoval = { fp, kind, name: device.name }; return render();
  }
  switch (t.dataset.do) {
    case "open-automation": return act(()=>invoke("open_automation_settings"));
    case "retry-activation": {
      const i=ui.activationIntent || ui.status.activation;if(!i)return;
      ui.restart=null;ui.restartError=null;go("home");return reviewActivation(i.service,i.on);
    }
    case "keep-provider":
      await invoke("cancel_scheduled_activation"); await invoke("dismiss_activation_notice");
      ui.restart=null;ui.restartError=null;return go("home");
    case "cancel-scheduled":
      try {await invoke("cancel_scheduled_activation"); await refresh(); go("home");}
      catch(e){toast(String(e));}return;
    case "switch-now": {
      const i=ui.status.activation;if(!i)return;
      await invoke("cancel_scheduled_activation");ui.restart=null;go("home");return reviewActivation(i.service,i.on,"now");
    }
    case "cancel-removal": ui.deviceRemoval = null; return render();
    case "confirm-removal": {
      const device = ui.deviceRemoval;
      if (!device) return;
      ui.deviceRemoval = null;
      return act(() => invoke(device.kind === "revoke" ? "revoke_client" : "remove_peer", {fp:device.fp}), device.kind === "revoke" ? "Revoked" : "Removed");
    }
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
    case "invite":
    case "full-invite":
      if (ui.creatingInvite) return;
      ui.creatingInvite = true;
      try {
        ui.invite = await invoke(t.dataset.do === "full-invite" ? "create_full_invite" : "create_invite",{permissions:{...ui.invitePermissions}});
        ui.inviteCreated = Date.now();
        await refresh();
        go("invite");
      } catch (e) {
        toast(String(e)); await refresh();
      } finally { ui.creatingInvite = false; render(); }
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
      ui.pairCode = document.querySelector("[data-code]").value;
      ui.fullPairCode = document.querySelector("[data-full-code]")?.value ?? "";
      const code = ui.fullPairCode.trim().startsWith("aillies1:") ? ui.fullPairCode.trim() : ui.pairCode.trim();
      if (!code) { ui.error = "Enter a pairing code to continue."; render(); return; }
      ui.waiting = true;
      ui.error = null;
      render();
      try {
        ui.pairing = await invoke("start_pairing", { code });
        ui.pairCode = ui.fullPairCode = "";
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
      if (!ui.pairing) return;
      ui.pairingEpoch++;
      const cancelled = ui.pairing.id;
      ui.pairing = null; ui.waiting = false;
      await invoke("cancel_pairing", { id: cancelled }).catch(() => {});
      return go("home");
    case "confirm-pair": {
      if (ui.waiting || !ui.pairing) return;
      const epoch = ++ui.pairingEpoch;
      const pairingId = ui.pairing.id;
      ui.waiting = true;
      ui.error = null;
      render();
      try {
        const paired = await invoke("confirm_pairing", { id: pairingId });
        if (epoch !== ui.pairingEpoch) return;
        ui.paired = paired;
        ui.waiting = false;
        ui.pairing = null;
        ui.screen = "ready";
        checkHealth(ui.paired.fp);
        await refresh();
      } catch (e) {
        if (epoch !== ui.pairingEpoch) return;
        ui.waiting = false;
        ui.error = String(e);
        render();
      }
      return;
    }
    case "rename": {
      const name = document.querySelector("[data-name]").value.trim();
      if (name) await act(() => invoke("set_name", { name }), "Renamed");
      return;
    }
  }
});


(async () => {
  try { ui.setupDone = localStorage.getItem("aillies.setup.v1") === "done"; } catch (_) { /* Optional preference. */ }
  ui.startupError = await invoke("startup_error").catch(() => null);
  await refresh();
  const s = ui.status;
  if (s && ui.screen === "home" && !ui.setupDone && !s.peers.length && !s.clients.length && !s.pending.length && !s.codex_installed && !s.claude?.installed && !s.claude?.desktop_installed && !s.sharing && !s.claude?.sharing) go("welcome");
  if(ui.status?.peers.length) { await discoverDevices("codex"); ui.picker=null; render(); }
  window.__TAURI__.event?.listen("aillies:navigate", () => refresh()).catch(() => {});
  setInterval(() => refresh(true), 2000);
  setInterval(()=>{
    if(ui.screen !== "invite" || !ui.invite || ui.creatingInvite)return;
    if(Date.now()-ui.inviteCreated >= 600000) { if(document.querySelector("[data-invite-timer]"))render(true); }
    else { const timer=document.querySelector("[data-invite-timer]"); if(timer)timer.textContent=`${countdown(ui.inviteCreated+600000)} left`; }
  },1000);
  setInterval(() => {
    const d = ui.status?.destination;
    if (d?.kind === "peer" && ui.screen === "home") checkHealth(d.id);
  }, 15000);
})();
