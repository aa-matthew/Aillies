import http from 'node:http';
import net from 'node:net';
import { spawn } from 'node:child_process';
import { readFile, mkdir, writeFile, rename } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';
import { randomUUID } from 'node:crypto';

const root = dirname(fileURLToPath(import.meta.url));
const defaults = () => ({ peers: [], selected: 'direct' });
export function destination(authority) {
  const match = /^([a-z0-9.-]+):443$/i.exec(authority || '');
  if (!match) throw new Error('Only HTTPS destinations on port 443 are supported');
  const host = match[1].toLowerCase();
  const allowed = ['openai.com', 'chatgpt.com', 'oaistatic.com', 'oaiusercontent.com'];
  if (!allowed.some(domain => host === domain || host.endsWith(`.${domain}`))) {
    throw new Error('Destination is outside the OpenAI/ChatGPT allowlist');
  }
  return host;
}
export function validatePeer(input) {
  if (typeof input.name !== 'string' || !input.name.trim() || input.name.length > 80) throw new Error('Enter a machine name');
  if (typeof input.sshAlias !== 'string' || !/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,99}$/.test(input.sshAlias)) throw new Error('Use an SSH config alias, such as ubuntu-home');
  return { id: randomUUID(), name: input.name.trim(), sshAlias: input.sshAlias };
}
export async function createApp({ directory = join(root, 'data'), uiPort = 4780, proxyPort = 4781, connect = net.connect, spawnSSH = spawn } = {}) {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const statePath = join(directory, 'settings.json');
  let state;
  try { state = JSON.parse(await readFile(statePath, 'utf8')); }
  catch (error) { if (error.code !== 'ENOENT') throw error; state = defaults(); }
  if (!Array.isArray(state.peers) || !state.peers.every(p => p.id && validatePeer(p))) throw new Error('Invalid saved settings');
  if (state.selected !== 'direct' && !state.peers.some(p => p.id === state.selected)) throw new Error('Selected machine is missing');
  let saving = Promise.resolve();
  const save = () => {
    const snapshot = JSON.stringify(state, null, 2);
    saving = saving.then(async () => {
      const temp = statePath + '.tmp';
      await writeFile(temp, snapshot, { mode: 0o600 });
      await rename(temp, statePath);
    });
    return saving;
  };
  let active = 0;
  let lastError = '';
  const sockets = new Set();
  const children = new Set();
  const track = socket => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); };
  const proxy = http.createServer((req, res) => { res.writeHead(405); res.end('Use HTTPS CONNECT'); });
  proxy.on('connection', track);
  proxy.on('connect', (req, client, head) => {
    let host;
    try { host = destination(req.url); }
    catch (error) { client.end('HTTP/1.1 403 Forbidden\r\n\r\n'); lastError = error.message; return; }
    const peer = state.peers.find(p => p.id === state.selected);
    let input, output, child, socket;
    let ready = false, finished = false;
    active++;
    const timer = setTimeout(() => fail('Connection timed out'), 15000);
    const cleanup = () => {
      if (finished) return;
      finished = true; clearTimeout(timer); active--;
      socket?.destroy(); child?.kill(); client.destroy();
    };
    const fail = message => {
      lastError = message;
      if (!ready && !client.destroyed) client.write('HTTP/1.1 502 Bad Gateway\r\n\r\n');
      cleanup();
    };
    const established = () => {
      if (finished || ready) return;
      ready = true; clearTimeout(timer);
      client.write('HTTP/1.1 200 Connection Established\r\n\r\n');
      if (head.length) input.write(head);
      client.pipe(input); output.pipe(client);
    };
    client.on('error', cleanup); client.on('close', cleanup);
    if (peer) {
      // Arguments never pass through a shell. SSH authenticates the peer and
      // encrypts the transport; the original client retains end-to-end TLS.
      child = spawnSSH('ssh', ['-T', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', '-o', 'ConnectTimeout=10', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=2', '-W', `${host}:443`, peer.sshAlias], { stdio: ['pipe', 'pipe', 'pipe'] });
      children.add(child); child.once('close', () => children.delete(child));
      input = child.stdin; output = child.stdout;
      let stderr = '';
      child.stderr.on('data', chunk => { stderr = (stderr + chunk.toString()).slice(-1000); });
      child.on('error', () => fail('Could not start SSH'));
      child.on('close', code => { if (code) lastError = `SSH failed for ${peer.name}: ${stderr.trim() || code}`; cleanup(); });
      input.on('error', () => fail('SSH stream closed'));
      output.on('error', () => fail('SSH stream closed'));
      // CONNECT must be acknowledged before TLS starts; SSH errors after this
      // point close the tunnel and are shown on the dashboard.
      child.once('spawn', established);
    } else {
      socket = connect({ host, port: 443 }); track(socket);
      input = output = socket;
      socket.once('connect', established);
      socket.on('error', () => fail('Upstream connection failed'));
      socket.on('close', cleanup);
    }
  });
  const ui = http.createServer(async (req, res) => {
    const reply = (code, value) => { res.writeHead(code, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' }); res.end(JSON.stringify(value)); };
    const expectedHost = `127.0.0.1:${ui.address().port}`;
    if (req.headers.host !== expectedHost) return reply(403, { error: 'Use the loopback dashboard address' });
    res.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'self'; style-src 'self'; frame-ancestors 'none'; base-uri 'none'");
    res.setHeader('X-Content-Type-Options', 'nosniff');
    try {
      const url = new URL(req.url, `http://${expectedHost}`);
      if (req.method === 'GET' && url.pathname === '/api/state') return reply(200, { ...state, active, lastError, proxyPort: proxy.address().port });
      if (req.method === 'POST') {
        if (req.headers.origin !== `http://${expectedHost}` || req.headers['content-type'] !== 'application/json') return reply(403, { error: 'Untrusted request' });
        let size = 0;
        const chunks = [];
        for await (const chunk of req) { size += chunk.length; if (size > 32768) return reply(413, { error: 'Request too large' }); chunks.push(chunk); }
        const value = JSON.parse(Buffer.concat(chunks).toString('utf8'));
        if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Expected a JSON object');
        if (url.pathname === '/api/chat') {
          const peer = state.peers.find(p => p.id === state.selected);
          if (!peer) throw new Error('Choose a peer machine before sending a request');
          if (typeof value.prompt !== 'string' || !value.prompt.trim() || value.prompt.length > 6000) throw new Error('Enter a prompt of up to 6,000 characters');
          if (children.size >= 8) return reply(429, { error: 'Too many active connections' });
          const command = "bash -lc 'exec codex exec --ignore-user-config --json --ephemeral --sandbox read-only --skip-git-repo-check -'";
          const child = spawnSSH('ssh', ['-T', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', '-o', 'ConnectTimeout=10', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=2', peer.sshAlias, command], { stdio: ['pipe', 'pipe', 'pipe'] });
          children.add(child);
          res.writeHead(200, { 'Content-Type': 'application/x-ndjson', 'Cache-Control': 'no-store' });
          const emit = event => { if (!res.destroyed) res.write(JSON.stringify(event) + '\n'); };
          emit({ type: 'peer.selected', name: peer.name });
          let pending = '', diagnostic = '', ended = false;
          child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
          const timer = setTimeout(() => { emit({ type: 'relay.error', message: 'Request timed out after five minutes' }); child.kill(); res.end(); }, 300000);
          const stop = () => { clearTimeout(timer); child.kill(); };
          res.once('close', stop);
          child.stdout.on('data', chunk => {
            pending += chunk.toString();
            if (pending.length > 1048576) { emit({ type: 'relay.error', message: 'Peer sent an oversized event' }); stop(); res.end(); return; }
            let index;
            while ((index = pending.indexOf('\n')) >= 0) {
              const line = pending.slice(0, index); pending = pending.slice(index + 1);
              try { emit(JSON.parse(line)); } catch { /* Ignore shell startup banners. */ }
            }
          });
          child.stderr.on('data', chunk => { diagnostic = (diagnostic + chunk.toString()).slice(-2000); });
          child.stdin.on('error', () => {});
          child.once('error', error => { emit({ type: 'relay.error', message: error.message }); res.end(); });
          child.once('close', code => {
            children.delete(child); clearTimeout(timer);
            if (!ended) {
              ended = true;
              if (code) emit({ type: 'relay.error', message: diagnostic.trim() || `SSH/Codex exited with code ${code}` });
              emit({ type: 'relay.done', code }); res.end();
            }
          });
          child.stdin.end(value.prompt);
          return;
        } else if (url.pathname === '/api/peers') {
          if (state.peers.length >= 20) throw new Error('Maximum 20 peers');
          state.peers.push(validatePeer(value));
        } else if (url.pathname === '/api/select') {
          if (value.id !== 'direct' && !state.peers.some(p => p.id === value.id)) throw new Error('Unknown machine');
          state.selected = value.id;
        } else if (url.pathname === '/api/remove') {
          state.peers = state.peers.filter(p => p.id !== value.id);
          if (state.selected === value.id) state.selected = 'direct';
        } else return reply(404, { error: 'Not found' });
        await save(); return reply(200, { ok: true });
      }
      const assets = { '/': ['index.html', 'text/html'], '/app.js': ['app.js', 'text/javascript'], '/style.css': ['style.css', 'text/css'] };
      const asset = assets[url.pathname];
      if (req.method !== 'GET' || !asset) return reply(404, { error: 'Not found' });
      res.writeHead(200, { 'Content-Type': asset[1] }); res.end(await readFile(join(root, 'public', asset[0])));
    } catch (error) { reply(400, { error: error.message }); }
  });
  ui.on('connection', track);
  const listen = (server, port) => new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', resolve); });
  try { await listen(proxy, proxyPort); await listen(ui, uiPort); }
  catch (error) { proxy.close(); ui.close(); throw error; }
  return { ui, proxy, close: async () => { for (const child of children) child.kill(); for (const socket of sockets) socket.destroy(); await Promise.all([new Promise(r => ui.close(r)), new Promise(r => proxy.close(r))]); } };
}
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const app = await createApp();
  console.log('Peer Route: http://127.0.0.1:4780\nHTTPS proxy: http://127.0.0.1:4781');
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, async () => { await app.close(); process.exit(0); });
}
