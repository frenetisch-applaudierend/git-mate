//! The inline chooser `git mate sync` shows for branches whose remote is
//! gone: one row per branch with its recommended action preselected, changed
//! in place with the arrow keys and applied row by row, each row turning
//! into its result where it stands.

use super::sync_report::{self as report, Candidate, Choice, DecisionRow};
use console::{Key, Term, style};

const HELP: &str = "↑↓ select · ←→ change · ⏎ apply · esc leave all as is";

/// What a key press asks of the chooser.
#[derive(Debug, Eq, PartialEq)]
enum Step {
    Continue,
    Apply,
    Cancel,
}

/// The action picked for every row and the row being changed.
struct Selection {
    choices: Vec<Choice>,
    focus: usize,
}

impl Selection {
    fn new(candidates: &[Candidate]) -> Self {
        Self {
            choices: candidates.iter().map(|c| c.recommended).collect(),
            focus: 0,
        }
    }

    fn handle(&mut self, key: &Key) -> Step {
        let last = self.choices.len().saturating_sub(1);
        match key {
            Key::ArrowUp => self.focus = self.focus.saturating_sub(1),
            Key::ArrowDown => self.focus = (self.focus + 1).min(last),
            Key::ArrowLeft => self.choices[self.focus] = self.choices[self.focus].previous(),
            Key::ArrowRight | Key::Char(' ') => {
                self.choices[self.focus] = self.choices[self.focus].next()
            }
            Key::Enter => return Step::Apply,
            Key::Escape | Key::CtrlC | Key::Char('q') => return Step::Cancel,
            _ => {}
        }
        Step::Continue
    }

    fn rows(&self) -> Vec<DecisionRow> {
        self.choices
            .iter()
            .map(|&c| DecisionRow::Choice(c))
            .collect()
    }
}

/// The chooser's rows on screen, redrawn as a whole on every change.
struct Board<'a> {
    term: &'a Term,
    candidates: &'a [Candidate],
    column: usize,
    detail_column: usize,
    /// Terminal lines drawn last time, to move back over.
    drawn: usize,
}

impl Board<'_> {
    /// The rows, plus the key help while `focus` marks a row being chosen.
    fn lines(&self, rows: &[DecisionRow], focus: Option<usize>) -> Vec<String> {
        let mut lines: Vec<String> = self
            .candidates
            .iter()
            .zip(rows)
            .enumerate()
            .flat_map(|(i, (candidate, row))| {
                report::decision_lines(
                    candidate,
                    row,
                    focus == Some(i),
                    self.column,
                    self.detail_column,
                )
            })
            .collect();
        if focus.is_some() {
            lines.push(String::new());
            lines.push(format!("   {}", style(HELP).dim()));
        }
        lines
    }

    /// Terminal lines `lines` take up, counting the ones that wrap.
    fn height(&self, lines: &[String]) -> usize {
        let cols = (self.term.size().1 as usize).max(1);
        lines
            .iter()
            .map(|line| console::measure_text_width(line).div_ceil(cols).max(1))
            .sum()
    }

    fn draw(&mut self, rows: &[DecisionRow], focus: Option<usize>) {
        let lines = self.lines(rows, focus);
        if self.drawn > 0 {
            let _ = self.term.move_cursor_up(self.drawn);
            let _ = self.term.clear_to_end_of_screen();
        }
        for line in &lines {
            let _ = self.term.write_line(line);
        }
        self.drawn = self.height(&lines);
    }
}

/// Hides the cursor while choosing, and shows it again however choosing ends.
struct HiddenCursor<'a>(&'a Term);

impl<'a> HiddenCursor<'a> {
    fn new(term: &'a Term) -> Self {
        let _ = term.hide_cursor();
        Self(term)
    }
}

impl Drop for HiddenCursor<'_> {
    fn drop(&mut self) {
        let _ = self.0.show_cursor();
    }
}

/// Let the user pick an action for every candidate, then `apply` them in
/// order, each row showing its result as soon as it is done. Cancelling
/// leaves every branch as it is.
///
/// Returns `None` without printing anything when there is no terminal to
/// interact with, or when the rows would not fit on screen (redrawing them
/// would scroll); the caller then falls back to a line prompt.
pub fn resolve(
    candidates: &[Candidate],
    column: usize,
    mut apply: impl FnMut(&Candidate, Choice) -> DecisionRow,
) -> Option<Vec<DecisionRow>> {
    use std::io::IsTerminal as _;
    let term = Term::stderr();
    if candidates.is_empty() || !term.is_term() || !std::io::stdin().is_terminal() {
        return None;
    }

    let mut selection = Selection::new(candidates);
    let mut board = Board {
        term: &term,
        candidates,
        column,
        detail_column: report::detail_column(candidates),
        drawn: 0,
    };
    // The title and its blank line sit above the board.
    let needed = board.height(&board.lines(&selection.rows(), Some(0))) + 2;
    if needed >= term.size().0 as usize {
        return None;
    }

    report::print_title("Remote gone");
    let cursor = HiddenCursor::new(&term);
    let step = loop {
        board.draw(&selection.rows(), Some(selection.focus));
        match term.read_key_raw() {
            Ok(key) => match selection.handle(&key) {
                Step::Continue => {}
                step => break step,
            },
            Err(_) => break Step::Cancel,
        }
    };
    // Show the cursor again before anything that could be interrupted.
    drop(cursor);

    if step == Step::Cancel {
        let rows = vec![DecisionRow::Left; candidates.len()];
        board.draw(&rows, None);
        return Some(rows);
    }

    let mut rows = selection.rows();
    for (i, candidate) in candidates.iter().enumerate() {
        let choice = selection.choices[i];
        rows[i] = DecisionRow::Applying(choice);
        board.draw(&rows, None);
        rows[i] = apply(candidate, choice);
    }
    board.draw(&rows, None);
    Some(rows)
}

#[cfg(test)]
mod tests {
    use super::{Selection, Step};
    use crate::cmd::sync_report::{Candidate, Choice};
    use console::Key;

    fn candidate(branch: &str, recommended: Choice) -> Candidate {
        Candidate {
            branch: branch.to_string(),
            detail: String::new(),
            warn: false,
            recommended,
        }
    }

    fn selection() -> Selection {
        Selection::new(&[
            candidate("merged", Choice::Delete),
            candidate("unmerged", Choice::Keep),
        ])
    }

    #[test]
    fn preselects_recommended_actions() {
        assert_eq!(selection().choices, [Choice::Delete, Choice::Keep]);
    }

    #[test]
    fn arrows_change_only_the_focused_row() {
        let mut selection = selection();
        assert_eq!(selection.handle(&Key::ArrowDown), Step::Continue);
        selection.handle(&Key::ArrowRight);
        assert_eq!(selection.choices, [Choice::Delete, Choice::Delete]);
        selection.handle(&Key::ArrowUp);
        selection.handle(&Key::ArrowLeft);
        assert_eq!(selection.choices, [Choice::Keep, Choice::Delete]);
    }

    #[test]
    fn focus_stays_within_rows() {
        let mut selection = selection();
        selection.handle(&Key::ArrowUp);
        assert_eq!(selection.focus, 0);
        selection.handle(&Key::ArrowDown);
        selection.handle(&Key::ArrowDown);
        assert_eq!(selection.focus, 1);
    }

    #[test]
    fn enter_applies_and_escape_cancels() {
        let mut selection = selection();
        assert_eq!(selection.handle(&Key::Enter), Step::Apply);
        assert_eq!(selection.handle(&Key::Escape), Step::Cancel);
        assert_eq!(selection.handle(&Key::CtrlC), Step::Cancel);
    }
}
