//! A single yes/no question on the terminal.

use console::{Key, Term, style};

/// Ask `question` and wait for y/n (Enter takes `default`, Esc means no).
/// Returns `None` without asking when there is nobody to ask: no terminal,
/// or `--json`.
pub fn confirm(question: &str, default: bool) -> Option<bool> {
    if !can_ask() {
        return None;
    }
    let term = Term::stderr();
    let choices = if default { "[Y/n]" } else { "[y/N]" };
    let _ = term.write_str(&format!(
        "{} {question} {} ",
        style("?").cyan().bold(),
        style(choices).dim()
    ));
    let answer = loop {
        match term.read_key() {
            Ok(Key::Char('y' | 'Y')) => break true,
            Ok(Key::Char('n' | 'N')) | Ok(Key::Escape) => break false,
            Ok(Key::Enter) => break default,
            Ok(_) => {}
            Err(_) => break false,
        }
    };
    let _ = term.write_line(if answer { "yes" } else { "no" });
    Some(answer)
}

/// Whether there is somebody to ask: a terminal, and no `--json`.
pub fn can_ask() -> bool {
    use std::io::IsTerminal as _;
    !crate::output::json_mode() && Term::stderr().is_term() && std::io::stdin().is_terminal()
}
