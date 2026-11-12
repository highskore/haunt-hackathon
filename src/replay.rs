//! Runs a trace through haunt-core, the same core the Haunt app runs, and records what it did.
//!
//! Every step calls the core's public API: `register_surface`, `replay_line` (the intake's decoding
//! and nonce check), `on_user_input`, `select_surface`, `answer_permission` and
//! `gate_plugin_request`. Time is pinned to the trace's with `set_clock`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use haunt_core::event::visible;
use haunt_core::plugins::HostRequest;
use haunt_core::ptygate::untypable;
use haunt_core::{Core, Outcome, Row, RowKind, SidebarSnapshot, SurfaceEnv};
use serde_json::{Map, Value};

use crate::out::{Out, Style};
use crate::trace::{Action, Header, Key, Nonce, Step};

struct Replay {
    core: Core,
    tabs: HashMap<String, SurfaceEnv>,
    /// Surface id to tab name, for rows and decisions.
    names: HashMap<String, String>,
    out: Out,
    /// The sidebar as last printed, to print it only when it changes.
    sidebar: Out,
}

pub fn replay(header: &Header, steps: &[Step]) -> Out {
    // Never started: no socket, no ctl, no decision log on disk.
    let core = Core::new(std::env::temp_dir().join("haunt-replay-unused"));
    core.set_home_and_shell(None, None);
    let mut r = Replay { core, tabs: HashMap::new(), names: HashMap::new(), out: Out::default(), sidebar: Out::default() };
    if let Some(t) = &header.title {
        r.out.line(Style::Bold, &visible(t));
    }
    if let Some(s) = &header.source {
        r.out.line(Style::Plain, &format!("source: {}", visible(s)));
    }
    for n in &header.notes {
        r.out.line(Style::Plain, &format!("note: {}", visible(n)));
    }
    let intro = format!("{} steps through haunt-core: its state, hook decoding, nonce check and answer gate. Time is the trace's.", steps.len());
    r.out.line(Style::Dim, &intro);
    let start = Instant::now();
    for s in steps {
        r.core.set_clock(Some(start + Duration::from_millis(s.t)));
        r.step(s);
    }
    r.decisions();
    r.out
}

impl Replay {
    fn step(&mut self, s: &Step) {
        let tab = visible(&s.tab);
        if let Some(n) = &s.note {
            self.out.line(Style::Dim, &format!("# {}", visible(n)));
        }
        self.out.push(Style::Dim, &clock(s.t));
        self.out.push(Style::Plain, &format!(" {tab:<6} "));
        if s.scripted {
            self.out.push(Style::Dim, "(scripted) ");
        }
        let env = self.tabs.get(&s.tab).cloned();
        match (&s.action, env) {
            (Action::Open { cwd }, None) => match self.core.register_surface(cwd) {
                Ok(env) => {
                    self.names.insert(env.surface_id.clone(), tab.clone());
                    self.tabs.insert(s.tab.clone(), env);
                    self.out.push(Style::Plain, &format!("open tab at {}", visible(cwd)));
                }
                Err(e) => self.out.push(Style::Bad, &format!("open refused: {}", visible(&e.to_string()))),
            },
            (Action::Open { .. }, Some(_)) => self.out.push(Style::Bad, "open: the tab is already open"),
            (_, None) => self.out.push(Style::Bad, "no such tab (open it first)"),
            (Action::Hook { body, nonce }, Some(env)) => self.hook(&env, body, nonce),
            (Action::Select, Some(env)) => {
                self.core.select_surface(&env.surface_id);
                self.out.push(Style::Plain, "select tab");
            }
            (Action::Typed, Some(env)) => {
                self.core.on_user_input(&env.surface_id);
                self.out.push(Style::Plain, "the user types at the terminal");
            }
            (Action::Answer { allow, via, key }, Some(env)) => {
                let key = self.key(&env, key);
                let k = key.unwrap_or(0);
                self.out.push(Style::Plain, &format!("answer {} via {}, request {} -> ", verdict(*allow), visible(via), key_text(key)));
                // Asked first, then answered: one thread, so nothing changes in between.
                let why = self.core.answer_refusal(&env.surface_id, k, *allow);
                match self.core.answer_permission(&env.surface_id, k, *allow) {
                    Some(bytes) => self.allowed(&bytes),
                    None => self.refused(why.map_or("refused", |w| w.text())),
                }
            }
            (Action::PluginAnswer { plugin, allow, key }, Some(env)) => {
                let key = self.key(&env, key);
                self.out.push(Style::Plain, &format!("plugin {} answers {}, request {} -> ", visible(plugin), verdict(*allow), key_text(key)));
                let req = HostRequest::PtyAnswer { plugin_id: plugin.clone(), surface: env.surface_id.clone(), allow: *allow, request_id: key.unwrap_or(0) };
                match self.core.gate_plugin_request(&req) {
                    Some(w) => self.allowed(&w.bytes),
                    None => {
                        let why = self.core.take_plugin_refusals().into_iter().rev().find(|r| r.plugin_id == *plugin);
                        self.refused(why.map_or("refused", |r| r.reason.text()));
                    }
                }
            }
            (Action::PluginType { plugin, text, enter }, Some(env)) => {
                let what = format!("plugin {} types \"{}\"{} -> ", visible(plugin), visible(text), if *enter { " + Return" } else { "" });
                self.out.push(Style::Plain, &what);
                let req = HostRequest::PtyType { plugin_id: plugin.clone(), surface: env.surface_id.clone(), text: text.clone(), enter: *enter };
                match self.core.gate_plugin_request(&req) {
                    Some(w) => self.allowed(&w.bytes),
                    // The gate gives no reason for typing: these are the two conditions it checks.
                    None => match untypable(text) {
                        Some(why) => self.refused(&why),
                        None => self.refused("not the selected tab, or a prompt is up or may still be on screen"),
                    },
                }
            }
            (Action::Close, Some(env)) => {
                self.core.unregister_surface(&env.surface_id);
                self.tabs.remove(&s.tab);
                self.out.push(Style::Plain, "close tab");
            }
        }
        self.out.push(Style::Plain, "\n");
        self.show_sidebar();
    }

    fn allowed(&mut self, bytes: &[u8]) {
        self.out.push(Style::Good, &format!("ALLOWED by the gate: types {}", keys(bytes)));
    }

    fn refused(&mut self, why: &str) {
        self.out.push(Style::Bad, &format!("REFUSED: {}", visible(why)));
    }

    fn hook(&mut self, env: &SurfaceEnv, body: &Map<String, Value>, nonce: &Nonce) {
        let mut line = body.clone();
        line.insert("surface_id".into(), env.surface_id.clone().into());
        line.remove("nonce");
        let how = match nonce {
            Nonce::Tab => {
                line.insert("nonce".into(), env.nonce.clone().into());
                "nonce"
            }
            Nonce::Missing => "no nonce",
            Nonce::Wrong(n) => {
                line.insert("nonce".into(), n.clone().into());
                "wrong nonce"
            }
        };
        self.out.push(Style::Plain, &format!("{}  ", describe(body)));
        match self.core.replay_line(&socket_line(&Value::Object(line))) {
            None => self.out.push(Style::Bad, "dropped: unknown surface"),
            Some(r) => match r.error {
                Some(e) => self.out.push(Style::Bad, &format!("rejected: {}", visible(&e))),
                None if r.verified => self.out.push(Style::Good, "verified"),
                None => self.out.push(Style::Warn, &format!("NOT verified ({how}): status only, can't create a request")),
            },
        }
    }

    /// The request an answer names: `live` is what the tab's row shows now, if anything.
    fn key(&self, env: &SurfaceEnv, key: &Key) -> Option<u64> {
        match key {
            Key::Fixed(k) => Some(*k),
            Key::Live => row_of(&self.core.snapshot(), &env.surface_id).and_then(|r| r.pending.as_ref()).map(|p| p.key),
        }
    }

    /// The sidebar as the app would draw it, printed only when it reads differently.
    fn show_sidebar(&mut self) {
        let snap = self.core.snapshot();
        let mut s = Out::default();
        for g in &snap.groups {
            let label = if g.label.is_empty() { "(shells)".to_owned() } else { visible(&g.label) };
            s.push(Style::Plain, "    ");
            s.line(Style::Bold, &label);
            for r in &g.rows {
                self.row(&mut s, r);
            }
        }
        if s != self.sidebar {
            self.out.extend(&s);
            self.sidebar = s;
        }
    }

    fn row(&self, s: &mut Out, r: &Row) {
        let tab = self.names.get(&r.surface_id).map_or("?", String::as_str);
        let status = if r.status_text.is_empty() { String::new() } else { format!(" {}", visible(&r.status_text)) };
        s.line(Style::Plain, &format!("      {tab} [{}]{status}  {}", kind(r.kind), visible(&r.meta)));
        if let Some(q) = &r.pending {
            s.push(Style::Plain, &format!("        request #{} {}: ", q.key, request(&q.tool, &q.preview)));
            match q.suspicious {
                true => s.line(Style::Warn, "terminal-only, no Allow/Deny"),
                false => s.line(Style::Good, "Allow/Deny offered"),
            }
        }
        if let Some(p) = &r.progress {
            s.line(Style::Plain, &format!("        tasks {}/{}", p.done, p.total));
        }
        for sub in &r.subagents {
            s.line(Style::Plain, &format!("        subagent {} ({})", visible(&sub.label), visible(&sub.meta)));
        }
    }

    fn decisions(&mut self) {
        let mut all = self.core.decisions("", 1000);
        all.reverse();
        self.out.line(Style::Plain, "");
        self.out.line(Style::Bold, "decision log");
        if all.is_empty() {
            self.out.line(Style::Plain, "  (none: no verified request ended)");
        }
        for d in all {
            let tab = self.names.get(&d.surface_id).map_or("?", String::as_str);
            let outcome = match &d.outcome {
                Outcome::AllowedByPlugin { plugin_id } | Outcome::DeniedByPlugin { plugin_id } => {
                    format!("{} ({})", d.outcome.as_str(), visible(plugin_id))
                }
                Outcome::RefusedPlugin { plugin_id, reason } => format!("{} ({}: {})", d.outcome.as_str(), visible(plugin_id), reason.text()),
                o => o.as_str().to_owned(),
            };
            self.out.line(Style::Plain, &format!("  {tab:<6} {}  {outcome}", request(&d.tool, &d.preview)));
        }
    }
}

/// The event as an adapter writes it to the socket: serde_json leaves C1 controls raw, which the
/// core rejects, so they go out as `\u` escapes, as the adapters send them.
fn socket_line(v: &Value) -> String {
    let mut s = String::new();
    for c in v.to_string().chars() {
        match c {
            '\u{80}'..='\u{9f}' => s += &format!("\\u{:04x}", c as u32),
            c => s.push(c),
        }
    }
    s
}

fn row_of<'a>(snap: &'a SidebarSnapshot, surface_id: &str) -> Option<&'a Row> {
    snap.groups.iter().flat_map(|g| &g.rows).find(|r| r.surface_id == surface_id)
}

/// `mm:ss.mmm` since the trace began.
fn clock(t: u64) -> String {
    format!("{:02}:{:02}.{:03}", t / 60_000, t / 1000 % 60, t % 1000)
}

fn verdict(allow: bool) -> &'static str {
    if allow { "Allow" } else { "Deny" }
}

fn key_text(key: Option<u64>) -> String {
    key.map_or_else(|| "(none shown)".into(), |k| format!("#{k}"))
}

fn kind(k: RowKind) -> &'static str {
    match k {
        RowKind::Shell => "shell",
        RowKind::Claude => "claude",
        RowKind::Codex => "codex",
    }
}

/// The answer bytes, named: they go to the terminal as keys.
fn keys(bytes: &[u8]) -> String {
    match bytes {
        b"\x1b" => "Esc".into(),
        b => format!("\"{}\"", visible(&String::from_utf8_lossy(b))),
    }
}

/// A Bash preview is the command alone; the others start with their tool.
fn request(tool: &str, preview: &str) -> String {
    let preview = visible(preview);
    if tool == "Bash" { format!("Bash \"{preview}\"") } else { format!("\"{preview}\"") }
}

/// One event in a few words, every string with nothing hidden.
fn describe(body: &Map<String, Value>) -> String {
    let s = |k: &str| body.get(k).and_then(Value::as_str).map(visible);
    let event = s("event").unwrap_or_else(|| "?".into());
    let agent = s("agent").map(|a| format!("{a} ")).unwrap_or_default();
    let input = body.get("tool_input").and_then(|i| i.get("command").or_else(|| i.get("file_path"))).and_then(Value::as_str).map(visible);
    let detail = match event.as_str() {
        "prompt_submit" => s("query").map(|q| format!("\"{}\"", cut(&q, 60))),
        "task_created" => s("task_subject").map(|t| format!("\"{t}\"")),
        "task_updated" => Some(format!("{} {}", s("task_id").unwrap_or_else(|| "?".into()), s("task_status").unwrap_or_else(|| "?".into()))),
        "subagent_spawn" | "subagent_start" | "subagent_stop" | "subagent_result" => {
            Some([s("agent_type"), s("agent_id"), s("description")].into_iter().flatten().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" "))
        }
        "notification" => s("notification_type"),
        _ => s("tool_name").map(|t| match &input {
            Some(i) => format!("{t} \"{i}\""),
            None => t,
        }),
    };
    let sub = s("agent_id").filter(|_| !event.starts_with("subagent")).map(|a| format!(" [subagent {a}]")).unwrap_or_default();
    format!("{agent}{event}{}{sub}", detail.map(|d| format!(" {d}")).unwrap_or_default())
}

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_owned() } else { s.chars().take(max - 1).collect::<String>() + "…" }
}
