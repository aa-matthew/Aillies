// UI behavior contract with a fake bridge; no real settings or browser are touched.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const listeners=new Map(), elements={app:{innerHTML:''},toast:{hidden:true,textContent:''}};
const state={name:'Test Mac',platform:'macos',short_id:'TEST',version:'1.0.0',diagnostics_build:'router-core-v4-pairing',state:'local',routing:true,sharing:true,destination:{kind:'local'},peers:[],clients:[],pending:[],codex_installed:true,local_account:{state:'ready'}};
let resolveSwitch, rejectSwitch, calls=0, restartCalls=0, pendingOn, navigation=null, quits=0, pairCalls=0, confirmCalls=0, pairingCode="0012 3456", fullCode="";
const context=vm.createContext({
 document:{getElementById:id=>elements[id],querySelector:selector=>({value:selector==="[data-full-code]"?fullCode:pairingCode}),activeElement:null,addEventListener:(type,fn)=>listeners.set(type,fn)},
 window:{__TAURI__:{core:{invoke:async(cmd,args)=>{
  if(cmd==='status') return structuredClone(state);
  if(cmd==='startup_error') return null;
  if(cmd==='take_navigation') { const next=navigation; navigation=null; return next; }
  if(cmd==='quit_if_normal') { assert.equal(state.codex_installed,false); quits++; return; }
  if(cmd==='start_pairing') { pairCalls++; assert.equal(args.code,pairingCode.trim() || fullCode.trim()); return {id:'pair',code:'BLUE 47 MOON',name:'Other <Mac>',short_id:'TEST'}; }
  if(cmd==='confirm_pairing') { confirmCalls++; assert.equal(args.id,'pair'); return {fp:'synthetic',name:'Other <Mac>',short_id:'TEST',platform:'macos'}; }
  if(cmd==='peer_health') return {reachable:true,status:{providers:[{provider:'codex',account:{state:'ready'}}]}};
  if(cmd==='create_invite') return '00123456';
  if(cmd==='create_full_invite') return 'aillies1:synthetic';
  if(cmd==='export_diagnostics') return 'Downloads/synthetic.json';
  if(cmd==='prepare_codex_restart') { pendingOn=args.on; return {token:'one-use',on:args.on,apps:['ChatGPT','Visual Studio Code'],terminals:2}; }
  if(cmd==='restart_codex_provider') { restartCalls++; assert.equal(args.token,'one-use'); return new Promise((resolve,reject)=>{resolveSwitch=()=>{state.codex_installed=pendingOn;resolve({chats:7,reopen_failed:0});};rejectSwitch=reject;}); }
  if(cmd==='switch_codex_provider') { calls++; return new Promise((resolve,reject)=>{resolveSwitch=()=>{state.codex_installed=args.on;resolve({reopen_required:true})};rejectSwitch=reject;}); }
  throw Error('Unexpected UI command');
 }}}},setInterval:()=>0,setTimeout:()=>0,clearTimeout:()=>{},console
});
vm.runInContext(fs.readFileSync('apps/desktop/ui/app.js','utf8'),context);
await new Promise(resolve=>setImmediate(resolve));
const click=dataset=>listeners.get('click')({target:{closest:()=>({dataset})}});
// Home exposes provider selection instead of a routing pause, and hides unused controls.
await click({go:'home'});
assert.match(elements.app.innerHTML,/Use Aillies with/);
assert.doesNotMatch(elements.app.innerHTML,/data-toggle="routing"/);
assert.match(elements.app.innerHTML,/Allow Fast mode/);
state.sharing=false; await vm.runInContext('refresh(true)',context);
assert.doesNotMatch(elements.app.innerHTML,/Allow Fast mode/);
state.codex_installed=false; await vm.runInContext('refresh(true)',context);
assert.match(elements.app.innerHTML,/Normal provider/);
assert.doesNotMatch(elements.app.innerHTML,/data-dest|data-toggle="fast"/);
await click({go:'pair'});
assert.match(elements.app.innerHTML,/data-code/);
assert.match(elements.app.innerHTML,/inputmode="numeric"/);
assert.match(elements.app.innerHTML,/maxlength="9"/);
assert.match(elements.app.innerHTML,/Use a full pairing code/);
assert.match(elements.app.innerHTML,/Show a pairing code/);
assert.doesNotMatch(elements.app.innerHTML,/Use another device’s account<\/strong>/);
state.sharing=true; state.codex_installed=true; await vm.runInContext('refresh(true)',context);
await click({go:'codex'});
assert.match(elements.app.innerHTML,/Codex provider/);
assert.match(elements.app.innerHTML,/Claude Code/);
assert.match(elements.app.innerHTML,/Upcoming/);
const change=(value='normal')=>listeners.get('change')({target:{value,matches:s=>s==='[data-provider]'}});
const switching=change();
assert.match(elements.app.innerHTML,/Updating your chats/);
state.provider_switch={phase:'preparing',completed:2,total:12};
await vm.runInContext('refresh(true)',context);
assert.match(elements.app.innerHTML,/Preparing chat history… 2 of 12/);
assert.match(elements.app.innerHTML,/value="normal" selected/);
assert.equal(state.codex_installed,true);
state.provider_switch={phase:'applying',completed:2,total:12};
await vm.runInContext('refresh(true)',context);
assert.match(elements.app.innerHTML,/Saving your provider choice/);
state.provider_switch=null;
assert.match(elements.app.innerHTML,/aria-label="Codex provider" disabled/);
change(); assert.equal(calls,1); // polling/double input cannot start a second migration
resolveSwitch(); await switching;
assert.match(elements.app.innerHTML,/value="normal" selected/);
assert.match(elements.toast.textContent,/Reopen Codex/);
await click({go:'details'});
await click({do:'export-diagnostics'});
assert.equal(elements.toast.textContent,'Diagnostics saved to Downloads');
await click({go:'codex'});
const failed=change('aillies');rejectSwitch('Could not switch providers.');await failed;
assert.match(elements.toast.textContent,/Could not switch/);
assert.match(elements.app.innerHTML,/value="normal" selected/);
assert.doesNotMatch(elements.app.innerHTML,/aria-label="Codex provider" disabled/);
// A blocked switch only reviews sessions. Cancellation never closes anything.
const review=change('aillies');
assert.match(elements.app.innerHTML,/value="aillies" selected/); rejectSwitch('Quit Codex, then switch providers here. Your chats will be preserved.'); await review;
assert.match(elements.app.innerHTML,/Quit, switch &amp; reopen|Quit, switch & reopen/);
assert.match(elements.app.innerHTML,/Visual Studio Code/);
assert.match(elements.app.innerHTML,/Only ChatGPT will reopen/);
assert.match(elements.app.innerHTML,/value="aillies" selected/);
await vm.runInContext('refresh(true)',context);
assert.match(elements.app.innerHTML,/value="aillies" selected/);
assert.equal(state.codex_installed,false);
assert.equal(restartCalls,0);
await click({do:'cancel-restart'});
assert.doesNotMatch(elements.app.innerHTML,/Ready to switch/);
assert.match(elements.app.innerHTML,/value="normal" selected/);
const reviewAgain=change('aillies'); rejectSwitch('Quit Codex, then switch providers here.'); await reviewAgain;
const restart=click({do:'restart-codex'});
await click({do:'restart-codex'}); assert.equal(restartCalls,1);
assert.match(elements.app.innerHTML,/Closing Codex and switching/);
resolveSwitch(); await restart;
assert.doesNotMatch(elements.app.innerHTML,/Ready to switch/);
assert.match(elements.toast.textContent,/Provider switched/);
assert.match(elements.app.innerHTML,/value="aillies" selected/);
assert.equal(state.codex_installed,true);
// An execution failure restores the committed provider and keeps its message visible.
const finalReview=change('normal');rejectSwitch('Quit Codex, then switch providers here.');await finalReview;
const finalRestart=click({do:'restart-codex'});rejectSwitch('Codex history could not be updated safely.');await finalRestart;
assert.match(elements.app.innerHTML,/value="aillies" selected/);
assert.match(elements.app.innerHTML,/role="alert"/);
assert.match(elements.app.innerHTML,/history could not be updated safely/);
assert.doesNotMatch(elements.app.innerHTML,/aria-label="Codex provider" disabled/);
// Recent request warnings survive success and polling, stay bounded, and render as text.
state.recent_warnings=Array.from({length:5},(_,i)=>({at:100+i,code:'upstream_rejected',retry:i < 3 ? i+1 : null,message:`Warning ${i} <img onerror=oops>`}));
await vm.runInContext('refresh(true)',context);
await click({go:'home'});
assert.match(elements.app.innerHTML,/Recent warnings/);
assert.equal((elements.app.innerHTML.match(/data-request-warning/g)||[]).length,4);
assert.doesNotMatch(elements.app.innerHTML,/Warning 4|<img onerror/);
assert.match(elements.app.innerHTML,/&lt;img onerror=oops&gt;/);
await vm.runInContext('refresh(true)',context);
assert.equal((elements.app.innerHTML.match(/data-request-warning/g)||[]).length,4);
// Tray intents use the same review; cancellation keeps the current provider and app open.
navigation={screen:'codex',on:false};
const traySwitch=vm.runInContext('refresh(true)',context);
await new Promise(resolve=>setImmediate(resolve));
assert.match(elements.app.innerHTML,/value="normal" selected/);
rejectSwitch('Quit Codex, then switch providers here.'); await traySwitch;
await click({do:'cancel-restart'});
assert.equal(state.codex_installed,true); assert.equal(quits,0);
// Quit is a deliberate Normal switch; it cannot exit while a review is pending.
navigation={screen:'quit'}; await vm.runInContext('refresh(true)',context);
assert.match(elements.app.innerHTML,/Switch to Normal/);
const quitting=click({do:'normal-and-quit'});
rejectSwitch('Quit Codex, then switch providers here.'); await quitting;
assert.equal(quits,0);
await click({do:'cancel-restart'}); assert.equal(quits,0);
await click({go:'quit'});
const quitSuccess=click({do:'normal-and-quit'}); resolveSwitch(); await quitSuccess;
assert.equal(quits,1); assert.equal(state.codex_installed,false);
// Direct code entry still requires matching codes and approval; empty codes never connect.
navigation={screen:'pair'}; await vm.runInContext('refresh(true)',context);
pairingCode=' '; await click({do:'start-pair'});
assert.equal(pairCalls,0); assert.match(elements.app.innerHTML,/Enter a pairing code/);
pairingCode='0012 3456'; await click({do:'start-pair'});
assert.equal(pairCalls,1); assert.match(elements.app.innerHTML,/BLUE 47 MOON/);
assert.match(elements.app.innerHTML,/Other &lt;Mac&gt;/); assert.equal(confirmCalls,0);
await click({do:'confirm-pair'}); assert.equal(confirmCalls,1);
assert.match(elements.app.innerHTML,/Approved on both devices/);
navigation={screen:'error',message:'Control <error>'}; await vm.runInContext('refresh(true)',context);
assert.match(elements.app.innerHTML,/Control &lt;error&gt;/);
await click({go:'pair'});
await click({do:'invite'});
assert.match(elements.app.innerHTML,/0012 3456/);
assert.doesNotMatch(elements.app.innerHTML,/aillies1:synthetic/);
await click({do:'full-invite'});
assert.match(elements.app.innerHTML,/aillies1:synthetic/);
await click({go:'pair'}); pairingCode=''; fullCode='aillies1:synthetic';
await click({do:'start-pair'}); assert.equal(pairCalls,2);
await click({do:'confirm-pair'}); assert.equal(confirmCalls,2);
console.log('Settings UI: provider/tray review, cancellation, safe quit, code pairing, warnings and diagnostics passed');
