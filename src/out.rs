//! Output as styled pieces. Printing strips every control and hidden character a piece still holds
//! (newlines excepted), and only then adds colour, so no text from a trace can reach the terminal
//! as an escape sequence.

use haunt_core::event::is_hidden;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Dim,
    Bold,
    Good,
    Warn,
    Bad,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Out(Vec<(Style, String)>);

impl Out {
    pub fn push(&mut self, style: Style, text: &str) {
        self.0.push((style, text.to_owned()));
    }

    pub fn line(&mut self, style: Style, text: &str) {
        self.push(style, text);
        self.push(Style::Plain, "\n");
    }

    pub fn extend(&mut self, other: &Out) {
        self.0.extend(other.0.iter().cloned());
    }

    /// The text to print; ANSI colour only when `color`.
    pub fn render(&self, color: bool) -> String {
        let mut s = String::new();
        for (style, text) in &self.0 {
            let text = strip(text);
            let code = match style {
                Style::Plain => None,
                Style::Dim => Some("2"),
                Style::Bold => Some("1"),
                Style::Good => Some("32"),
                Style::Warn => Some("33"),
                Style::Bad => Some("31"),
            };
            match code {
                Some(c) if color && !text.is_empty() => s += &format!("\x1b[{c}m{text}\x1b[0m"),
                _ => s += &text,
            }
        }
        s
    }
}

/// Drops C0, C1, format and other invisible characters but the newline. Every trace string already
/// went through `visible`, which spells them out, so this only catches a slip.
pub fn strip(s: &str) -> String {
    s.chars().filter(|&c| c == '\n' || !is_hidden(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printing_drops_escapes_and_colour_comes_after() {
        let mut o = Out::default();
        o.line(Style::Bad, "a\x1b[2Jb\u{9b}c\u{202e}d\x07");
        assert_eq!(o.render(false), "a[2Jbcd\n");
        assert_eq!(o.render(true), "\x1b[31ma[2Jbcd\x1b[0m\n");
    }
}
