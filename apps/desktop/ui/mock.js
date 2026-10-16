// Browser preview only: fakes the Tauri bridge when the page is opened
// outside the app (design reviews, screenshots). Ignored inside Aillies.
"use strict";
if (!window.__TAURI__) {
  const fp = "2b8e05d4" + "0".repeat(56);
  const q = new URLSearchParams(location.search);
  if (["light", "dark"].includes(q.get("theme"))) document.documentElement.style.colorScheme = q.get("theme");
  const state = {
    internet_ready: true, name: "Matthew’s Mac", platform: "macos", short_id: "7F3A·91C2", fingerprint: "7f3a91c2" + "1".repeat(56), version: "1.2.0", diagnostics_build: "router-core-v24-adaptive-updates",
    state: "ready", routing: true, sharing: true, fast_mode: false, allow_fast_mode: false, destination: { kind: "peer", id: fp }, destination_name: "Dailyngocss",
    peers: [{ fp, short_id: "2B8E·05D4", name: "Dailyngocss", platform: "macos", addrs: ["192.168.1.20:18544", "100.101.7.3:18544", "dailyngocss.local:18544"] }],
    clients: [{ fp: "9" .repeat(64), short_id: "9999·9999", name: "Office PC", platform: "windows", approved_at: 0, last_used: Date.now() / 1000 - 7200 }],
    claude: { connected:true, reconnect_required:false, desktop_installed:false, account_source:"connected", installed:false, sharing:false, account:{state:"ready"}, destination:{kind:"local"}, destination_name:"This device" },
    recent_warnings:[{at:Date.now()/1000-120,code:"peer_unavailable",message:"The selected device is unavailable. Check its connection and sharing settings.",retry:null},{at:Date.now()/1000-480,code:"upstream_service_unavailable",message:"The upstream service is temporarily unavailable.",retry:1}],
    pending: [], codex_installed: true, local_account: { state: "ready" }, last_failure: null, loopback_port: 18522, peer_port: 18544,
  };
  if (q.get("state") === "action") { state.state = "action"; state.last_failure = { code: "oauth_expired", message: "The sharing device's Codex login expired. Open Codex there to renew it, then retry.", at: 0 }; }
  if (q.get("state") === "pending") state.pending = [{ id: "p1", name: "Lan’s MacBook", platform: "macos", code: "BLUE · 47 · MOON", short_id: "5C1D·88A0",permissions:{codex:true,claude:false} }];
  if (q.get("state") === "new") { state.claude.connected=false; state.peers = []; state.clients = []; state.sharing = false; state.destination = { kind: "local" }; state.state = "local"; state.codex_installed = false; }
  if (q.get("state") === "access") {
    if (["macos","windows","linux"].includes(q.get("platform"))) state.platform=q.get("platform");
    state.activation={id:"preview-access",service:"codex",on:true,phase:"failed",failed_stage:"settings",error:"Settings access is blocked. Grant access, then review the switch again.",permission_issues:[{target:"codex_config",operation:"write",code:"permission_denied",can_repair:true}]};
  }
  const health = { reachable: true, latency_ms: 4, addr: "192.168.1.20:18544", status: { sharing: true, providers: [{ provider: "codex", allow_fast_mode: true, account: { state: "ready" } },{provider:"claude",account:{state:"ready"}}] } };
  let activation;
  let previewId=0;
  let update={phase:q.get("state")==="update"?"ready":"idle",version:q.get("state")==="update"?"1.2.0":null};
  const handlers = {
    update_status:()=>update,
    check_update:()=>{update={phase:"ready",version:"1.2.0"};},
    install_update:()=>{update.phase="waiting";update.message="Waiting for active work…";},
    cancel_update:()=>{update.phase="ready";update.message="Update postponed. Your current version is still running.";},
    open_release_page:()=>{},
    repair_settings_access: () => {state.activation.permission_issues=[];state.activation.error="Write access is ready. Review the switch and try again.";},
    prepare_activation: ({service,on,mode="now"}) => { activation={service,on,mode,token:`preview-${++previewId}`}; return {token:activation.token,service,on,mode,chats:12,apps:service === "claude" ? ["Claude"] : ["ChatGPT"],terminals:1,manual_close:mode==="manual"||state.platform!=="macos"}; },
    schedule_activation: ({mode}) => {
      const pending={id:activation.token,service:activation.service,on:activation.on,mode,phase:"waiting",open_clients:[{label:activation.service === "claude" ? "Claude" : "Codex",count:2}]};
      state.activation=pending; activation=null;
      setTimeout(()=>{
        if(state.activation?.id!==pending.id || state.activation.phase!=="waiting")return;
        if(pending.service === "codex")state.codex_installed=pending.on;
        else state.claude.installed=state.claude.desktop_installed=pending.on;
        Object.assign(state.activation,{phase:"completed",open_clients:[],closed:true,recovery_saved:true,history_done:true,settings_done:true,reopen_failed:0});
      },8000);
    },
    cancel_scheduled_activation: () => {activation=null;state.activation=null;},
    dismiss_activation_notice: () => {state.activation=null;},
    open_automation_settings: () => {},
    cancel_invitation: () => {},
    cancel_activation: () => { activation=null; },
    apply_activation: () => {
      if(activation.service === "codex") state.codex_installed=activation.on;
      if(activation.service === "claude") state.claude.installed=state.claude.desktop_installed=activation.on;
      if(activation.service === "quit") {
        state.sharing=state.claude.sharing=false;
        if(q.get("state") === "cleanup") return {complete:false,codex:true,claude_code:false,claude_desktop:true,reopen_failed:0};
        state.codex_installed=state.claude.installed=state.claude.desktop_installed=false;
      }
      return {complete:true,codex:true,claude_code:true,claude_desktop:true,reopen_failed:0};
    },
    quit_anyway: () => { document.getElementById("app").innerHTML="<h1>Aillies has quit</h1>"; },
    diagnostics: () => ({persistence_ok:true,incoming:[{at_unix_seconds:Date.now()/1000-120,request:{provider:"claude",model:"claude-sonnet-4-6",route:"peer",attempt:1},event:{phase:"completed",status:200,elapsed_ms:2100}}],upstream:[],entries:[]}),
    set_name: ({name}) => {state.name=name;},
    begin_claude_connect: () => "https://claude.ai/oauth/authorize?preview=true",
    open_claude_authorization: () => {},
    cancel_claude_connect: () => {},
    finish_claude_connect: () => { state.claude.connected = true; state.claude.account_source = "connected"; },
    disconnect_claude_account: () => { state.claude.connected = false; },
    switch_claude_desktop_provider: ({ on }) => { state.claude.desktop_installed = on; },
    switch_claude_provider: ({ on }) => { state.claude.installed = on; },
    set_claude_sharing: ({ on, source }) => { state.claude.sharing = on; state.claude.account_source = source; },
    set_claude_destination: ({ target }) => { state.claude.destination = target === "local" ? {kind:"local"} : {kind:"peer",id:target}; state.claude.destination_name = target === "local" ? "This device" : state.peers.find(p => p.fp === target)?.name || "Unknown device"; },
    grant_codex: ({ fp, on }) => { const client = state.clients.find(c => c.fp === fp); if(client) client.codex = on; },
    grant_claude: ({ fp, on }) => { const client = state.clients.find(c => c.fp === fp); if(client) client.claude = on; },
    prepare_codex_restart: ({ on }) => ({ token: "preview", on, apps: ["ChatGPT"], terminals: 2 }),
    restart_codex_provider: () => { state.codex_installed = !state.codex_installed; return { chats: 12, reopen_failed: 0 }; },
    switch_codex_provider: ({ on }) => { if (q.get("state") === "restart") throw "Quit Codex, then switch providers here. Your chats will be preserved."; state.codex_installed = on; return { chats: 12, elapsed_ms: 28, reopen_required: true }; },
    take_navigation: () => null, quit_if_normal: () => { if (state.codex_installed || state.claude.installed || state.claude.desktop_installed) throw "Switch to Normal before quitting."; },
    export_diagnostics: () => "Downloads/Aillies-diagnostics-preview.json",
    startup_error: () => null, status: () => structuredClone(state), peer_health: () => health,
    set_routing: ({ on }) => { state.routing = on; state.state = on ? "ready" : "paused"; },
    set_fast_mode: ({ on }) => { state.fast_mode = on; },
    set_allow_fast_mode: ({ on }) => { state.allow_fast_mode = on; },
    set_sharing: ({ on }) => { state.sharing = on; }, set_destination: ({target}) => { state.destination=target === "local" ? {kind:"local"} : {kind:"peer",id:target}; }, dismiss_failure: () => { state.last_failure = null; state.state = "ready"; },
    create_invite: ({permissions}) => {state.sharing ||= !!permissions?.codex;state.claude.sharing ||= !!permissions?.claude;return "00123456";},
    cancel_pairing: () => {},
    decide_pairing: () => {state.pending=[];},
    remove_peer: ({fp}) => {state.peers=state.peers.filter(p=>p.fp!==fp);},
    revoke_client: ({fp}) => {state.clients=state.clients.filter(p=>p.fp!==fp);},
    create_full_invite: () => "aillies1:eyJ2IjoxLCJuYW1lIjoiTWF0dGhld-KAmXMgTWFjIiwicGxhdGZvcm0iOiJtYWNvcyIsImFkZHJzIjpbIjE5Mi4xNjguMS4yMDoxODU0NCJdfQ",
    start_pairing: () => ({ id: "x", code: "BLUE · 47 · MOON", name: "Dailyngocss", platform: "macos", short_id: "2B8E·05D4" }),
    confirm_pairing: () => ({ fp, short_id: "2B8E·05D4", name: "Dailyngocss", platform: "macos", addrs: [] }),
  };
  window.__TAURI__ = { core: { invoke: async (cmd, args) => (handlers[cmd] ?? (() => null))(args ?? {}) } };
  if (q.get("screen")) setTimeout(() => { document.dispatchEvent(new CustomEvent("x")); }, 0);
}
