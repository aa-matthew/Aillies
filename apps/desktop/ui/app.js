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
  paired: null,        // PeerView after success
  error: null,
  startupError: null,
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
  ui.screen = screen;
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
  const lead = !s.routing ? "Codex requests are not sent anywhere." : dest === "local" ? "Codex uses this device’s own login." : `New Codex chats use ${esc(s.destination_name)}’s account.`;
  const fail = s.last_failure;
  const pending = s.pending[0];
  return `
    ${brand()}
    ${pending ? `<div class="callout info"><strong>${esc(pending.name)} wants to use this ${dev}’s account</strong>
       <p>Approve only if both devices show <b>${esc(pending.code)}</b>.</p>
       <div class="actions"><button class="btn" data-decide="${esc(pending.id)}" data-approve="0">Decline</button><button class="btn primary" data-decide="${esc(pending.id)}" data-approve="1">Codes match — approve</button></div></div>` : ""}
    ${fail ? `<div class="callout action" role="alert"><strong>${esc(fail.message)}</strong>
       <div class="actions"><button class="btn" data-do="dismiss">Dismiss</button>${dest !== "local" ? `<button class="btn primary" data-do="recheck">Check again</button>` : ""}</div></div>` : ""}
    <h1 class="${STATE_CLASS[s.state] ?? ""}"><span class="dot"></span>${LABEL[s.state]}</h1>
    <p class="lead">${lead}</p>
    <div class="group">
      <div class="row"><span>Send through<span class="sub">${destSub}</span></span>
        <select data-dest aria-label="Send through">
          <option value="local" ${dest === "local" ? "selected" : ""}>This device</option>
          ${s.peers.map((p) => `<option value="${esc(p.fp)}" ${p.fp === dest ? "selected" : ""}>${esc(p.name)}</option>`).join("")}
        </select></div>
      <div class="row"><span>Routing${s.routing ? "" : '<span class="sub">Paused — nothing is sent</span>'}</span>
        <button class="switch" role="switch" aria-label="Routing" aria-checked="${s.routing}" data-toggle="routing"></button></div>
      <div class="row"><span>Share this ${dev}<span class="sub">${s.sharing ? `${s.local_account.state === "ready" ? "Codex account ready" : "Codex login not ready"} · <button class="btn link" data-go="devices">${s.clients.length} approved</button>` : "Off"}</span></span>
        <button class="switch" role="switch" aria-label="Share this device" aria-checked="${s.sharing}" data-toggle="sharing"></button></div>
    </div>
    ${!s.codex_installed ? `<div class="group" style="margin-top:14px"><button class="row" data-go="codex"><span>Use Aillies with Codex<span class="sub">Not set up yet</span></span><span class="chev"></span></button></div>` : ""}
    <div class="actions"><button class="btn primary" data-go="pair">Pair a device</button></div>
    <div class="group" style="margin-top:16px"><button class="row" data-go="details"><span>Connection details</span><span class="chev"></span></button></div>`;
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

function pair() {
  return `<button class="back" data-go="home">‹ Home</button>
    <h1>Pair a device</h1><p class="lead">Do this once per device.</p>
    <div class="choice">
      <button class="card" data-go="pair-enter"><strong>Use another device’s account</strong><span>Enter the code shown on that device.</span></button>
      <button class="card" data-do="invite"><strong>Let another device use this one</strong><span>Show a one-time code to enter there.</span></button>
    </div>
    <p class="note">Works on the same network or a private network such as Tailscale.</p>`;
}

function pairEnter() {
  return `<button class="back" data-go="pair">‹ Back</button>
    <div class="steps"><span class="on"></span><span class="on"></span><span></span><span></span></div>
    <h1>Enter the pairing code</h1>
    <p class="lead">On the other device choose <b>Pair a device → Let another device use this one</b>, then paste its code here.</p>
    <textarea data-code placeholder="aillies1:…" spellcheck="false" autofocus></textarea>
    ${ui.error ? `<p class="note" style="color:var(--action)">${esc(ui.error)}</p>` : ""}
    <div class="actions"><button class="btn primary" data-do="start-pair" ${ui.waiting ? "disabled" : ""}>${ui.waiting ? '<span class="spin"></span>Connecting…' : "Continue"}</button></div>`;
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
    <p class="note">New Codex chats on this device use ${esc(p.name)}’s account. The login stays on ${esc(p.name)}.</p>
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
    <div class="mono" data-invite>${esc(ui.invite ?? "")}</div>
    <div class="actions"><button class="btn" data-do="copy">Copy code</button></div>
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
      <button class="row" data-go="codex"><span>Use Aillies with Codex</span><span class="trail">${s.codex_installed ? "On" : "Off"}<span class="chev"></span></span></button>
      <button class="row" data-go="devices"><span>Devices</span><span class="trail">${s.peers.length + s.clients.length}<span class="chev"></span></span></button>
      <div class="row"><span>Name<span class="sub">ID ${esc(s.short_id)}</span></span><span class="trail"><input data-name value="${esc(s.name)}" maxlength="64" aria-label="Device name" style="width:150px"><button class="btn" data-do="rename">Save</button></span></div>
      <div class="row"><span>Version<span class="sub">Aillies ${esc(s.version)}</span></span><span></span></div>
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
  return `<button class="back" data-go="details">‹ Connection details</button>
    <h1>Codex integration</h1><p class="lead">Codex sends new chats to the device chosen in Send through.</p>
    <div class="group"><div class="row"><span>Use Aillies with Codex<span class="sub">${s.codex_installed ? "Your previous settings are backed up" : "Off · Codex uses its own settings"}</span></span>
      <button class="switch" role="switch" aria-label="Use Aillies with Codex" aria-checked="${s.codex_installed}" data-toggle="codex"></button></div></div>
    <p class="note">After turning this on or off, quit and reopen Codex. Existing chats stay on the account they started with. If you edited Codex settings meanwhile, Aillies keeps your backup and asks you to merge instead of overwriting.</p>`;
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
  const views = { home, welcome, pair, "pair-enter": pairEnter, verify, ready, invite, details, devices, codex };
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
    if (t.dataset.toggle === "routing") return act(() => invoke("set_routing", { on: !s.routing }));
    if (t.dataset.toggle === "sharing") return act(() => invoke("set_sharing", { on: !s.sharing }), s.sharing ? "Sharing off" : "Sharing on");
    if (t.dataset.toggle === "codex") return act(() => invoke(s.codex_installed ? "codex_restore" : "codex_enable", {}), "Quit and reopen Codex to apply");
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
    case "dismiss":
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
      try {
        ui.invite = await invoke("create_invite");
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
      const code = document.querySelector("[data-code]").value.trim();
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
  if (ev.target.matches("[data-dest]")) {
    const target = ev.target.value;
    act(() => invoke("set_destination", { target }), "New Codex chats use the selected device. Start a new chat.");
    if (target !== "local") checkHealth(target);
  }
});

(async () => {
  ui.startupError = await invoke("startup_error").catch(() => null);
  await refresh();
  setInterval(() => refresh(true), 2000);
  setInterval(() => {
    const d = ui.status?.destination;
    if (d?.kind === "peer" && ui.screen === "home") checkHealth(d.id);
  }, 15000);
})();
