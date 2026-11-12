//! `haunt-replay <trace.jsonl>`: replays recorded agent hook events and answer attempts through
//! haunt-core and prints the timeline, the sidebar and every gate decision.

mod mm;
mod out;
mod replay;
mod trace;

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        eprintln!("usage: haunt-replay <trace.jsonl>");
        return ExitCode::from(2);
    };
    let shown = haunt_core::event::visible(path);
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("haunt-replay: {shown}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (header, steps) = match trace::parse(&text) {
        Ok(p) => p,
        Err(e) => {
            // visible, not strip: a newline in the error must not start a line of its own.
            eprintln!("haunt-replay: {shown}: {}", haunt_core::event::visible(&e));
            return ExitCode::FAILURE;
        }
    };
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let text = replay::replay(&header, &steps).render(color);
    // A closed pipe (`| head`) is not worth an error.
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
    ExitCode::SUCCESS
}
