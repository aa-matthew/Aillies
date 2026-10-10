import { test } from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createApp, destination, validatePeer } from '../server.mjs';

test('destination and SSH alias validation reject arbitrary hosts and command injection', () => {
  assert.equal(destination('api.openai.com:443'), 'api.openai.com');
  for (const value of ['openai.com.evil.test:443', 'evilopenai.com:443', '127.0.0.1:443', 'chatgpt.com:80', 'chatgpt.com:443/path']) assert.throws(() => destination(value));
  for (const sshAlias of ['-oProxyCommand=whoami', 'a;whoami', 'a b', '$(whoami)']) assert.throws(() => validatePeer({ name: 'peer', sshAlias }));
});

test('selection persists and requests execute on selected peer with streamed events', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'peer-route-test-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  let invocation;
  const spawnSSH = (program, args) => {
    const child = new EventEmitter();
    child.stdin = new PassThrough(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
    child.kill = () => {};
    invocation = { program, args, prompt: '' };
    child.stdin.on('data', chunk => { invocation.prompt += chunk; });
    child.stdin.on('finish', () => setImmediate(() => {
      child.stdout.write(JSON.stringify({ type: 'item.completed', item: { id: 'answer', type: 'agent_message', text: 'Hello from the selected peer' } }) + '\n');
      child.stdout.write('{"type":"turn.completed"}\n');
      child.emit('close', 0);
    }));
    return child;
  };
  const app = await createApp({ directory, uiPort: 0, proxyPort: 0, spawnSSH });
  t.after(() => app.close());
  const origin = `http://127.0.0.1:${app.ui.address().port}`;
  const post = (path, body, requestOrigin = origin) => fetch(origin + path, { method: 'POST', headers: { 'Content-Type': 'application/json', Origin: requestOrigin }, body: JSON.stringify(body) });
  assert.equal((await post('/api/peers', { name: 'Blocked', sshAlias: 'blocked' }, 'https://evil.test')).status, 403);
  assert.equal((await post('/api/chat', { prompt: 'hello' })).status, 400);
  for (const [name, sshAlias] of [['Ubuntu', 'ubuntu-home'], ['Mac', 'mac-home']]) assert.equal((await post('/api/peers', { name, sshAlias })).status, 200);
  const state = await (await fetch(origin + '/api/state')).json();
  const mac = state.peers[1];
  await post('/api/select', { id: mac.id });
  const result = await post('/api/chat', { prompt: 'a prompt with $(shell) and `backticks`' });
  const events = (await result.text()).trim().split('\n').map(JSON.parse);
  assert.equal(invocation.program, 'ssh');
  assert.equal(invocation.args.at(-2), 'mac-home');
  assert.match(invocation.args.at(-1), /--sandbox read-only/);
  assert.ok(!invocation.args.join(' ').includes('backticks'));
  assert.equal(invocation.prompt, 'a prompt with $(shell) and `backticks`');
  assert.equal(events[0].name, 'Mac');
  assert.equal(events[1].item.text, 'Hello from the selected peer');
  assert.equal(events.at(-1).type, 'relay.done');
  const restored = await createApp({ directory, uiPort: 0, proxyPort: 0, spawnSSH });
  t.after(() => restored.close());
  const persisted = await (await fetch(`http://127.0.0.1:${restored.ui.address().port}/api/state`)).json();
  assert.equal(persisted.selected, mac.id);
  await post('/api/remove', { id: mac.id });
  assert.equal((await (await fetch(origin + '/api/state')).json()).selected, 'direct');
});

test('peer authentication failures are streamed as explicit errors', async t => {
  const directory = await mkdtemp(join(tmpdir(), 'peer-route-error-'));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const spawnSSH = () => {
    const child = new EventEmitter();
    child.stdin = new PassThrough(); child.stdout = new PassThrough(); child.stderr = new PassThrough(); child.kill = () => {};
    child.stdin.on('finish', () => setImmediate(() => { child.stderr.write('Permission denied (publickey)'); child.emit('close', 255); }));
    return child;
  };
  const app = await createApp({ directory, uiPort: 0, proxyPort: 0, spawnSSH }); t.after(() => app.close());
  const origin = `http://127.0.0.1:${app.ui.address().port}`;
  const post = (path, body) => fetch(origin + path, { method: 'POST', headers: { Origin: origin, 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
  await post('/api/peers', { name: 'Ubuntu', sshAlias: 'ubuntu-home' });
  const state = await (await fetch(origin + '/api/state')).json(); await post('/api/select', { id: state.peers[0].id });
  const result = await post('/api/chat', { prompt: 'hello' });
  assert.match(await result.text(), /Permission denied/);
});
