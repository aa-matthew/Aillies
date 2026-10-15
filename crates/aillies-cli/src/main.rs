//! `aillies` — run the node headless and control it from a terminal.

use aillies_node::node::{default_home, NodeConfig};
use aillies_node::store::Store;
use aillies_node::Node;
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::io::Write;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "aillies", version, about = "Use a trusted device's Codex account from this device.")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run Aillies in the foreground (keep this open, or use the desktop app).
    Run,
    /// Show what this device is doing.
    Status,
    /// Save the latest 100 request errors and warnings to Downloads.
    Diagnostics,
    /// Share this device's Codex login with approved devices.
    Share {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Use Fast mode on new requests to the selected device.
    Fast {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Allow approved devices to request Fast mode on this account.
    AllowFast {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Create a one-time pairing code here, then approve the device that uses it.
    Invite {
        /// Full code for private networks or older devices.
        #[arg(long)]
        full: bool,
    },
    /// Pair with a device using the code it showed.
    Pair { code: String },
    /// List pairing requests waiting for approval.
    Pending,
    /// Approve a pairing request (the only one if no id is given).
    Approve { id: Option<String> },
    /// Decline a pairing request.
    Reject { id: Option<String> },
    /// Choose where new Codex requests go: a device name/id, or "local".
    Use { target: String },
    /// Stop sending requests anywhere.
    Pause,
    /// Resume sending requests.
    Resume,
    /// List paired and approved devices.
    Devices,
    /// Check connection and account readiness of a paired device.
    Check { device: Option<String> },
    /// Stop allowing a device to use this device's account.
    Revoke { device: String },
    /// Forget a device you send through.
    Forget { device: String },
    /// Turn the Codex integration on or off.
    Codex {
        #[arg(value_parser = ["on", "off"])]
        state: String,
        /// Model to set in Codex (keeps your current model if omitted).
        #[arg(long)]
        model: Option<String>,
        /// Preserve and switch existing chats too (quit Codex first).
        #[arg(long, conflicts_with = "model")]
        existing_chats: bool,
    },
    /// Configure Claude Code. Its local history stays intact; restart/resume manually.
    Claude {
        #[command(subcommand)]
        command: ClaudeCmd,
    },
    /// Rename this device.
    Name { name: String },
    /// Clear the last error.
    Dismiss,
}

#[derive(Subcommand)]
enum ClaudeCmd {
    On {
        /// Route desktop Chat instead of Claude Code. Quit Claude desktop first.
        #[arg(long)]
        desktop: bool,
    },
    Off {
        #[arg(long)]
        desktop: bool,
    },
    Use {
        target: String,
    },
    Share {
        #[arg(value_parser = ["on", "off"])]
        state: String,
        /// Choose the connected Aillies account or an explicit legacy native source.
        #[arg(long, value_parser = ["connected", "desktop", "code"])]
        source: Option<String>,
    },
    Grant {
        fingerprint: String,
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
}

struct Client {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl Client {
    fn new() -> Result<Self> {
        let s = Store::read_secrets(&default_home())?;
        Ok(Self { base: format!("http://127.0.0.1:{}/control", s.loopback_port), token: s.control_token, http: reqwest::Client::new() })
    }
    async fn call(&self, method: &str, path: &str, body: Value) -> Result<Value> {
        let url = format!("{}/{}", self.base, path);
        let req = if method == "GET" { self.http.get(url) } else { self.http.post(url).json(&body) };
        let resp = req
            .bearer_auth(&self.token)
            .timeout(Duration::from_secs(330))
            .send()
            .await
            .map_err(|_| anyhow!("Aillies is not running. Start it with `aillies run` (or open the Aillies app)."))?;
        let ok = resp.status().is_success();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if !ok {
            bail!("{}", v.get("error").and_then(Value::as_str).unwrap_or("Request failed."));
        }
        Ok(v)
    }
}

fn ask(q: &str) -> bool {
    print!("{q} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    matches!(s.trim().to_lowercase().as_str(), "y" | "yes")
}

fn dot(state: &str) -> &'static str {
    match state {
        "ready" => "● Connected",
        "local" => "○ This device",
        "paused" => "○ Paused",
        _ => "▲ Action required",
    }
}

fn pending_id(st: &Value, id: Option<String>) -> Result<String> {
    if let Some(id) = id {
        return Ok(id);
    }
    let p = st["pending"].as_array().cloned().unwrap_or_default();
    match p.as_slice() {
        [one] => Ok(one["id"].as_str().unwrap_or_default().to_string()),
        [] => bail!("No pairing requests are waiting."),
        _ => bail!("Several requests are waiting; pass an id from `aillies pending`."),
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run(Cli::parse()).await {
        eprintln!("aillies: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    if let Cmd::Run = cli.cmd {
        tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).init();
        let node = Node::start(NodeConfig::from_env()).await?;
        let st = node.status();
        println!("Aillies {} · {} ({})", st.version, st.name, st.short_id);
        println!("Codex endpoint  http://127.0.0.1:{}/v1   (run `aillies codex on` once)", st.loopback_port);
        println!("Sending through {}", st.destination_name);
        println!("Sharing         {}", if st.sharing { format!("on, port {}", st.peer_port.unwrap_or(0)) } else { "off".into() });
        println!("Press Ctrl-C to stop. Stopping blocks Codex requests until Aillies runs again.");
        tokio::signal::ctrl_c().await?;
        node.shutdown();
        return Ok(());
    }

    let c = Client::new()?;
    match cli.cmd {
        Cmd::Run => unreachable!(),
        Cmd::Diagnostics => {
            let result = c.call("POST", "diagnostics/export", json!({})).await?;
            println!("Saved: {}", result["path"].as_str().unwrap_or(""));
        }
        Cmd::Status => {
            let s = c.call("GET", "status", json!({})).await?;
            println!("{}  {}", dot(s["state"].as_str().unwrap_or("")), s["name"].as_str().unwrap_or(""));
            println!("  Send through    {}", s["destination_name"].as_str().unwrap_or(""));
            println!("  Routing         {}", if s["routing"].as_bool() == Some(true) { "on" } else { "paused" });
            let share =
                if s["sharing"].as_bool() == Some(true) { format!("on · {} approved", s["clients"].as_array().map_or(0, Vec::len)) } else { "off".into() };
            println!("  Share this device {share}");
            println!("  Use Fast mode   {}", if s["fast_mode"].as_bool() == Some(true) { "on" } else { "off" });
            println!("  Allow Fast mode {}", if s["allow_fast_mode"].as_bool() == Some(true) { "on" } else { "off" });
            println!("  Codex           {}", if s["codex_installed"].as_bool() == Some(true) { "using Aillies" } else { "not set up (`aillies codex on`)" });
            println!(
                "  Claude Code     {} · {}",
                if s["claude"]["installed"] == true { "using Aillies" } else { "Normal" },
                s["claude"]["destination_name"].as_str().unwrap_or("")
            );
            let acct = if s["local_account"]["state"] == "ready" {
                "signed in".to_string()
            } else {
                format!("not ready ({})", s["local_account"]["code"].as_str().unwrap_or(""))
            };
            println!("  This device's Codex login  {acct}");
            if let Some(f) = s["last_failure"].as_object() {
                println!("\n  ▲ {}", f["message"].as_str().unwrap_or(""));
            }
            for p in s["pending"].as_array().cloned().unwrap_or_default() {
                println!("\n  {} wants to pair · code {} · run `aillies approve`", p["name"].as_str().unwrap_or(""), p["code"].as_str().unwrap_or(""));
            }
        }
        Cmd::Share { state } => {
            c.call("POST", "sharing", json!({"on": state == "on"})).await?;
            println!("Sharing {state}.");
        }
        Cmd::Fast { state } => {
            c.call("POST", "fast-mode", json!({"on":state == "on"})).await?;
            println!("Use Fast mode {state} for new requests.");
        }
        Cmd::AllowFast { state } => {
            c.call("POST", "allow-fast-mode", json!({"on":state == "on"})).await?;
            println!("Allow Fast mode {state}.");
        }
        Cmd::Invite { full } => {
            let v = c.call("POST", if full { "invite" } else { "pairing-code" }, json!({})).await?;
            println!("On the other device run:\n\n  aillies pair {}\n", v["code"].as_str().unwrap_or(""));
            if !full {
                println!("Keep both devices on the same local network. Use --full for private networks.");
            }
            println!("Send it privately. It works once and expires in 10 minutes. Waiting for the device…");
            let deadline = std::time::Instant::now() + Duration::from_secs(600);
            loop {
                let st = c.call("GET", "status", json!({})).await?;
                if let Some(p) = st["pending"].as_array().and_then(|a| a.first()).cloned() {
                    println!(
                        "\n{} ({}, {}) wants to use this device's Codex account.",
                        p["name"].as_str().unwrap_or(""),
                        p["platform"].as_str().unwrap_or(""),
                        p["short_id"].as_str().unwrap_or("")
                    );
                    println!("Verification code:  {}", p["code"].as_str().unwrap_or(""));
                    let id = p["id"].as_str().unwrap_or("");
                    if ask("Does the other device show the same code?") {
                        c.call("POST", &format!("approve/{id}"), json!({})).await?;
                        println!("Approved. Revoke any time with `aillies revoke \"{}\"`.", p["name"].as_str().unwrap_or(""));
                    } else {
                        c.call("POST", &format!("reject/{id}"), json!({})).await?;
                        println!("Declined.");
                    }
                    break;
                }
                if std::time::Instant::now() > deadline {
                    bail!("The pairing code expired.");
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        Cmd::Pair { code } => {
            let v = c.call("POST", "pair", json!({"code": code})).await?;
            let name = v["name"].as_str().unwrap_or("the other device");
            println!("Connected to {name} ({}).", v["short_id"].as_str().unwrap_or(""));
            println!("Verification code:  {}", v["code"].as_str().unwrap_or(""));
            let id = v["id"].as_str().unwrap_or("").to_string();
            if !ask(&format!("Does {name} show the same code?")) {
                c.call("POST", &format!("pair/{id}/cancel"), json!({})).await?;
                bail!("Pairing cancelled.");
            }
            println!("Approve on {name} now…");
            let p = c.call("POST", &format!("pair/{id}/confirm"), json!({"wait_secs": 300})).await?;
            println!("Paired. New Codex chats on this device now use {}.", p["name"].as_str().unwrap_or(name));
            let st = c.call("GET", "status", json!({})).await?;
            if st["codex_installed"].as_bool() != Some(true) {
                println!("Next: `aillies codex on`, then restart Codex.");
            }
        }
        Cmd::Pending => {
            let st = c.call("GET", "status", json!({})).await?;
            let p = st["pending"].as_array().cloned().unwrap_or_default();
            if p.is_empty() {
                println!("No pairing requests.");
            }
            for r in p {
                println!(
                    "{}  {} ({})  code {}",
                    r["id"].as_str().unwrap_or(""),
                    r["name"].as_str().unwrap_or(""),
                    r["platform"].as_str().unwrap_or(""),
                    r["code"].as_str().unwrap_or("")
                );
            }
        }
        Cmd::Approve { id } => {
            let st = c.call("GET", "status", json!({})).await?;
            let id = pending_id(&st, id)?;
            c.call("POST", &format!("approve/{id}"), json!({})).await?;
            println!("Approved.");
        }
        Cmd::Reject { id } => {
            let st = c.call("GET", "status", json!({})).await?;
            let id = pending_id(&st, id)?;
            c.call("POST", &format!("reject/{id}"), json!({})).await?;
            println!("Declined.");
        }
        Cmd::Use { target } => {
            c.call("POST", "destination", json!({"target": target})).await?;
            let st = c.call("GET", "status", json!({})).await?;
            println!("New Codex chats use {}. Existing chats keep their original account; start a new chat.", st["destination_name"].as_str().unwrap_or(""));
        }
        Cmd::Pause => {
            c.call("POST", "routing", json!({"on": false})).await?;
            println!("Paused. Codex requests are refused until you resume.");
        }
        Cmd::Resume => {
            c.call("POST", "routing", json!({"on": true})).await?;
            println!("Resumed.");
        }
        Cmd::Devices => {
            let st = c.call("GET", "status", json!({})).await?;
            println!("You can send through:");
            let dest = st["destination"]["id"].as_str().unwrap_or("");
            for p in st["peers"].as_array().cloned().unwrap_or_default() {
                let mark = if p["fp"].as_str() == Some(dest) { "●" } else { " " };
                println!("  {mark} {}  {}  {}", p["name"].as_str().unwrap_or(""), p["short_id"].as_str().unwrap_or(""), p["platform"].as_str().unwrap_or(""));
            }
            println!("Allowed to use this device's account:");
            for d in st["clients"].as_array().cloned().unwrap_or_default() {
                println!("    {}  {}  {}", d["name"].as_str().unwrap_or(""), d["short_id"].as_str().unwrap_or(""), d["platform"].as_str().unwrap_or(""));
            }
        }
        Cmd::Check { device } => {
            let st = c.call("GET", "status", json!({})).await?;
            let fp = match device {
                Some(d) => st["peers"]
                    .as_array()
                    .and_then(|ps| {
                        ps.iter().find(|p| {
                            p["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(&d)) || p["fp"].as_str().is_some_and(|f| f.starts_with(&d.to_lowercase()))
                        })
                    })
                    .and_then(|p| p["fp"].as_str())
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow!("No paired device matches “{d}”."))?,
                None => st["destination"]["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("This device is selected; nothing to check."))?,
            };
            let h = c.call("GET", &format!("peers/{fp}/health"), json!({})).await?;
            if h["reachable"].as_bool() == Some(true) {
                println!("Connection     reachable · {} ms · {}", h["latency_ms"], h["addr"].as_str().unwrap_or(""));
            } else {
                println!("Connection     unreachable");
            }
            match h["status"]["providers"][0]["account"]["state"].as_str() {
                Some("ready") => println!("Codex account  ready"),
                Some(_) => println!("Codex account  not ready ({})", h["status"]["providers"][0]["account"]["code"].as_str().unwrap_or("")),
                None => println!("Codex account  unknown ({})", h["failure"].as_str().unwrap_or("")),
            }
        }
        Cmd::Revoke { device } => {
            c.call("POST", "clients/revoke", json!({"target": device})).await?;
            println!("Revoked. That device can no longer use this account.");
        }
        Cmd::Forget { device } => {
            c.call("POST", "peers/remove", json!({"target": device})).await?;
            println!("Forgotten.");
        }
        Cmd::Codex { state, model, existing_chats } => {
            if existing_chats {
                let report = c.call("POST", "codex/switch", json!({"on":state=="on"})).await?;
                println!("Provider switched. {} compatible chats updated. Reopen Codex to continue.", report["chats"]);
                if report["history_pending"] == true {
                    println!("Older histories were kept unchanged. Use a new chat for Aillies; see Diagnostics → History.");
                }
                return Ok(());
            }
            if state == "on" {
                let v = c.call("POST", "codex/enable", json!({"model": model})).await?;
                println!("Codex now uses Aillies ({}). Your previous settings are backed up.", v["config"].as_str().unwrap_or("config.toml"));
                println!("Quit and reopen Codex, then start a new chat.");
            } else {
                c.call("POST", "codex/restore", json!({})).await?;
                println!("Codex settings restored. Restart Codex.");
            }
        }
        Cmd::Claude { command } => match command {
            ClaudeCmd::On { desktop } | ClaudeCmd::Off { desktop } => {
                let on = matches!(command, ClaudeCmd::On { .. });
                c.call("POST", if desktop { "claude/desktop/switch" } else { "claude/switch" }, json!({"on":on})).await?;
                if desktop {
                    println!("Claude desktop provider changed. Reopen Claude desktop.");
                } else {
                    println!("Claude provider changed. Restart Claude Code, then resume your session from its original folder.");
                }
            }
            ClaudeCmd::Use { target } => {
                c.call("POST", "claude/destination", json!({"target":target})).await?;
                println!("Claude's next turn uses the selected device.");
            }
            ClaudeCmd::Share { state, source } => {
                c.call("POST", "claude/sharing", json!({"on":state=="on","source":source})).await?;
                println!("Claude sharing {state}.");
            }
            ClaudeCmd::Grant { fingerprint, state } => {
                c.call("POST", "claude/grant", json!({"target":fingerprint,"on":state=="on"})).await?;
                println!("Claude permission {state}.");
            }
        },
        Cmd::Name { name } => {
            c.call("POST", "name", json!({"name": name})).await?;
            println!("Renamed.");
        }
        Cmd::Dismiss => {
            c.call("POST", "dismiss", json!({})).await?;
        }
    }
    Ok(())
}
