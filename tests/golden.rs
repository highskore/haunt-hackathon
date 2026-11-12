//! The replay of every `traces/<name>.jsonl` against `expected/<name>.out`. `HAUNT_BLESS=1` rewrites them.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn replay(path: &Path) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_haunt-replay")).arg(path).output().unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

fn traces() -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = std::fs::read_dir(root().join("traces"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    all.sort();
    all
}

#[test]
fn every_trace_replays_to_its_expected_output() {
    let all = traces();
    assert!(all.len() >= 3, "the traces are missing");
    for path in all {
        let (ok, out, err) = replay(&path);
        assert!(ok, "{}: {err}", path.display());
        let expected = root().join("expected").join(path.file_stem().unwrap()).with_extension("out");
        if std::env::var_os("HAUNT_BLESS").is_some() {
            std::fs::write(&expected, &out).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&expected).unwrap_or_else(|e| panic!("{}: {e} (HAUNT_BLESS=1 writes it)", expected.display()));
        assert!(out == want, "{} differs from {}; rerun with HAUNT_BLESS=1 if that is intended\n{out}", path.display(), expected.display());
    }
}

/// A trace that fails to parse: its stderr against `expected/malformed/<name>.err`, run from the repo
/// root so the path in it is the same everywhere.
#[test]
fn a_malformed_trace_fails_with_its_expected_error() {
    let mut seen = 0;
    for entry in std::fs::read_dir(root().join("traces/malformed")).unwrap() {
        let path = entry.unwrap().path();
        let rel = path.strip_prefix(root()).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_haunt-replay")).arg(rel).current_dir(root()).output().unwrap();
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(!out.status.success() && out.stdout.is_empty(), "{}", rel.display());
        assert!(err.lines().count() == 1 && !err.chars().any(|c| c != '\n' && haunt_core::event::is_hidden(c)), "{err:?}");
        seen += 1;
        let expected = root().join("expected/malformed").join(path.file_stem().unwrap()).with_extension("err");
        if std::env::var_os("HAUNT_BLESS").is_some() {
            std::fs::write(&expected, &err).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&expected).unwrap_or_else(|e| panic!("{}: {e} (HAUNT_BLESS=1 writes it)", expected.display()));
        assert_eq!(err, want, "{}", rel.display());
    }
    assert!(seen >= 2);
}

#[test]
fn no_trace_text_reaches_the_terminal_as_a_control_or_hidden_character() {
    for path in traces() {
        let out = replay(&path).1;
        let bad: Vec<char> = out.chars().filter(|&c| c != '\n' && haunt_core::event::is_hidden(c)).collect();
        assert!(bad.is_empty(), "{}: {bad:?}", path.display());
    }
    let hostile = replay(&root().join("traces/hostile.jsonl")).1;
    for spelled in ["\\u{1b}[2J", "\\u{1b}]52;c;", "\\u{7}", "\\u{202e}", "\\u{9b}", "⏎"] {
        assert!(hostile.contains(spelled), "{spelled} is shown, not dropped");
    }
    let invisible = replay(&root().join("traces/hostile-invisible.jsonl")).1;
    for spelled in ["\\u{7f}", "l\\u{115f}s", "\\u{3164}", "\\u{fe0f}", "look\\u{7f}\\u{3164}around"] {
        assert!(invisible.contains(spelled), "{spelled} is shown, not dropped");
    }
}

#[test]
fn the_same_trace_replays_the_same_way_twice() {
    let path = root().join("traces/handmade-gate.jsonl");
    assert_eq!(replay(&path).1, replay(&path).1);
}

#[test]
fn a_malformed_trace_is_refused_with_its_line() {
    let dir = std::env::temp_dir().join(format!("haunt-replay-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (body, want) in [
        ("{\"t\":5,\"tab\":\"a\",\"open\":{\"cwd\":\"/w\"}}\n{\"t\":1,\"tab\":\"a\",\"typed\":true}\n", "line 2: t goes backwards"),
        ("{\"t\":0,\"tab\":\"a\",\"typed\":true,\"nonce\":false}\n", "line 1: nonce applies to hook steps only"),
        ("{\"t\":0,\"tab\":\"a\",\"typed\":true,\"select\":true}\n", "line 1: needs exactly one action"),
        ("{\"t\":0,\"tab\":\"a\",\"fly\\u001b[2J\":true}\n", "line 1: unknown action fly\\u{1b}[2J"),
        ("not json\n", "line 1:"),
    ] {
        let path = dir.join("bad.jsonl");
        std::fs::write(&path, body).unwrap();
        let (ok, _, err) = replay(&path);
        assert!(!ok && err.contains(want), "{body:?}: {err}");
        assert!(!err.contains('\x1b'));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
