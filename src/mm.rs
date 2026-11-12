//! Runs the MULTIMAXXING engine beside the core: it gets the events the plugin would, and its
//! decisions print under the step that caused them.
//!
//! The events are built from haunt-core's own parse of each verified hook line, field by field as
//! haunt's api-0.5 plugin host fills them (`agent_event` in core/src/plugins.rs on api-0.5), with
//! `event_details` granted as the plugin's manifest asks. Unverified events never reach it: the
//! plugin ignores them by default.

use haunt_core::event::{AgentEvent, EventKind, visible};
use multimaxxing_engine::model::{dur, plural};
use multimaxxing_engine::{Config, Decision, Engine, Event, Row};

/// The plugin's timer, as its own replay sends it.
const TICK_MS: u64 = 15_000;

/// The kinds the plugin passes on (`AGENT_KINDS` in haunt-multimaxxing's plugin/src/lib.rs).
const AGENT_KINDS: &[&str] = &[
    "prompt_submit",
    "tool_start",
    "tool_done",
    "tool_failed",
    "tool_ran",
    "permission_seen",
    "question_asked",
    "question_answered",
    "task_created",
    "task_updated",
    "subagent_start",
    "subagent_stop",
    "file_edited",
    "stop",
    "stop_failure",
];

#[derive(Default)]
struct Tally {
    pings: u32,
    interrupts: u32,
    held: u32,
    batches: u32,
    dropped: u32,
}

pub struct Mm {
    eng: Engine,
    /// When the last step happened; ticks fill the time up to the next one.
    last: Option<u64>,
    tally: Tally,
}

impl Mm {
    pub fn new() -> Self {
        Self { eng: Engine::new(Config::default()), last: None, tally: Tally::default() }
    }

    /// Timer ticks between the last step and `t`, on the 15 s grid; each with what it decided.
    pub fn ticks_until(&mut self, t: u64) -> Vec<(u64, String)> {
        let mut out = Vec::new();
        if let Some(last) = self.last {
            let mut at = last - last % TICK_MS + TICK_MS;
            while at < t {
                for d in self.eng.tick(at) {
                    out.extend(self.render(&d).map(|l| (at, l)));
                }
                at += TICK_MS;
            }
        }
        self.last = Some(t);
        out
    }

    /// A verified agent event on tab `tab`.
    pub fn event(&mut self, t: u64, tab: &str, e: &AgentEvent) -> Vec<String> {
        match to_event(t, tab, e) {
            Some(e) => self.ingest(&e),
            None => Vec::new(),
        }
    }

    pub fn focus(&mut self, t: u64, tab: &str) -> Vec<String> {
        self.ingest(&Event::new(t, "focus", tab))
    }

    pub fn close(&mut self, t: u64, tab: &str) -> Vec<String> {
        self.ingest(&Event::new(t, "close", tab))
    }

    fn ingest(&mut self, e: &Event) -> Vec<String> {
        let mut out = Vec::new();
        // Coming back to a tab: what the plugin's resume brief would say.
        if let Some(s) = e.surface.as_deref()
            && e.kind == "focus"
            && self.eng.focused() != Some(s)
            && let Some(b) = self.eng.brief(s, e.t)
        {
            out.push(format!("resume   {:<6} {}", visible(s), visible(&b)));
        }
        for d in self.eng.ingest(e, e.t) {
            out.extend(self.render(&d));
        }
        out
    }

    /// One line per decision; every string from the trace or the engine goes through `visible`.
    fn render(&mut self, d: &Decision) -> Option<String> {
        let t = &mut self.tally;
        Some(match d {
            Decision::Notify { surface, reason, .. } => {
                t.pings += 1;
                t.interrupts += 1;
                format!("NOTIFY   {:<6} {}", visible(surface), visible(reason))
            }
            Decision::Hold { surface, reason } => {
                t.pings += 1;
                t.held += 1;
                format!("hold     {:<6} {}", visible(surface), visible(reason))
            }
            Decision::Release { pings, reason, .. } => {
                t.batches += 1;
                let s: Vec<String> = pings.iter().map(|p| visible(&p.surface)).collect();
                format!("RELEASE  {:<6} {}", s.join(","), visible(reason))
            }
            Decision::Drop { surface, reason } => {
                t.dropped += 1;
                format!("drop     {:<6} {}", visible(surface), visible(reason))
            }
            Decision::Row { surface, row } => format!("row      {:<6} {}", visible(surface), row_text(row)),
            Decision::Title(h) => format!("title           {}", h.as_deref().map_or("cleared".into(), |h| format!("\"{}\"", visible(h)))),
            Decision::Fill { gap_ms, items } => {
                let list: Vec<String> = items
                    .iter()
                    .map(|s| format!("{} ({}{})", visible(&s.title), visible(&s.reason), if s.fits { "" } else { ", too long" }))
                    .collect();
                format!("fill     ~{:<5} {}", dur(*gap_ms), list.join("; "))
            }
        })
    }

    /// The closing count, in the engine replay's words.
    pub fn summary(&self) -> String {
        let t = &self.tally;
        format!(
            "{}: {} interrupted at once, {} held; {} at a break or the cap; {} dropped before they reached you",
            plural(t.pings as usize, "ping"),
            t.interrupts,
            t.held,
            plural(t.batches as usize, "release"),
            t.dropped
        )
    }
}

/// The label ("next" in brackets when it's the orange one), the reason line and the bar.
fn row_text(r: &Option<Row>) -> String {
    let Some(r) = r else { return "cleared".into() };
    let mut parts = Vec::new();
    if let Some(l) = &r.label {
        let l = visible(l);
        parts.push(if r.accent { format!("[{l}]") } else { l });
    }
    if let Some(s) = &r.suffix {
        parts.push(format!("\"{}\"", visible(s)));
    }
    if let Some(b) = r.bar {
        parts.push(format!("bar {}%", b / 10));
    }
    parts.join(" ")
}

fn to_event(t: u64, tab: &str, e: &AgentEvent) -> Option<Event> {
    let kind = e.kind.name();
    if !AGENT_KINDS.contains(&kind) {
        return None;
    }
    let call = match &e.kind {
        EventKind::ToolStart { tool } | EventKind::ToolDone { tool } | EventKind::ToolFailed { tool } | EventKind::ToolRan { tool } | EventKind::PermissionSeen { tool } => {
            Some(tool)
        }
        _ => None,
    };
    let tool = match &e.kind {
        EventKind::FileEdited { tool_name, .. } => tool_name.clone(),
        _ => call.map(|c| c.tool_name.clone()),
    };
    let path = match &e.kind {
        EventKind::FileEdited { file_path, .. } => file_path.clone(),
        _ => call.and_then(|c| c.raw_file_path.clone().or_else(|| c.file_path.clone())),
    };
    let (task, text) = match &e.kind {
        EventKind::TaskCreated { task_id, .. } => (Some(task_id.clone()), None),
        EventKind::TaskUpdated { task_id, status } => (Some(task_id.clone()), Some(status.clone())),
        _ => (None, None),
    };
    Some(Event {
        t,
        kind: kind.to_owned(),
        surface: Some(tab.to_owned()),
        tool,
        cmd: call.and_then(|c| c.raw_command.as_deref().or(c.command.as_deref())).map(visible),
        path: path.as_deref().map(visible),
        task,
        text,
    })
}
