// Behavior checks with synthetic state only; never touches native apps/accounts/configs.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const listeners=new Map(), elements={app:{innerHTML:''},toast:{hidden:true,textContent:''}};
const state={name:'Test Mac',platform:'macos',version:'1.1.0',state:'local',routing:true,sharing:false,allow_fast_mode:false,destination:{kind:'local'},peers:[{fp:'both',name:'Both <Mac>',platform:'macos'},{fp:'codex-only',name:'Codex Mac',platform:'macos'}],clients:[{fp:'friend',name:'Friend <Mac>',codex:true,claude:true}],pending:[],codex_installed:false,local_account:{state:'ready'},claude:{connected:true,reconnect_required:false,sharing:false,installed:false,desktop_installed:false,destination:{kind:'local'}}};
let healthProviders=null;
let nameValue='New <Mac>', codeValue='', pairValue='0012 3456', plan=null, resolveApply, rejectApply, deferred=false, cleanupFails=false, reviewFails=false, sharingFails=false, healthCalls=0, applies=0, quits=0, connects=0, pairCalls=0, confirmCalls=0;
const commands=[];
const context=vm.createContext({document:{getElementById:id=>elements[id],querySelector:selector=>({value:selector==='[data-name]'?nameValue:selector==='[data-claude-oauth-code]'?codeValue:pairValue}),activeElement:null,addEventListener:(type,fn)=>listeners.set(type,fn)},window:{__TAURI__:{core:{invoke:async(cmd,args={})=>{
 commands.push(cmd);
 if(cmd==='status') return structuredClone(state);
 if(cmd==='startup_error'||cmd==='take_navigation'||cmd==='open_claude_authorization'||cmd==='cancel_claude_connect') return null;
 if(cmd==='peer_health') {healthCalls++;return {reachable:true,status:{name:args.fp==='both'?'Renamed <Mac>':'Codex Mac',sharing:true,providers:healthProviders??[{provider:'codex',account:{state:'ready'}},...(args.fp==='both'?[{provider:'claude',account:{state:'ready'}}]:[])]}};}
 if(cmd==='prepare_activation') {if(reviewFails) throw 'Close the unrecognized session yourself.'; plan={token:'one-use',service:args.service,on:args.on,apps:args.service==='claude'?['Claude']:['ChatGPT','Visual Studio Code'],terminals:2,manual_close:state.platform!=='macos'};return structuredClone(plan);}
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
 if(cmd==='set_claude_sharing'){if(sharingFails)throw 'Reconnect the Claude account.';state.claude.sharing=args.on;return;}
 if(cmd==='set_name'){state.name=args.name;return;}
 if(cmd==='diagnostics')return {persistence_ok:true,incoming:[{at_unix_seconds:100,request:{provider:'claude',model:'claude-sonnet-4-6',route:'peer',attempt:1},event:{phase:'completed',status:200}}],upstream:[],entries:[]};
 if(cmd==='export_diagnostics')return 'Downloads/synthetic.json';
 if(cmd==='begin_claude_connect'){connects++;return 'https://claude.ai/oauth/authorize?state=SYNTHETIC';}
 if(cmd==='finish_claude_connect'){assert.equal(args.code,codeValue);state.claude.connected=true;state.claude.reconnect_required=false;return;}
 if(cmd==='disconnect_claude_account'){state.claude.connected=false;return;}
 if(cmd==='grant_codex'){state.clients[0].codex=args.on;return;}
 if(cmd==='grant_claude'){state.clients[0].claude=args.on;return;}
 if(cmd==='start_pairing'){pairCalls++;assert.equal(args.code,pairValue.trim());return {id:'pair',name:'Friend <Mac>',short_id:'TEST',code:'BLUE 47 MOON'};}
 if(cmd==='confirm_pairing'){confirmCalls++;return {fp:'friend',name:'Friend <Mac>',platform:'macos'};}
 throw Error(`Unexpected UI command: ${cmd}`);
}}}},setInterval:()=>0,setTimeout:()=>0,clearTimeout:()=>{},console});
vm.runInContext(fs.readFileSync('apps/desktop/ui/app.js','utf8'),context);
const flush=()=>new Promise(resolve=>setImmediate(resolve));await flush();
const click=dataset=>listeners.get('click')({target:{closest:()=>({dataset})}});
const refresh=()=>vm.runInContext('refresh(true)',context);
const html=()=>elements.app.innerHTML;
await click({go:'home'});
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
await click({do:'cancel-restart'});assert.equal(applies,0);assert.equal(state.codex_installed,false);
await click({toggle:'codex'});deferred=true;
const changing=click({do:'restart-codex'});await click({do:'restart-codex'});assert.equal(applies,1);assert.equal(state.codex_installed,false);
assert.match(html(),/Updating your provider/);resolveApply();await changing;assert.equal(state.codex_installed,true);deferred=false;
await click({toggle:'claude'});await click({do:'restart-codex'});assert.ok(state.claude.installed&&state.claude.desktop_installed);
await click({toggle:'claude'});deferred=true;const refused=click({do:'restart-codex'});rejectApply('Changed session inventory. Review again.');await refused;deferred=false;
assert.ok(state.claude.installed);assert.match(html(),/Changed session inventory/);
// Warnings are bounded/escaped and sit below Connection details.
state.recent_warnings=Array.from({length:5},(_,i)=>({at:100+i,code:'peer_unavailable',message:`Warning ${i} <img onerror=oops>`,retry:i===0?1:null}));await refresh();
assert.ok(html().indexOf('Connection details')<html().indexOf('Recent warnings'));
assert.equal((html().match(/data-request-warning/g)||[]).length,4);assert.doesNotMatch(html(),/Warning 4|<img onerror/);
// Details remain lean, editable and escaped; diagnostics is explicitly opened.
await click({go:'details'});assert.match(html(),/data-name/);assert.match(html(),/Version/);assert.match(html(),/Diagnostics/);assert.doesNotMatch(html(),/Known addresses|loopback|fingerprint|18522/);
await click({do:'rename'});assert.equal(state.name,nameValue);assert.match(html(),/New &lt;Mac&gt;/);
context.document.activeElement={matches:()=>true};const edit=html()+'<!-- retained typing -->';elements.app.innerHTML=edit;await refresh();assert.equal(html(),edit);context.document.activeElement=null;
await click({go:'diagnostics'});await flush();assert.match(html(),/Latest 100/);assert.match(html(),/claude-sonnet-4-6/);assert.match(html(),/Export diagnostics/);
await click({logTab:'upstream'});assert.match(html(),/No activity recorded/);await click({do:'export-diagnostics'});assert.equal(elements.toast.textContent,'Diagnostics saved to Downloads');
// Account reconnection never silently imports Keychain. Code stays masked and transient.
state.claude.connected=false;state.claude.reconnect_required=true;await refresh();await click({go:'home'});assert.match(html(),/Reconnect/);
await click({go:'claude'});assert.match(html(),/without Keychain prompts/);
await click({do:'claude-connect'});assert.equal(connects,1);assert.match(html(),/type="password" data-claude-oauth-code/);
const entered=html()+'<!-- code retained -->';elements.app.innerHTML=entered;await refresh();assert.equal(html(),entered);
await click({do:'claude-connect-finish'});assert.match(html(),/Paste the authorization code/);
codeValue='SYNTHETIC_CODE#state';await click({do:'claude-connect-finish'});assert.equal(state.claude.connected,true);assert.doesNotMatch(html(),/SYNTHETIC_CODE|data-claude-oauth-code/);
// Paired-device permissions remain separate; peer strings cannot inject markup.
await click({go:'devices'});assert.match(html(),/Friend &lt;Mac&gt;/);await click({claudeGrant:'friend',on:'false'});assert.equal(state.clients[0].claude,false);
await click({go:'welcome'});assert.match(html(),/shared Codex or Claude account/);await click({go:'pair'});pairValue=' ';await click({do:'start-pair'});assert.equal(pairCalls,0);
pairValue='0012 3456';await click({do:'start-pair'});assert.equal(pairCalls,1);assert.equal(confirmCalls,0);assert.match(html(),/BLUE 47 MOON/);
healthProviders=[{provider:'claude',account:{state:'ready'}}];
await click({do:'confirm-pair'});assert.equal(confirmCalls,1);await flush();
assert.match(html(),/Claude<span class="sub">Shared by Friend &lt;Mac&gt;/);
assert.match(html(),/Codex<span class="sub">Not shared with this device/);
assert.doesNotMatch(html(),/Set up Codex|data-go="codex"/);
await click({go:'home'});state.platform='windows';await refresh();
await click({toggle:'codex'});assert.match(html(),/Switch provider/);assert.match(html(),/Reopen desktop apps yourself/);
assert.doesNotMatch(html(),/Quit, switch & reopen|Only desktop apps reopen/);
await click({do:'restart-codex'});assert.equal(elements.toast.textContent,'Provider switched. Reopen your desktop app to continue.');
state.platform='macos';await refresh();
// A broken config/process review can never trap the user. Complete cleanup uses normal exit.
await click({go:'quit'});cleanupFails=true;await click({do:'disconnect-quit'});await click({do:'restart-codex'});assert.match(html(),/Cleanup incomplete/);assert.match(html(),/Quit anyway/);assert.equal(quits,0);
await click({do:'quit-anyway'});assert.equal(quits,1);
reviewFails=true;await click({do:'disconnect-quit'});assert.match(html(),/unrecognized session/);assert.match(html(),/Quit anyway/);reviewFails=false;
cleanupFails=false;await click({do:'disconnect-quit'});await click({do:'restart-codex'});assert.equal(quits,2);
console.log('Settings UI: unified activation, discovery/selection, editable name, sharing grants/Fast, OAuth, diagnostics and exit gate passed');
