const $ = id => document.getElementById(id);
let selected = 'direct', busy = false;
async function post(path, data) {
  const response = await fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) });
  if (!response.ok) throw new Error((await response.json()).error);
  return response;
}
function showError(error) { $('error').textContent = error.message; }
async function refresh() {
  const response = await fetch('/api/state');
  if (!response.ok) throw new Error('Could not load machine settings');
  const state = await response.json(); selected = state.selected;
  $('peers').replaceChildren(); $('count').textContent = `${state.peers.length} saved`;
  $('proxy').textContent = `http://127.0.0.1:${state.proxyPort}`;
  const peer = state.peers.find(p => p.id === selected);
  $('route').textContent = peer ? peer.name : 'Choose a machine';
  $('send').disabled = !peer || busy;
  if (!state.peers.length) { const empty = document.createElement('p'); empty.className = 'empty'; empty.textContent = 'Your machines will appear here.'; $('peers').append(empty); }
  for (const peer of state.peers) {
    const row = document.createElement('div'); row.className = `peer${peer.id === selected ? ' selected' : ''}`;
    const choose = document.createElement('button'); choose.type = 'button'; choose.disabled = busy; choose.setAttribute('aria-pressed', String(peer.id === selected));
    const name = document.createElement('span'); name.textContent = `${peer.id === selected ? '● ' : '○ '}${peer.name}`;
    const alias = document.createElement('small'); alias.textContent = peer.sshAlias; choose.append(name, alias);
    choose.onclick = async () => { try { await post('/api/select', { id: peer.id }); await refresh(); } catch (error) { showError(error); } };
    const remove = document.createElement('button'); remove.className = 'remove'; remove.textContent = '×'; remove.setAttribute('aria-label', `Remove ${peer.name}`); remove.disabled = busy;
    remove.onclick = async () => { try { await post('/api/remove', { id: peer.id }); await refresh(); } catch (error) { showError(error); } };
    row.append(choose, remove); $('peers').append(row);
  }
}
$('add').onsubmit = async event => {
  event.preventDefault(); $('error').textContent = '';
  const data = Object.fromEntries(new FormData(event.target));
  try { await post('/api/peers', data); event.target.reset(); await refresh(); } catch (error) { showError(error); }
};
$('ask').onsubmit = async event => {
  event.preventDefault(); if (busy || selected === 'direct') return;
  busy = true; $('send').disabled = true; $('error').textContent = ''; $('answer').textContent = ''; $('status').textContent = 'Connecting to peer…';
  let messages = new Map(), pending = '', completed = false;
  const handle = item => {
    if (item.type === 'peer.selected') $('status').textContent = `Running on ${item.name}…`;
    if (item.item?.type === 'agent_message' && item.item.text) {
      messages.set(item.item.id || 'answer', item.item.text); $('answer').textContent = [...messages.values()].join('\n\n');
    }
    if (item.type === 'turn.completed') { completed = true; $('status').textContent = 'Response complete'; }
    if (item.type === 'error' || item.type === 'turn.failed' || item.type === 'relay.error') throw new Error(item.message || item.error?.message || 'Peer request failed');
  };
  try {
    await refresh();
    const response = await post('/api/chat', { prompt: $('prompt').value });
    const reader = response.body.getReader(), decoder = new TextDecoder();
    try {
      for (;;) {
        const { value, done } = await reader.read();
        pending += decoder.decode(value, { stream: !done });
        let index;
        while ((index = pending.indexOf('\n')) >= 0) { const line = pending.slice(0, index); pending = pending.slice(index + 1); if (line) handle(JSON.parse(line)); }
        if (done) break;
      }
    } finally { await reader.cancel(); }
    if (!completed) throw new Error('The peer disconnected before completing the response');
  } catch (error) { showError(error); $('status').textContent = 'Request failed'; }
  finally { busy = false; await refresh().catch(showError); }
};
refresh().catch(showError);
