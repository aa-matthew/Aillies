# Request diagnostics and Windows LAN access

## Export the latest 100 request errors and warnings

Install the build whose filename includes `request-log`, then open Aillies. Quit the previous app from its tray menu before installing; keep the existing app data and pairing.

1. After a request fails, open **Connection details → Export diagnostics**.
2. Open **Downloads** and send the new `Aillies-diagnostics-….json` file to the person helping you.
3. For a paired request, exporting on both devices gives the requesting and sharing sides. For **This device**, export on that device.

CLI users can run `aillies diagnostics` while the node is running. It calls the same export method.

The log records new failures after this build is installed. Previous failures cannot be reconstructed. It keeps the latest 100 entries across restarts, oldest first. Successful requests and Dismiss do not erase the history. Nothing is uploaded automatically.

Each entry contains UTC Unix time, warning/error level, fixed Aillies failure code, upstream HTTP status when known, route (`local`, `peer`, `hosted`, or the `loopback` boundary), allowlisted model/reasoning/service-tier values, a Lite-format flag, an image-tool flag, and input/tool counts. Unknown strings become `other`. The export includes the public app version, `request-log-v1` build marker, platform, and compile-time upstream client identity. Counts describe the request at that node's entry; a peer may already have normalized the body.

No prompts, replies, images, tool arguments, raw upstream error bodies, tokens, account identifiers, device names, or network addresses enter this log. It cannot recover OpenAI's detailed error text. Codex's own configuration warnings are outside Aillies and are not included.

If export cannot find Downloads, copy this file instead (Win+R, paste the path):

```text
%APPDATA%\anyax\Aillies\config\request-diagnostics.json
```

An `AILLIES_HOME` override changes that location. The separate `desktop.log` still contains startup/lifecycle events. Do not send `auth.json`, `secrets.json`, or identity key files.

`persistence_ok: false` in an export means the current in-memory log could not be saved, or an existing file could not be loaded safely. Export still includes retained in-memory entries. The log is bounded on disk; an invalid or oversized existing log is ignored rather than copied into an export.

## Let another computer connect to Aillies on Windows

This is needed on a device accepting peer connections. **This device** requests use loopback and do not need an inbound LAN firewall rule.

1. On a trusted home/office LAN, open **Settings → Network & internet → Wi-Fi or Ethernet → the connected network**, and set its network profile to **Private**.
2. Open Aillies and turn **Share this device** on.
3. If Windows asks to allow network access, allow Aillies on **Private networks**.
4. If the prompt was dismissed, open **Windows Security → Firewall & network protection → Allow an app through firewall → Change settings**. Enable **Private** for Aillies. If it is absent, use **Allow another app → Browse** and select the installed `Aillies.exe`. Task Manager → Aillies → Open file location can locate it.
5. Run **Check again** on the requesting device.

For an explicit rule limited to the running desktop app and the local subnet, open PowerShell **as Administrator** with Aillies running:

```powershell
$AilliesExe = (Get-Process -Name Aillies -ErrorAction Stop | Select-Object -First 1).Path
if (-not $AilliesExe) { throw 'Cannot find the running Aillies app path.' }
New-NetFirewallRule -DisplayName 'Aillies LAN TCP 18544' -Direction Inbound -Action Allow -Program $AilliesExe -Protocol TCP -LocalPort 18544 -Profile Private -RemoteAddress LocalSubnet
```

The default peer port is **TCP 18544**. Use the configured port if `AILLIES_PEER_PORT` overrides it. **18522 stays bound to 127.0.0.1** and should not be exposed to the LAN. No router port forwarding is needed on the same LAN. Tailscale peers may require a separately scoped rule for their VPN addresses; `LocalSubnet` is intended for the local LAN.

An OpenAI **HTTP 400** or **HTTP 503** means the request reached the upstream HTTP service. An inbound Windows firewall rule does not fix those upstream responses. Aillies now describes 503 as Service Unavailable and never retries the request automatically.
