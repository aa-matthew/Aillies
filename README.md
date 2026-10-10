# Peer Route

The proposed cross-platform tray-first architecture, public encrypted tunnel, minimal UX and staged upgrade from the current Swift build are documented in [Peer Route 0.9 migration](docs/NEXT-VERSION-MIGRATION.md).

The native macOS app is in [`macos/`](macos/README.md). Version 0.8.3 adds a paired **Codex OAuth turn executor**: choose B’s `auth.json`, AirDrop its pairing, connect A, then test text/image streaming. OAuth stays on B. A minimal main screen provides sharing and machine selection; diagnostics are under Advanced. The Responses endpoint can be configured in Codex after a generation check.

**The official ChatGPT desktop UI adapter remains unfinished.** Its network relay still uses A’s login. Local tests use a mock inference upstream; live account entitlement, quota and official-app compatibility need a two-Mac test. See [macOS setup and limitations](macos/README.md).

The browser prototype below is retained as an earlier experiment.

A local browser app for choosing a Mac or Ubuntu peer and sending a fresh Codex request through that peer's saved CLI login. Node.js 20+ and OpenSSH are required. No npm dependencies.

## Start

```sh
npm start
```

Open http://127.0.0.1:4780. Settings are saved locally in `data/settings.json`. The app binds only to loopback.

## Prepare each peer

1. Install Codex CLI, then run `codex login` using your ChatGPT account on that machine. The CLI needs its own usable saved authentication; a ChatGPT desktop session alone is insufficient. Use a recent CLI that supports `--ignore-user-config`.
2. Enable Remote Login in macOS System Settings, or install/enable OpenSSH Server on Ubuntu.
3. Configure an SSH alias on the machine running Peer Route:

```sshconfig
Host ubuntu-home
  HostName 192.168.1.25
  User your-user
  IdentityFile ~/.ssh/id_ed25519
  IdentitiesOnly yes
```

4. Run `ssh ubuntu-home` yourself to verify the host fingerprint and set up key-based access. Peer Route uses strict host checking and noninteractive SSH; it never accepts unknown keys automatically or handles passwords.
5. Verify `ssh ubuntu-home "bash -lc 'codex --version'"` works. If Codex is installed through a version manager, make it available in the peer's Bash login environment.
6. Add a friendly name and the SSH alias in the app. Select that machine and submit a request. Repeat with a Mac alias to switch between peers.

Use a private VPN or reachable LAN for peers. You can run this app on either OS. The receiving peer needs SSH and Codex, not this web server.

## Behavior and limits

- Requests execute `codex exec` on the selected peer, using its saved CLI authentication. Credentials never need to be copied to the requesting machine. Configure the peer's CLI login as ChatGPT auth if you want its plan usage. Desktop-app cookies are not used.
- Each request is a fresh, ephemeral session. There is no conversation history, attachment support, or token-by-token UI guarantee; Codex JSON events are streamed and agent messages display as they arrive.
- Codex runs read-only, with user config ignored, from the SSH login directory. It may read files available to that account and use built-in tools under the read-only sandbox. This is a Codex agent, not a tool-free chat completion. Use a dedicated OS account when isolation is needed.
- A five-minute timeout or closing the browser disconnects the SSH request. Remote process termination after an SSH disconnect depends on the peer; do not assume instant billing cancellation.
- This prototype does **not** intercept the existing ChatGPT/Codex desktop apps or replace their accounts. It implements peer account execution through its own request screen. Native-app interception and generic ChatGPT session reuse remain unimplemented.
- Real two-machine execution must be verified with your SSH aliases and peer authentication. Automated tests use a simulated SSH/Codex process and make no paid model calls.

## Optional traffic-only proxy

`http://127.0.0.1:4781` accepts HTTPS CONNECT for `openai.com`, `chatgpt.com`, `oaistatic.com`, `oaiusercontent.com` and their subdomains on port 443. The selected SSH peer forwards the encrypted connection using `ssh -W`. With no peer selected it connects directly. This keeps the **requesting client's account**, so it does not accomplish account switching by itself. It does not install certificates, decrypt TLS, change system settings, or guarantee native app proxy compatibility. Other destinations, plaintext HTTP and UDP/QUIC are unsupported. Existing connections retain their original route when selection changes.

## Verify

```sh
npm test
```

Official references: [non-interactive Codex and saved authentication](https://learn.chatgpt.com/docs/non-interactive-mode), [remote desktop connections](https://learn.chatgpt.com/docs/remote-connections).
