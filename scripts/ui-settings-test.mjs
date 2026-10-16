// Behavior checks with synthetic state only; never touches native apps/accounts/configs.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const listeners=new Map(), elements={app:{innerHTML:'',dataset:{}},toast:{hidden:true,textContent:''}};
const state={internet_ready:false,name:'Test Mac',platform:'macos',version:'1.2.0',state:'local',routing:true,sharing:false,allow_fast_mode:false,destination:{kind:'local'},peers:[{fp:'both',name:'Both <Mac>',platform:'macos'},{fp:'codex-only',name:'Codex Mac',platform:'macos'}],clients:[{fp:'friend',name:'Friend <Mac>',codex:true,claude:true}],pending:[],codex_installed:false,local_account:{state:'ready'},claude:{connected:true,reconnect_required:false,sharing:false,installed:false,desktop_installed:false,destination:{kind:'local'}}};
let healthProviders=null, holdHealth=false, finishHealth;
let update={phase:"idle"};
let nameValue='New <Mac>', codeValue='', pairValue='0012 3456', plan=null, resolveApply, rejectApply, deferred=false, cleanupFails=false, reviewFails=false, sharingFails=false, healthCalls=0, applies=0, quits=0, connects=0, pairCalls=0, confirmCalls=0;
let delayedPair=false, resolvePair, cancels=0;
let invites=0;
const commands=[];
const context=vm.createContext({document:{getElementById:id=>elements[id],querySelector:selector=>({value:selector==='[data-name]'?nameValue:selector==='[data-claude-oauth-code]'?codeValue:pairValue}),activeElement:null,addEventListener:(type,fn)=>listeners.set(type,fn)},window:{__TAURI__:{core:{invoke:async(cmd,args={})=>{
 commands.push(cmd);
 if(cmd==='update_status')return structuredClone(update);
 if(cmd==='check_update'){update={phase:'ready',version:'1.2.0 <bad>'};return;}
 if(cmd==='install_update'){update.phase='waiting';return;}
 if(cmd==='cancel_update'){update.phase='ready';return;}
 if(cmd==='open_release_page')return;
 if(cmd==='status') return structuredClone(state);
 if(cmd==='startup_error'||cmd==='take_navigation'||cmd==='open_claude_authorization'||cmd==='cancel_claude_connect') return null;
 if(cmd==='peer_health') {healthCalls++;if(holdHealth)return new Promise(resolve=>{finishHealth=resolve;});return {reachable:true,status:{name:args.fp==='both'?'Renamed <Mac>':'Codex Mac',sharing:true,providers:healthProviders??[{provider:'codex',account:{state:'ready'}},...(args.fp==='both'?[{provider:'claude',account:{state:'ready'}}]:[])]}};}
 if(cmd==='prepare_activation') {if(reviewFails) throw 'Close the unrecognized session yourself.'; plan={token:'one-use',service:args.service,on:args.on,mode:args.mode??'now',chats:12,apps:args.service==='claude'?['Claude']:['ChatGPT','Visual Studio Code'],terminals:2,manual_close:args.mode==='manual'||state.platform!=='macos'};return structuredClone(plan);}
 if(cmd==='schedule_activation'){assert.equal(args.token,plan.token);state.activation={id:'scheduled',service:plan.service,on:plan.on,mode:args.mode,phase:'waiting',open_clients:[{label:'Codex',count:2}]};plan=null;return;}
 if(cmd==='cancel_scheduled_activation'){plan=null;state.activation=null;return;}
 if(cmd==='dismiss_activation_notice'){state.activation=null;return;}
 if(cmd==='repair_settings_access'){assert.equal(args.id,'permission-failure');state.activation.permission_issues=[];state.activation.error='Write access is ready. Review the switch and try again.';return;}
 if(cmd==='open_automation_settings'||cmd==='cancel_invitation')return;
 if(cmd==='cancel_activation') {plan=null;return;}
 if(cmd==='apply_activation') {
  applies++;assert.equal(args.token,plan.token);const current=plan;plan=null;
  const apply=()=>{if(current.service==='codex')state.codex_installed=current.on;if(current.service==='claude')state.claude.installed=state.claude.desktop_installed=current.on;
   if(current.service==='quit'){state.sharing=state.claude.sharing=false;if(!cleanupFails)state.codex_installed=state.claude.installed=state.claude.desktop_installed=false;}
   return {complete:!cleanupFails,codex:true,claude_code:!cleanupFails,claude_desktop:true,reopen_failed:0};};
  if(deferred) return new Promise((resolve,reject)=>{resolveApply=()=>resolve(apply());rejectApply=reject;});return apply();
 }
 if(cmd==='quit_if_normal') {assert.ok(!state.codex_installed&&!state.claude.installed&&!state.claude.desktop_installed);quits++;return;}
 if(cmd==='quit_anyway') {quits++;return;}
 if(cmd==='set_destination'||cmd==='set_claude_destination'){const dest=args.target==='local'?{kind:'local'}:{kind:'peer',id:args.target};if(cmd==='set_destination')state.destination=dest;else state.claude.destination=dest;return;}
 if(cmd==='set_sharing'){state.sharing=args.on;return;}
 if(cmd==='set_allow_fast_mode'){state.allow_fast_mode=args.on;return;}
 if(cmd==='set_fast_mode'){state.fast_mode=args.on;return;}
 if(cmd==='set_claude_sharing'){if(sharingFails)throw 'Reconnect the Claude account.';state.claude.sharing=args.on;return;}
 if(cmd==='set_name'){state.name=args.name;return;}
 if(cmd==='diagnostics')return {persistence_ok:true,settings:[{at_unix_seconds:100,service:'codex',target:'codex_config',operation:'write',code:'permission_denied'}],incoming:[{at_unix_seconds:100,request:{provider:'claude',model:'claude-sonnet-4-6',route:'peer',attempt:1,reasoning_effort:'unspecified'},event:{phase:'completed',status:200}}],upstream:[{at_unix_seconds:100,request:{provider:'claude',reasoning_effort:'unspecified',forwarded_reasoning_effort:'medium'},event:{phase:'completed',status:200,effective_reasoning_effort:'medium'}},{at_unix_seconds:99,request:{provider:'claude',reasoning_effort:'unspecified'},event:{phase:'failed',status:429}}],entries:[],history:[{code:'damaged_record',stage:'prepare',record:7,chat_ref:'a'.repeat(64),at_unix_seconds:100}]};
 if(cmd==='export_diagnostics')return 'Downloads/synthetic.json';
 if(cmd==='begin_claude_connect'){connects++;return 'https://claude.ai/oauth/authorize?state=SYNTHETIC';}
 if(cmd==='finish_claude_connect'){assert.equal(args.code,codeValue);state.claude.connected=true;state.claude.reconnect_required=false;return;}
 if(cmd==='disconnect_claude_account'){state.claude.connected=false;return;}
 if(cmd==='grant_codex'){state.clients[0].codex=args.on;return;}
 if(cmd==='grant_claude'){state.clients[0].claude=args.on;return;}
 if(cmd==='start_pairing'){pairCalls++;assert.equal(args.code,pairValue.trim());return {id:'pair',name:'Friend <Mac>',short_id:'TEST',code:'BLUE 47 MOON'};}
 if(cmd==='confirm_pairing'){confirmCalls++;const paired={fp:'friend',name:'Friend <Mac>',platform:'macos'};return delayedPair?new Promise(resolve=>{resolvePair=()=>resolve(paired);}):paired;}
 if(cmd==='cancel_pairing'){cancels++;return;}
 if(cmd==='create_invite'){invites++;assert.ok(args.permissions.codex||args.permissions.claude);state.sharing ||= args.permissions.codex;state.claude.sharing ||= args.permissions.claude;return '00123456';}
 if(cmd==='create_full_invite'){invites++;assert.ok(args.permissions.codex||args.permissions.claude);return 'aillies1:SYNTHETIC';}
 if(cmd==='revoke_client'){state.clients=state.clients.filter(p=>p.fp!==args.fp);return;}
 throw Error(`Unexpected UI command: ${cmd}`);
}}}},setInterval:()=>0,setTimeout:()=>0,clearTimeout:()=>{},console});
vm.runInContext(fs.readFileSync('apps/desktop/ui/app.js','utf8'),context);
const flush=()=>new Promise(resolve=>setImmediate(resolve));await flush();
const click=dataset=>listeners.get('click')({target:{closest:()=>({dataset})}});
const refresh=()=>vm.runInContext('refresh(true)',context);
const html=()=>elements.app.innerHTML;
await click({go:'home'});
assert.match(html(),/class="app-header"/);assert.match(html(),/Connection details for Test Mac/);
assert.equal((html().match(/class="service-card"/g)||[]).length,2);
assert.doesNotMatch(html(),/Use Fast mode/);
assert.match(html(),/Aillies Use/);assert.match(html(),/Aillies Share/);
assert.match(html(),/Activate Aillies for Codex/);assert.match(html(),/Activate Aillies for Claude/);
assert.match(html(),/2 Aillies are sharing with you/); // same peer offering two services counted once
assert.doesNotMatch(html(),/Codex devices.*available|Claude devices.*available|data-toggle="routing"|Allow Fast mode/);
assert.match(html(),/Manage paired devices/);
await click({toggle:'sharing'});assert.match(html(),/Allow Fast mode/);assert.match(html(),/sharing with 1 Ailly/);
await click({toggle:'claude-sharing'});assert.match(html(),/sharing with 1 Ailly/); // grants union, not sum
await click({toggle:'sharing'});assert.doesNotMatch(html(),/Allow Fast mode/);
await click({toggle:'claude-sharing'});
// Dropdown discovery is read-only until an explicit choice. Services filter separately.
const beforeHealth=healthCalls, beforeCommands=commands.length;
await click({picker:'claude'});assert.ok(healthCalls>beforeHealth);
assert.match(html(),/This Mac/);assert.match(html(),/Your own account/);assert.match(html(),/Renamed &lt;Mac&gt;/);
assert.match(html(),/data-target="codex-only" disabled/);
assert.equal(state.claude.destination.kind,'local');
assert.ok(!commands.slice(beforeCommands).includes('set_claude_destination'));
await click({selectService:'claude',target:'both'});assert.equal(state.claude.destination.id,'both');
// Every activation uses an explicit reviewed, single-use operation. Cancel does not mutate.
await click({toggle:'codex'});assert.equal(state.codex_installed,false);assert.match(html(),/Quit, switch & reopen/);assert.match(html(),/Terminal and VS Code stay closed/);
assert.match(html(),/What happens to open apps/);assert.match(html(),/When I next quit Codex/);assert.match(html(),/About 12 saved chats/);
await click({do:'cancel-restart'});assert.equal(applies,0);assert.equal(state.codex_installed,false);
await click({toggle:'codex'});await click({activationMode:'later'});assert.match(html(),/Switch when I quit/);
await click({do:'restart-codex'});assert.match(html(),/Waiting for Codex to close/);assert.equal(applies,0);assert.equal(state.codex_installed,false);
await click({go:'home'});assert.match(html(),/Provider switch scheduled/);
await click({do:'cancel-scheduled'});assert.equal(state.activation,null);assert.equal(state.codex_installed,false);
await click({toggle:'codex'});deferred=true;
const changing=click({do:'restart-codex'});await click({do:'restart-codex'});assert.equal(applies,1);assert.equal(state.codex_installed,false);
assert.match(html(),/Updating your provider/);resolveApply();await changing;assert.equal(state.codex_installed,true);deferred=false;
state.allow_fast_mode=true;await refresh();assert.match(html(),/Use Fast mode/);
await click({toggle:'fast'});assert.equal(state.fast_mode,true);
state.allow_fast_mode=false;await refresh();assert.doesNotMatch(html(),/Use Fast mode/);
await click({toggle:'claude'});await click({do:'restart-codex'});assert.ok(state.claude.installed&&state.claude.desktop_installed);
await click({toggle:'claude'});deferred=true;const refused=click({do:'restart-codex'});rejectApply('Changed session inventory. Review again.');await refused;deferred=false;
assert.ok(state.claude.installed);assert.match(html(),/Changed session inventory/);assert.doesNotMatch(html(),/Open Automation settings/);assert.match(html(),/I’ll close them myself/);
await click({do:'cancel-restart'});
// Warnings are bounded/escaped and sit below Connection details.
state.recent_warnings=Array.from({length:5},(_,i)=>({at:100+i,code:'peer_unavailable',message:`Warning ${i} <img onerror=oops>`,retry:i===0?1:null}));await refresh();
assert.ok(html().indexOf('Connection details')<html().indexOf('Recent warnings'));
assert.equal((html().match(/data-request-warning/g)||[]).length,4);assert.doesNotMatch(html(),/Warning 0|<img onerror/);
// Details remain lean, editable and escaped; diagnostics is explicitly opened.
await click({go:'details'});assert.match(html(),/data-name/);assert.match(html(),/Version/);assert.match(html(),/Diagnostics/);assert.doesNotMatch(html(),/Known addresses|loopback|fingerprint|18522/);
await click({do:'rename'});assert.equal(state.name,nameValue);assert.match(html(),/New &lt;Mac&gt;/);
context.document.activeElement={matches:()=>true};const edit=html()+'<!-- retained typing -->';elements.app.innerHTML=edit;await refresh();assert.equal(html(),edit);context.document.activeElement=null;
await click({go:'pair-enter'});assert.match(html(),/across networks with Iroh/);assert.match(html(),/data-full-code/);assert.match(html(),/Same private network/);assert.doesNotMatch(html(),/Same Wi-Fi|same Wi-Fi/);
await click({go:'diagnostics'});await flush();assert.match(html(),/Latest 100/);assert.match(html(),/claude-sonnet-4-6/);assert.match(html(),/Export diagnostics/);
await click({logTab:'settings'});assert.match(html(),/codex_config/);assert.match(html(),/permission denied/);assert.match(html(),/Operation/);
await click({logTab:'history'});assert.match(html(),/damaged record/);assert.match(html(),/Chat reference/);assert.match(html(),/>7</);
await click({logTab:'upstream'});assert.match(html(),/Incoming effort/);assert.match(html(),/Outgoing effort/);assert.match(html(),/Upstream effort/);assert.match(html(),/medium/);assert.match(html(),/Not recorded/);await click({do:'export-diagnostics'});assert.equal(elements.toast.textContent,'Diagnostics saved to Downloads');
// Account reconnection never silently imports Keychain. Code stays masked and transient.
state.claude.connected=false;state.claude.reconnect_required=true;await refresh();await click({go:'home'});assert.match(html(),/Reconnect/);
await click({go:'claude'});assert.match(html(),/without Keychain prompts/);assert.match(html(),/Approve in your browser/);assert.match(html(),/Paste the code Claude shows/);
await click({do:'claude-connect'});assert.equal(connects,1);assert.match(html(),/type="password" data-claude-oauth-code/);
const entered=html()+'<!-- code retained -->';elements.app.innerHTML=entered;await refresh();assert.equal(html(),entered);
await click({do:'claude-connect-finish'});assert.match(html(),/Paste the authorization code/);
codeValue='SYNTHETIC_CODE#state';await click({do:'claude-connect-finish'});assert.equal(state.claude.connected,true);assert.doesNotMatch(html(),/SYNTHETIC_CODE|data-claude-oauth-code/);
// Paired-device permissions remain separate; peer strings cannot inject markup.
await click({go:'devices'});assert.match(html(),/Friend &lt;Mac&gt;/);await click({claudeGrant:'friend',on:'false'});assert.equal(state.clients[0].claude,false);
await click({revoke:'friend'});assert.equal(state.clients.length,1);assert.match(html(),/Revoke access/);
await click({do:'cancel-removal'});assert.equal(state.clients.length,1);
await click({go:'invite'});assert.equal(invites,0);assert.match(html(),/Create internet pairing code/);
await click({do:'invite'});assert.equal(invites,1);assert.match(html(),/aillies1:SYNTHETIC/);assert.match(html(),/Copy internet pairing code/);assert.match(html(),/10:00 left/);
await click({do:'full-invite'});assert.match(html(),/Internet is not connected yet/);assert.match(html(),/data-do="full-invite"[^>]*>New code/);state.internet_ready=true;await refresh();assert.doesNotMatch(html(),/Internet is not connected yet/);
vm.runInContext('ui.inviteCreated = Date.now() - 600001',context);await refresh();assert.match(html(),/code has expired/);assert.doesNotMatch(html(),/data-invite>/);
await click({inviteMethod:'private'});await click({do:'invite'});assert.match(html(),/0012 3456/);const shareBefore=state.sharing;await click({invitePermission:'codex'});assert.equal(state.sharing,shareBefore);assert.doesNotMatch(html(),/data-invite>/);await click({invitePermission:'codex'});
await click({go:'welcome'});assert.match(html(),/One plan/);assert.match(html(),/Use another computer/);assert.match(html(),/Share this Mac/);assert.match(html(),/Skip setup/);
assert.ok(!commands.includes('set_routing'));
await click({go:'pair'});pairValue=' ';await click({do:'start-pair'});assert.equal(pairCalls,0);
pairValue='aillies1:SYNTHETIC';await click({do:'start-pair'});assert.equal(pairCalls,1);assert.equal(confirmCalls,0);assert.match(html(),/BLUE 47 MOON/);
healthProviders=[{provider:'claude',account:{state:'ready'}}];
await click({do:'confirm-pair'});assert.equal(confirmCalls,1);await flush();
assert.match(html(),/Claude<span class="sub">Shared by Friend &lt;Mac&gt;/);
assert.match(html(),/Codex<span class="sub">Not shared with this device/);
assert.doesNotMatch(html(),/Set up Codex|data-go="codex"/);
// Cancellation remains available while waiting; a late approval cannot navigate away.
await click({go:'pair'});await click({pairMethod:'private'});pairValue='0012 3456';await click({do:'start-pair'});delayedPair=true;
const confirming=click({do:'confirm-pair'});assert.match(html(),/Cancel pairing/);
await click({do:'cancel-pair'});assert.equal(cancels,1);assert.match(html(),/Aillies Use/);
resolvePair();await confirming;assert.match(html(),/Aillies Use/);assert.doesNotMatch(html(),/is paired/);delayedPair=false;
await click({go:'home'});state.platform='windows';await refresh();
await click({toggle:'codex'});assert.match(html(),/Wait for me/);assert.match(html(),/Reopen desktop apps yourself/);
assert.doesNotMatch(html(),/Quit, switch & reopen|Only desktop apps reopen/);
await click({do:'restart-codex'});assert.match(html(),/Waiting for Codex to close/);assert.equal(state.codex_installed,true);state.codex_installed=false;state.activation.phase='completed';await refresh();assert.equal(elements.toast.textContent,'Provider switched. Reopen your desktop app to continue.');
state.platform='macos';await refresh();
// A broken config/process review can never trap the user. Complete cleanup uses normal exit.
await click({go:'quit'});cleanupFails=true;await click({do:'disconnect-quit'});await click({do:'restart-codex'});assert.match(html(),/Cleanup incomplete/);assert.match(html(),/Quit anyway/);assert.equal(quits,0);
await click({do:'quit-anyway'});assert.equal(quits,1);
reviewFails=true;await click({do:'disconnect-quit'});assert.match(html(),/unrecognized session/);assert.match(html(),/Quit anyway/);reviewFails=false;
cleanupFails=false;await click({do:'disconnect-quit'});await click({do:'restart-codex'});assert.equal(quits,2);
// Permission recovery is explicit on every platform and never switches providers.
for(const platform of ['macos','windows','linux']) {
 state.platform=platform;state.activation={id:'permission-failure',service:'codex',on:true,phase:'failed',failed_stage:'settings',error:'Settings access is blocked.',permission_issues:[{target:'codex_config',operation:'write',code:'permission_denied',can_repair:true}]};
 await refresh();await click({go:'activation-error'});assert.match(html(),/Grant write access/);assert.doesNotMatch(html(),/Open Automation settings|Switch without closing clients/);
 assert.match(html(),platform==='windows'?/UAC/:platform==='linux'?/polkit/:/administrator approval/);
 const appliedBefore=applies;await click({do:'repair-settings-access'});assert.equal(applies,appliedBefore);assert.match(html(),/Write access is ready/);assert.doesNotMatch(html(),/Grant write access/);
}
state.platform='macos';state.activation=null;await click({go:'home'});
console.log('Settings UI: unified activation, discovery/selection, editable name, sharing grants/Fast, OAuth, diagnostics and exit gate passed');

// Update UI never accepts a download URL/package and waits for explicit install consent.
update={phase:'ready',version:'1.2.0 <bad>'};await vm.runInContext('refresh()',context);
assert.match(elements.app.innerHTML,/Update & restart/);assert.ok(!elements.app.innerHTML.includes('1.2.0 <bad>'));
await click({do:'install-update'});assert.equal(update.phase,'waiting');assert.match(elements.app.innerHTML,/Later/);
await click({do:'cancel-update'});assert.equal(update.phase,'ready');
console.log('Update consent, escaping and postpone contracts passed.');

// Both services use node-owned recovery state; late UI probes cannot overwrite a newer network generation.
state.destination={kind:'peer',id:'both'};state.claude.destination={kind:'peer',id:'codex-only'};
state.network={generation:1,peers:[{fp:'both',phase:'reconnecting',health:null},{fp:'codex-only',phase:'reconnecting',health:null}]};
await refresh();await click({go:'home'});assert.equal((html().match(/Reconnecting…/g)||[]).length,2);
state.network={generation:2,peers:[{fp:'both',phase:'ready',health:{reachable:true,status:{sharing:true,providers:[{provider:'codex',account:{state:'ready'}}]}}},{fp:'codex-only',phase:'offline',health:{reachable:false}}]};
await refresh();assert.match(html(),/Ready/);assert.match(html(),/pair again with a full code/);assert.doesNotMatch(html(),/Reconnecting…/);
console.log('Codex and Claude network recovery and legacy pairing guidance passed.');

holdHealth=true;
const staleProbe=vm.runInContext('checkHealth("both")',context);
state.network.generation=3;
await refresh();finishHealth({reachable:false});await staleProbe;holdHealth=false;
assert.equal(vm.runInContext('ui.health.both.reachable',context),true);
console.log('Late probe results from a previous network are discarded.');
