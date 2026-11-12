# Traces

`haunt-replay <trace.jsonl>` runs a trace through haunt-core, the core the Haunt app runs. That covers its session state, the intake's hook decoding and nonce check, and the answer gate. It prints each step, the sidebar whenever it changes, every gate decision with its reason, and the decision log. Time comes from the trace, so a replay prints the same on every run. `expected/<name>.out` is the expected output, which `tests/golden.rs` checks (`HAUNT_BLESS=1` rewrites it).

| trace | made how |
|---|---|
| `recorded-claude-session.jsonl` | **Recorded** from a real Claude Code session (2.1.322) with the haunt-claude adapter: its socket lines and their arrival times. The cwd and prompts are scrubbed, and the session, tool call and subagent ids are replaced with stable fakes. One step is scripted and marked `"scripted": true`: a late notification Allow. |
| `handmade-gate.jsonl` | **Hand-made.** Hook lines written to haunt's event spec (`docs/agent-events.md`) plus scripted answers: a forged prompt, stale and repeated answers, multi-line, hidden-character and multi-file requests, plugin answers and plugin typing. |
| `hostile.jsonl` | **Hand-made.** Terminal escapes (`ESC[2J`, OSC 52), C1 controls, bidi overrides and newlines in every string a trace controls. The replay spells them out and never prints them raw. |

## Format

One JSON object per line. An optional first line `{"trace": {"title", "source", "notes": [...]}}` describes the trace. Every other line is a step with these fields:
- `t`: milliseconds since the start, never decreasing.
- `tab`: a name the trace picks.
- `note`: optional.
- `"scripted": true`: marks anything not recorded.
- exactly one action from the table below.

| action | what the core is asked |
|---|---|
| `"open": {"cwd": "/w"}` | `register_surface`: the tab gets a surface id and a random nonce |
| `"hook": {event}` | `replay_line` with the event as a socket line, carrying the tab's surface id and nonce. `"nonce": false` leaves the nonce out; `"nonce": "<hex>"` sends a wrong one. |
| `"typed": true` | `on_user_input`: the user typed at the terminal |
| `"select": true` | `select_surface` |
| `"answer": {"allow": true, "via": "notification", "key": "live"}` | `answer_permission`, with `answer_refusal` for the reason. `key` is `"live"` (what the row shows now) or a request number, as an older notification would carry. |
| `"plugin_answer": {"plugin": "<id>", "allow": true, "key": "live"}` | `gate_plugin_request` with a plugin's `pty-answer`; the reason comes from `take_plugin_refusals` |
| `"plugin_type": {"plugin": "<id>", "text": "make", "enter": true}` | `gate_plugin_request` with a plugin's `pty-type` |
| `"close": true` | `unregister_surface` |

## What a replay leaves out

- The socket itself and its rate limit: lines go straight to the decoder the intake uses.
- Git branch lookups: rows show the cwd where the app would show a branch.
- Plugins: no WASM runs. A plugin step is the request a plugin would queue, sent through the same gate.
- The app's own checks. For example, whether a notification offers Allow also depends on the command fitting on one line of the banner, and the Swift app measures that.
- Typing refusals: the gate gives no reason for them. The replay names the text check (`untypable`) when that is what failed, and otherwise the gate's other two conditions together.
