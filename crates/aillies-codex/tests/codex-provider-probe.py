# Optional installed-Codex regression. Uses an isolated home and an unreachable loopback mock only.
import subprocess, pathlib, os, json, select, sys, time
home = pathlib.Path(sys.argv[1])
mode = sys.argv[2]
if mode == 'create':
    home.mkdir(parents=True, exist_ok=True)
    (home/'config.toml').write_text('model="gpt-5.6-sol"\nmodel_provider="normal"\n[model_providers.normal]\nname="Normal mock"\nbase_url="http://127.0.0.1:19998/v1"\nwire_api="responses"\nrequires_openai_auth=false\nrequest_max_retries=0\nstream_max_retries=0\n')
env = {**os.environ, 'CODEX_HOME': str(home)}
p = subprocess.Popen(['codex','app-server'], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=0)
buf = b''
def call(i, method, params):
    global buf
    p.stdin.write((json.dumps({'id':i,'method':method,'params':params})+'\n').encode())
    end = time.monotonic()+15
    while time.monotonic()<end:
        if b'\n' in buf:
            line,buf=buf.split(b'\n',1)
            data=json.loads(line)
            if data.get('id')==i:
                assert 'error' not in data, 'Synthetic Codex probe failed'
                return data['result']
            continue
        if select.select([p.stdout],[],[],.2)[0]:
            part=os.read(p.stdout.fileno(),65536)
            assert part, 'Codex probe exited early'
            buf+=part
    raise TimeoutError(method)
try:
    call(1,'initialize',{'clientInfo':{'name':'aillies_probe','version':'1.0'}})
    p.stdin.write(b'{"method":"initialized"}\n')
    if mode == 'create':
        tid=call(2,'thread/start',{'modelProvider':'normal','cwd':str(home)})['thread']['id']
        (home/'probe-id').write_text(tid)
        call(3,'turn/start',{'threadId':tid,'input':[{'type':'text','text':'SYNTHETIC_HISTORY_ONLY'}]})
        deadline=time.monotonic()+5
        while time.monotonic()<deadline:
            paths=list((home/'sessions').rglob('*.jsonl'))
            if paths and b'SYNTHETIC_HISTORY_ONLY' in paths[0].read_bytes(): break
            time.sleep(.05)
        else: raise AssertionError('Synthetic history was not persisted')
    else:
        expected=mode
        tid=(home/'probe-id').read_text()
        # Codex rebuilds its index asynchronously after app-server startup.
        # Wait for discovery; do not fabricate or repair the missing index row.
        deadline=time.monotonic()+10
        while True:
            threads=call(4,'thread/list',{'limit':20,'sourceKinds':[]})['data']
            if any(t['id']==tid and t['modelProvider']==expected for t in threads): break
            assert time.monotonic()<deadline, f'Synthetic thread listing mismatch: expected {expected}, listed providers {[t["modelProvider"] for t in threads]}'
            time.sleep(.1)
        result=call(5,'thread/resume',{'threadId':tid})
        assert result['modelProvider']==expected, f"Synthetic provider mismatch: {result['modelProvider']}"
        assert any(b'SYNTHETIC_HISTORY_ONLY' in path.read_bytes() for path in (home/'sessions').rglob('*.jsonl'))
        read=call(6,'thread/read',{'threadId':tid,'includeTurns':True})
        assert 'SYNTHETIC_HISTORY_ONLY' in json.dumps(read)
        print('Stored provider, resumed provider and history verified:', expected)
finally:
    p.stdin.close()
    try: p.wait(timeout=3)
    except subprocess.TimeoutExpired:
        p.terminate(); p.wait(timeout=3)
