import fs from 'node:fs';
import vm from 'node:vm';
import {execFileSync} from 'node:child_process';
// Use a checkout pinned to ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8. No network calls.
const checkout=process.argv[2];
if (!checkout) throw new Error('Usage: node scripts/9router-fixtures.mjs /path/to/9router');
if (execFileSync('git',['-C',checkout,'rev-parse','HEAD'],{encoding:'utf8'}).trim() !== 'ce4460ef79382bfddb4aa5fc0ff9f3cb0d5f95a8') throw new Error('Unexpected 9router revision');
const root=checkout+'/open-sse/';
const ctx=vm.createContext({});
const read=p=>fs.readFileSync(root+p,'utf8');
vm.runInContext(read('providers/registry/codex.js').replace(/^import.*\n/gm,'').replace('export default','globalThis.registry ='),ctx);
vm.runInContext(read('config/codexInstructions.js').replace('export const','const'),ctx);
vm.runInContext(read('utils/codexToolSchema.js').replaceAll('export function','function'),ctx);
vm.runInContext(`
class BaseExecutor { constructor(){} }
const PROVIDERS={codex:{}};
const getProviderModels=()=>registry.models;
const getModelUpstreamId=(_,model)=>registry.models.find(m=>m.id===model)?.upstreamModelId || model;
const getThinkingLevels=(_,model)=>registry.models.find(m=>m.id===model)?.thinkingLevels ||
  (['gpt-5.6-sol','gpt-5.6-terra'].includes(model) ? ['none','minimal','low','medium','high','xhigh','max','ultra'] :
  model.startsWith('gpt-6') || model==='gpt-5.6-luna' ? ['none','minimal','low','medium','high','xhigh','max'] : ['none','minimal','low','medium','high','xhigh']);
const resolveSessionId=({body})=>body.prompt_cache_key;
const dbg=()=>{};
const RESPONSES_ITEM={MESSAGE:'message',USER:'user',INPUT_TEXT:'input_text'};
const ROLE={USER:'user'};
`,ctx);
const normalize=read('translator/formats/responsesApi.js').split('export function normalizeResponsesInput')[1].split('// Strict Responses upstreams')[0];
vm.runInContext('function normalizeResponsesInput'+normalize,ctx);
vm.runInContext(read('executors/codex.js').replace(/^import[\s\S]*?;\n/gm,'').replace('export class CodexExecutor','class CodexExecutor')+'\nglobalThis.transform=(body)=>new CodexExecutor().transformRequest(body.model,body,true,{});',ctx);
const fixtures=[
 ['astra-default',{model:'gpt-6-astra',input:'hello'}],
 ['sol-lite-default',{model:'gpt-6.1-sol',input:'hello'}],
 ['astra-explicit',{model:'gpt-6-astra',input:'hello',instructions:'my instruction',reasoning:{effort:'low'}}],
 ['lite-high',{model:'gpt-6-sol',input:'hello',instructions:'my instruction',reasoning:{effort:'high'}}],
 ['lite-ultra',{model:'gpt-6.1-sol',input:'hello',reasoning:{effort:'ultra'}}],
 ['lite-minimal',{model:'gpt-6-luna',input:'hello',reasoning:{effort:'minimal'}}],
 ['extended-context-alias',{model:'gpt-6-sol[1m]',input:'hello',reasoning:{effort:'high'}}],
 ['ordinary-56',{model:'gpt-5.6-sol',input:'hello',reasoning:{effort:'ultra'}}],
 ['nested-function',{model:'gpt-6-astra',input:'hello',tools:[{type:'function',function:{name:'check',parameters:{type:'object',properties:{pattern:{type:'string',pattern:'\\p{L}+'},literal:{type:'string',pattern:'[a-z]+'}}}}}]}],
 ['sampling-fields',{model:'gpt-6-astra',input:'hello',temperature:0.5,max_output_tokens:100,metadata:{},stream_options:{}}],
 ['hosted-search',{model:'gpt-6.1-sol',input:'hello',tools:[{type:'web_search'}]}],
 ['priority',{model:'gpt-6-astra',input:'hello',service_tier:'priority'}],
 ['lite-summary',{model:'gpt-6.1-sol',input:'hello',reasoning:{effort:'high',summary:'auto'}}],
];
const output=fixtures.map(([name,raw])=>{
 const request={...raw,prompt_cache_key:'fixture-session'};
 return {name,request,expected:ctx.transform(structuredClone(request))};
});
fs.writeFileSync('crates/aillies-codex/tests/9router-fixtures.json',JSON.stringify(output,null,2)+'\n');
console.log('Generated',output.length,'synthetic fixtures using pinned 9router transformRequest');

// Exercise the actual pinned resolver, retaining only identity relationships for random IDs.
const {default:crypto} = await import('node:crypto');
const sessionContext=vm.createContext({crypto,setInterval:()=>({unref(){}})});
vm.runInContext('const MEMORY_CONFIG={sessionTtlMs:7200000,sessionCleanupIntervalMs:1800000};\n'+read('utils/sessionManager.js').replace(/^import.*\n/gm,'').replaceAll('export function','function'),sessionContext);
vm.runInContext('globalThis.resolve=resolveSessionId;',sessionContext);
const assistant=s=>({input:[{role:'assistant',content:s}]});
const sessionCases=[
 ['header-before-cache',{headers:{'x-session-id':'header'},body:{prompt_cache_key:'cache'}}],
 ['request-header-before-body',{headers:{'x-client-request-id':'request'},body:{prompt_cache_key:'cache'}}],
 ['cache-before-session',{body:{prompt_cache_key:'cache',session_id:'body'}}],
 ['conversation-hint',{body:{conversation_id:'conversation'}}],
 ['metadata-hint',{body:{metadata:{user_id:'metadata'}}}],
 ['account-fallback',{body:{input:'hello'}}],
 ['same-account-fallback',{body:{input:'different'}}],
 ['different-account-fallback',{connectionId:'account-b',body:{input:'hello'}}],
 ['short-assistant-fallback',{body:assistant('a'.repeat(49))}],
 ['assistant-threshold',{body:assistant('a'.repeat(50))}],
 ['same-assistant-prefix',{body:assistant('a'.repeat(50)+'suffix')}],
 ['assistant-blocks',{body:assistant([{text:'a'.repeat(25)},{text:'a'.repeat(25)}])}],
 ['empty-text-output',{body:assistant([{text:'',output:'a'.repeat(50)}])}],
 ['utf16-boundary',{body:assistant('😀'.repeat(24)+'a😀suffix')}],
 ['same-utf16-prefix',{body:assistant('😀'.repeat(24)+'a😁different')}],
];
const groups=new Map();
const sessions=sessionCases.map(([name,opts])=>{
 const options={scope:'codex',connectionId:'account-a',...opts};
 const resolved=sessionContext.resolve(options);
 const explicit=!!(options.headers || options.body.prompt_cache_key || options.body.conversation_id || options.body.metadata);
 if(!explicit && !groups.has(resolved)) groups.set(resolved,groups.size);
 return {name,...options,expected:explicit?resolved:null,group:explicit?null:groups.get(resolved)};
});
fs.writeFileSync('crates/aillies-codex/tests/9router-sessions.json',JSON.stringify(sessions,null,2)+'\n');
console.log('Generated',sessions.length,'session relationships using pinned 9router resolver');
