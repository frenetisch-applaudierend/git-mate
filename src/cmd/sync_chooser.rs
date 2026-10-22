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

/// What the board shows: a row being chosen, a row being applied, or every
/// row once all is done.
#[derive(Clone, Copy)]
enum View {
    Choosing(usize),
    Applying(usize),
    Done,
}

/// The chooser's rows on screen, redrawn as a whole on every change. While
/// choosing or applying, a board taller than the terminal shows a window of
/// rows around the one in focus, since redrawing what scrolled off is not
/// possible; once done, every row is printed.
struct Board<'a> {
    term: &'a Term,
    candidates: &'a [Candidate],
    column: usize,
    detail_column: usize,
    /// Terminal lines drawn last time, to move back over.
    drawn: usize,
    /// The first row in the window, kept while the focus stays inside it.
    first: usize,
}

impl Board<'_> {
    fn lines(&mut self, rows: &[DecisionRow], view: View) -> Vec<String> {
        let (anchor, focus) = match view {
            View::Choosing(i) => (Some(i), Some(i)),
            View::Applying(i) => (Some(i), None),
            View::Done => (None, None),
        };
        let row_lines: Vec<Vec<String>> = self
            .candidates
            .iter()
            .zip(rows)
            .enumerate()
            .map(|(i, (candidate, row))| {
                report::decision_lines(
                    candidate,
                    row,
                    focus == Some(i),
                    self.column,
                    self.detail_column,
                )
            })
            .collect();
        let footer = match focus {
            Some(_) => vec![String::new(), format!("   {}", style(HELP).dim())],
            None => Vec::new(),
        };

        let heights: Vec<usize> = row_lines.iter().map(|l| self.height(l)).collect();
        // One line to spare, so the cursor below the board never scrolls
        // its top off screen.
        let budget = (self.term.size().0 as usize).saturating_sub(1);
        let fixed = self.height(&footer);
        let Some(anchor) = anchor.filter(|_| heights.iter().sum::<usize>() + fixed > budget) else {
            return row_lines.into_iter().flatten().chain(footer).collect();
        };

        // Two lines for the "more above / below" markers.
        let room = budget.saturating_sub(fixed + 2).max(heights[anchor]);
        let span = |from: usize, to: usize| heights[from..to].iter().sum::<usize>();
        self.first = self.first.min(anchor);
        while self.first < anchor && span(self.first, anchor + 1) > room {
            self.first += 1;
        }
        let mut end = anchor + 1;
        while end < heights.len() && span(self.first, end + 1) <= room {
            end += 1;
        }
        while self.first > 0 && span(self.first - 1, end) <= room {
            self.first -= 1;
        }

        let more = |n: usize, arrow: &str| match n {
            0 => String::new(),
            n => format!("   {}", style(format!("{arrow} {n} more")).dim()),
        };
        std::iter::once(more(self.first, "↑"))
            .chain(row_lines[self.first..end].iter().flatten().cloned())
            .chain(std::iter::once(more(heights.len() - end, "↓")))
            .chain(footer)
            .collect()
    }

    /// Terminal lines `lines` take up, counting the ones that wrap.
    fn height(&self, lines: &[String]) -> usize {
        let cols = (self.term.size().1 as usize).max(1);
        lines
            .iter()
            .map(|line| console::measure_text_width(line).div_ceil(cols).max(1))
            .sum()
    }

    fn draw(&mut self, rows: &[DecisionRow], view: View) {
        let lines = self.lines(rows, view);
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
/// interact with.
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
        first: 0,
    };

    report::print_title("Remote gone");
    let cursor = HiddenCursor::new(&term);
    let step = loop {
        board.draw(&selection.rows(), View::Choosing(selection.focus));
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
        board.draw(&rows, View::Done);
        return Some(rows);
    }

    let mut rows = selection.rows();
    for (i, candidate) in candidates.iter().enumerate() {
        let choice = selection.choices[i];
        rows[i] = DecisionRow::Applying(choice);
        board.draw(&rows, View::Applying(i));
        rows[i] = apply(candidate, choice);
    }
    board.draw(&rows, View::Done);
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
