//! The trace format: one JSON object per line. See traces/README.md.

use serde_json::{Map, Value};

/// How a hook line carries the nonce.
pub enum Nonce {
    /// The tab's own, as the adapter sends it from the environment.
    Tab,
    /// Left out, as a line forged from outside the session would be.
    Missing,
    /// A guess.
    Wrong(String),
}

/// Which request an answer names.
pub enum Key {
    /// The one the tab's row shows now, as a click on the sidebar would.
    Live,
    /// A fixed number, as an older notification would carry.
    Fixed(u64),
}

pub enum Action {
    Open { cwd: String },
    Hook { body: Map<String, Value>, nonce: Nonce },
    Select,
    Typed,
    Answer { allow: bool, via: String, key: Key },
    PluginAnswer { plugin: String, allow: bool, key: Key },
    PluginType { plugin: String, text: String, enter: bool },
    Close,
}

pub struct Step {
    pub t: u64,
    pub tab: String,
    pub action: Action,
    pub scripted: bool,
    pub note: Option<String>,
}

#[derive(Default)]
pub struct Header {
    pub title: Option<String>,
    pub source: Option<String>,
    pub notes: Vec<String>,
}

pub fn parse(text: &str) -> Result<(Header, Vec<Step>), String> {
    let mut header = Header::default();
    let mut steps = Vec::new();
    let mut last_t = 0;
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).map_err(|e| format!("line {n}: {e}"))?;
        let Value::Object(mut o) = v else { return Err(format!("line {n}: not a JSON object")) };
        if let Some(h) = o.remove("trace") {
            let s = |k: &str| h.get(k).and_then(Value::as_str).map(str::to_owned);
            header.title = s("title");
            header.source = s("source");
            header.notes = h.get("notes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
            continue;
        }
        let step = step(o).map_err(|e| format!("line {n}: {e}"))?;
        if step.t < last_t {
            return Err(format!("line {n}: t goes backwards ({} after {last_t})", step.t));
        }
        last_t = step.t;
        steps.push(step);
    }
    Ok((header, steps))
}

fn step(mut o: Map<String, Value>) -> Result<Step, String> {
    let t = o.remove("t").and_then(|v| v.as_u64()).ok_or("needs t, in milliseconds")?;
    let tab = o.remove("tab").and_then(|v| v.as_str().map(str::to_owned)).ok_or("needs tab")?;
    let scripted = o.remove("scripted").and_then(|v| v.as_bool()).unwrap_or(false);
    let note = o.remove("note").and_then(|v| v.as_str().map(str::to_owned));
    let nonce = match o.remove("nonce") {
        None => Nonce::Tab,
        Some(Value::Bool(false)) => Nonce::Missing,
        Some(Value::String(s)) => Nonce::Wrong(s),
        Some(_) => return Err("nonce is false (left out) or a string (a wrong one)".into()),
    };
    if !matches!(nonce, Nonce::Tab) && !o.contains_key("hook") {
        return Err("nonce applies to hook steps only".into());
    }
    let key = |a: &Value| match a.get("key") {
        None => Ok(Key::Live),
        Some(Value::String(s)) if s == "live" => Ok(Key::Live),
        Some(v) => v.as_u64().map(Key::Fixed).ok_or_else(|| "key is \"live\" or a request number".to_owned()),
    };
    let flag = |a: &Value, k: &str| a.get(k).and_then(Value::as_bool);
    let string = |a: &Value, k: &str| a.get(k).and_then(Value::as_str).map(str::to_owned);
    let mut actions = o.into_iter();
    let (Some((name, a)), None) = (actions.next(), actions.next()) else {
        return Err("needs exactly one action besides t, tab, nonce, scripted and note".into());
    };
    let action = match name.as_str() {
        "open" => Action::Open { cwd: string(&a, "cwd").ok_or("open needs cwd")? },
        "hook" => match a {
            Value::Object(body) => Action::Hook { body, nonce },
            _ => return Err("hook is the event object".into()),
        },
        "select" => Action::Select,
        "typed" => Action::Typed,
        "answer" => Action::Answer {
            allow: flag(&a, "allow").ok_or("answer needs allow")?,
            via: string(&a, "via").unwrap_or_else(|| "sidebar".into()),
            key: key(&a)?,
        },
        "plugin_answer" => Action::PluginAnswer {
            plugin: string(&a, "plugin").ok_or("plugin_answer needs plugin")?,
            allow: flag(&a, "allow").ok_or("plugin_answer needs allow")?,
            key: key(&a)?,
        },
        "plugin_type" => Action::PluginType {
            plugin: string(&a, "plugin").ok_or("plugin_type needs plugin")?,
            text: string(&a, "text").ok_or("plugin_type needs text")?,
            enter: flag(&a, "enter").unwrap_or(false),
        },
        "close" => Action::Close,
        other => return Err(format!("unknown action {}", haunt_core::event::visible(other))),
    };
    Ok(Step { t, tab, action, scripted, note })
}
