//! How `git mate sync` presents its results: every branch outcome is
//! collected while syncing and printed once, grouped by what (if anything)
//! the user has to do about it, with aligned branch names, dimmed details and
//! a colored one-line summary.

use console::{Style, StyledObject, style};

/// Branch names longer than this push their detail onto the next line
/// instead of widening the name column for everyone.
const MAX_NAME_COLUMN: usize = 40;

/// The report section an outcome is listed under.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Group {
    /// Fast-forwarded, reset or merged.
    Updated,
    /// Skipped or failed for a reason the user should look at.
    Attention,
    /// Remote gone and not merged: left alone on purpose.
    NotMerged,
    /// Remote gone and offered for deletion (deleted, kept or candidate).
    Gone,
    /// Up to date or without upstream: only counted.
    Quiet,
}

/// A labelled, copyable command shown under an entry or a section.
pub struct Hint {
    parts: Vec<(String, String)>,
}

impl Hint {
    pub fn new(label: &str, command: impl Into<String>) -> Self {
        Self {
            parts: vec![(label.to_string(), command.into())],
        }
    }

    pub fn and(mut self, label: &str, command: impl Into<String>) -> Self {
        self.parts.push((label.to_string(), command.into()));
        self
    }

    fn render(&self) -> String {
        self.parts
            .iter()
            .map(|(label, command)| {
                format!(
                    "{} {}",
                    style(format!("{label}:")).dim(),
                    style(command).cyan()
                )
            })
            .collect::<Vec<_>>()
            .join(&format!("   {}   ", style("·").dim()))
    }
}

/// The action taken (or not taken) for a single local branch during sync.
/// `branch`, `action` and `reason` are reported verbatim in `--json` mode;
/// the rest only drives the human-readable report.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOutcome {
    pub branch: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip)]
    pub group: Group,
    #[serde(skip)]
    detail: String,
    #[serde(skip)]
    hint: Option<Hint>,
}

impl BranchOutcome {
    fn new(branch: &str, action: &str, reason: Option<&str>, group: Group, detail: String) -> Self {
        Self {
            branch: branch.to_string(),
            action: action.to_string(),
            reason: reason.map(str::to_string),
            group,
            detail,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: Hint) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn no_upstream(branch: &str) -> Self {
        Self::new(
            branch,
            "no-upstream",
            None,
            Group::Quiet,
            "no upstream".to_string(),
        )
    }

    pub fn up_to_date(branch: &str) -> Self {
        Self::new(
            branch,
            "up-to-date",
            None,
            Group::Quiet,
            "up to date".to_string(),
        )
    }

    pub fn fast_forwarded(branch: &str, commits: usize, dry_run: bool) -> Self {
        let plural = if commits == 1 { "" } else { "s" };
        let verb = if dry_run {
            "would fast-forward"
        } else {
            "fast-forwarded"
        };
        Self::new(
            branch,
            "fast-forwarded",
            None,
            Group::Updated,
            format!("{verb}, {commits} commit{plural}"),
        )
    }

    pub fn reset_to_upstream(branch: &str, previous_tip: &str, dry_run: bool) -> Self {
        let verb = if dry_run { "would reset" } else { "reset" };
        Self::new(
            branch,
            "reset-to-upstream",
            Some(&format!(
                "remote was rewritten; previous tip {previous_tip}"
            )),
            Group::Updated,
            format!("{verb} to rewritten upstream (was {previous_tip})"),
        )
    }

    pub fn merged(branch: &str, dry_run: bool) -> Self {
        let verb = if dry_run { "would merge" } else { "merged" };
        Self::new(
            branch,
            "merged",
            Some("diverged; merged upstream"),
            Group::Updated,
            format!("{verb} upstream"),
        )
    }

    /// Skipped for a reason the user should look at. `reason` goes into
    /// `--json`; `detail` is the shorter text shown in the report.
    pub fn needs_attention(branch: &str, reason: &str, detail: &str) -> Self {
        Self::new(
            branch,
            "skipped",
            Some(reason),
            Group::Attention,
            detail.to_string(),
        )
    }

    /// Remote gone, but the branch has changes not in the default branch.
    /// The section title says so, so the entry itself carries no detail.
    pub fn not_merged(branch: &str, reason: &str) -> Self {
        Self::new(
            branch,
            "skipped",
            Some(reason),
            Group::NotMerged,
            String::new(),
        )
    }

    pub fn failed(branch: &str, error: &str) -> Self {
        // git's stderr can span several lines; the first one says what failed.
        let first_line = error.lines().next().unwrap_or(error);
        Self::new(
            branch,
            "failed",
            Some(error),
            Group::Attention,
            format!("failed: {first_line}"),
        )
    }

    pub fn deletion_candidate(branch: &str, reason: &str) -> Self {
        Self::new(
            branch,
            "deletion-candidate",
            Some(reason),
            Group::Gone,
            String::new(),
        )
    }

    pub fn deleted(branch: &str) -> Self {
        Self::new(
            branch,
            "deleted",
            Some("remote deleted"),
            Group::Gone,
            String::new(),
        )
    }

    pub fn kept(branch: &str, reason: Option<&str>) -> Self {
        Self::new(branch, "kept", reason, Group::Gone, String::new())
    }
}

/// A branch whose remote is gone and that is offered for deletion.
pub struct Candidate {
    pub branch: String,
    /// Why it is safe (or what to watch out for), e.g. "merged in #1156".
    pub detail: String,
    /// Show `detail` as a warning rather than dimmed context.
    pub warn: bool,
}

/// Width of the branch-name column: wide enough for the longest listed name,
/// capped so one very long name doesn't push every detail off screen.
pub fn name_column<'a>(names: impl Iterator<Item = &'a str>) -> usize {
    names
        .map(console::measure_text_width)
        .max()
        .unwrap_or(0)
        .min(MAX_NAME_COLUMN)
}

/// Whether an outcome gets its own line in the report (rather than only
/// being counted in the summary).
pub fn is_listed(outcome: &BranchOutcome) -> bool {
    match outcome.group {
        Group::Quiet => crate::git::is_verbose(),
        Group::Gone => false,
        _ => true,
    }
}

/// Whether an outcome's name takes part in aligning the detail column: only
/// listed entries that actually have a detail to align.
pub fn is_aligned(outcome: &BranchOutcome) -> bool {
    is_listed(outcome) && !outcome.detail.is_empty()
}

/// Print the updated, needs-attention and not-merged sections. Branches
/// offered for deletion are printed separately by [`print_candidates`],
/// next to the prompt that asks about them.
pub fn print_sections(outcomes: &[BranchOutcome], default_branch: Option<&str>, column: usize) {
    let not_merged_title = match default_branch {
        Some(default_branch) => format!("Remote gone, not merged into {default_branch}"),
        None => "Remote gone, not merged".to_string(),
    };
    print_section(
        style("Updated").bold(),
        outcomes.iter().filter(|o| o.group == Group::Updated),
        column,
        None,
    );
    print_section(
        style("Needs attention").yellow().bold(),
        outcomes.iter().filter(|o| o.group == Group::Attention),
        column,
        None,
    );
    print_section(
        style(not_merged_title.as_str()).bold(),
        outcomes.iter().filter(|o| o.group == Group::NotMerged),
        column,
        Some(
            Hint::new("delete", "git branch -D <branch>")
                .and("keep", "git branch --unset-upstream <branch>"),
        ),
    );
    if crate::git::is_verbose() {
        print_section(
            style("Up to date").dim().bold(),
            outcomes.iter().filter(|o| o.group == Group::Quiet),
            column,
            None,
        );
    }
}

fn print_section<'a>(
    title: StyledObject<&str>,
    outcomes: impl Iterator<Item = &'a BranchOutcome>,
    column: usize,
    footer: Option<Hint>,
) {
    let mut outcomes = outcomes.peekable();
    if outcomes.peek().is_none() {
        return;
    }
    eprintln!();
    eprintln!(" {title}");
    for outcome in outcomes {
        let (glyph, detail_style) = appearance(outcome);
        print_entry(
            glyph,
            &outcome.branch,
            &outcome.detail,
            &detail_style,
            column,
        );
        if let Some(hint) = &outcome.hint {
            eprintln!("     {}", hint.render());
        }
    }
    if let Some(footer) = footer {
        eprintln!("   {}", footer.render());
    }
}

fn appearance(outcome: &BranchOutcome) -> (StyledObject<&'static str>, Style) {
    let dim = Style::new().dim();
    match outcome.action.as_str() {
        "fast-forwarded" => (style("✓").green(), dim),
        "reset-to-upstream" => (style("↻").blue(), dim),
        "merged" => (style("⇄").blue(), dim),
        "failed" => (style("✗").red().bold(), Style::new().red()),
        _ => match outcome.group {
            Group::Attention => (style("⚠").yellow(), Style::new()),
            Group::NotMerged => (style("○").magenta().dim(), dim),
            _ => (style("·").dim(), dim),
        },
    }
}

/// Print the branches whose remote is gone and that are offered for
/// deletion, under `title`.
pub fn print_candidates(title: &str, candidates: &[Candidate], column: usize) {
    eprintln!();
    eprintln!(" {}", style(title).bold());
    for candidate in candidates {
        let detail_style = if candidate.warn {
            Style::new().yellow()
        } else {
            Style::new().dim()
        };
        print_entry(
            style("−").red(),
            &candidate.branch,
            &candidate.detail,
            &detail_style,
            column,
        );
    }
}

/// Print a branch-level failure that happened while acting on the
/// candidates (e.g. a worktree that could not be removed).
pub fn print_failure(branch: &str, error: &str, column: usize) {
    let first_line = error.lines().next().unwrap_or(error);
    print_entry(
        style("✗").red().bold(),
        branch,
        &format!("failed: {first_line}"),
        &Style::new().red(),
        column,
    );
}

/// Print a dimmed note belonging to the section above it.
pub fn print_note(note: &str) {
    eprintln!("   {}", style(note).dim());
}

/// `glyph name   detail`, with the detail aligned to `column`. If the name is
/// wider than the column or the line would not fit the terminal, the detail
/// goes on its own indented line instead of letting the terminal wrap it.
fn print_entry(
    glyph: StyledObject<&str>,
    name: &str,
    detail: &str,
    detail_style: &Style,
    column: usize,
) {
    if detail.is_empty() {
        eprintln!(" {glyph} {}", style(name).bold());
        return;
    }
    let name_width = console::measure_text_width(name);
    let detail_width = console::measure_text_width(detail);
    let fits_terminal = console::Term::stderr()
        .size_checked()
        .is_none_or(|(_, cols)| 3 + column + 2 + detail_width <= cols as usize);
    if name_width <= column && fits_terminal {
        let padding = " ".repeat(column - name_width);
        eprintln!(
            " {glyph} {}{padding}  {}",
            style(name).bold(),
            detail_style.apply_to(detail)
        );
    } else {
        eprintln!(" {glyph} {}", style(name).bold());
        eprintln!("     {}", detail_style.apply_to(detail));
    }
}

/// One line of per-outcome counts, each in its section's color, leaving out
/// outcomes that didn't happen.
pub fn print_summary(outcomes: &[BranchOutcome], dry_run: bool) {
    if outcomes.is_empty() {
        return;
    }

    let to_delete = if dry_run {
        "would be deleted"
    } else {
        "to delete"
    };
    let order: [(&str, &str, Style); 10] = [
        ("deleted", "deleted", Style::new().red()),
        ("deletion-candidate", to_delete, Style::new().red()),
        ("fast-forwarded", "fast-forwarded", Style::new().green()),
        ("reset-to-upstream", "reset", Style::new().blue()),
        ("merged", "merged", Style::new().blue()),
        ("failed", "failed", Style::new().red()),
        ("skipped", "need attention", Style::new().yellow()),
        ("kept", "kept", Style::new().magenta()),
        ("up-to-date", "up to date", Style::new().dim()),
        ("no-upstream", "without upstream", Style::new().dim()),
    ];

    let count = |pred: &dyn Fn(&BranchOutcome) -> bool| outcomes.iter().filter(|o| pred(o)).count();
    let mut parts = Vec::new();
    for (action, label, color) in order {
        let n = if action == "skipped" {
            count(&|o| o.action == "skipped" && o.group == Group::Attention)
        } else {
            count(&|o| o.action == action)
        };
        if n > 0 {
            parts.push(color.apply_to(format!("{n} {label}")).to_string());
        }
        // Not-merged branches are skipped too, but need no attention.
        if action == "skipped" {
            let n = count(&|o| o.group == Group::NotMerged);
            if n > 0 {
                parts.push(
                    Style::new()
                        .magenta()
                        .apply_to(format!("{n} not merged"))
                        .to_string(),
                );
            }
        }
    }

    eprintln!();
    eprintln!(" {}", parts.join(&format!(" {} ", style("·").dim())));
}

/// A transient status line on stderr ("⠋ Fetching…") with a spinner,
/// shown only on an interactive terminal and erased before the report is
/// printed.
pub struct Progress {
    shared: Option<std::sync::Arc<Spinner>>,
    ticker: Option<std::thread::JoinHandle<()>>,
}

struct Spinner {
    term: console::Term,
    /// The message being shown, or `None` while nothing is; `stopped` ends
    /// the ticker thread. Every write to the terminal happens under this
    /// lock, so clearing never races with a tick.
    state: std::sync::Mutex<SpinnerState>,
}

struct SpinnerState {
    message: Option<String>,
    frame: usize,
    stopped: bool,
}

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_INTERVAL: std::time::Duration = std::time::Duration::from_millis(80);

impl Spinner {
    fn draw(&self, state: &SpinnerState) {
        if let Some(message) = &state.message {
            let _ = self.term.clear_line();
            let _ = self.term.write_str(&format!(
                "{} {}",
                style(SPINNER_FRAMES[state.frame % SPINNER_FRAMES.len()]).cyan(),
                style(message).dim()
            ));
        }
    }
}

impl Progress {
    pub fn new(json: bool) -> Self {
        let term = console::Term::stderr();
        if json || !term.is_term() || crate::git::is_verbose() {
            return Self {
                shared: None,
                ticker: None,
            };
        }
        let shared = std::sync::Arc::new(Spinner {
            term,
            state: std::sync::Mutex::new(SpinnerState {
                message: None,
                frame: 0,
                stopped: false,
            }),
        });
        let ticker = {
            let shared = std::sync::Arc::clone(&shared);
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(SPINNER_INTERVAL);
                    let Ok(mut state) = shared.state.lock() else {
                        return;
                    };
                    if state.stopped {
                        return;
                    }
                    state.frame += 1;
                    shared.draw(&state);
                }
            })
        };
        Self {
            shared: Some(shared),
            ticker: Some(ticker),
        }
    }

    pub fn update(&self, message: &str) {
        if let Some(shared) = &self.shared
            && let Ok(mut state) = shared.state.lock()
        {
            state.message = Some(message.to_string());
            shared.draw(&state);
        }
    }

    pub fn clear(&self) {
        if let Some(shared) = &self.shared
            && let Ok(mut state) = shared.state.lock()
            && state.message.take().is_some()
        {
            let _ = shared.term.clear_line();
        }
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.clear();
        if let Some(shared) = &self.shared
            && let Ok(mut state) = shared.state.lock()
        {
            state.stopped = true;
        }
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}
