//! How `git mate sync` presents its results: every branch outcome is
//! collected while syncing and printed once, grouped by what (if anything)
//! the user has to do about it, with aligned branch names, dimmed details and
//! a colored one-line summary.

use console::{Style, StyledObject, style};

/// The name column always grows to at least this width (if a name needs it),
/// even when some details then no longer fit the terminal; beyond it, it only
/// grows as far as the longest detail still fits.
const GUARANTEED_NAME_COLUMN: usize = 40;

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

/// Who decided on the action taken for a branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DecidedBy {
    /// Sync, on its own (fast-forward, reset, merge, rebase).
    Auto,
    /// A flag such as `--delete-pruned`.
    Flag,
    /// The user, when asked.
    User,
}

/// The action taken (or not taken) for a single local branch during sync.
/// The serialized fields are reported verbatim in `--json` mode; the rest
/// only drives the human-readable report.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOutcome {
    pub branch: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// A deletion candidate's detail, e.g. "merged in #1156".
    #[serde(rename = "detail", skip_serializing_if = "Option::is_none")]
    candidate_detail: Option<String>,
    /// The action the chooser would preselect for a deletion candidate.
    #[serde(skip_serializing_if = "Option::is_none")]
    recommended: Option<Choice>,
    /// The actions a deletion candidate can be resolved with.
    #[serde(skip_serializing_if = "Option::is_none")]
    choices: Option<&'static [Choice]>,
    /// Set once sync acted (or tried to act) on the branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    decided_by: Option<DecidedBy>,
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
            candidate_detail: None,
            recommended: None,
            choices: None,
            decided_by: None,
            group,
            detail,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: Hint) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn decided_by(mut self, decided_by: DecidedBy) -> Self {
        self.decided_by = Some(decided_by);
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
        .decided_by(DecidedBy::Auto)
    }

    pub fn reset_to_upstream(branch: &str, previous_tip: &str, dry_run: bool) -> Self {
        let detail = if dry_run {
            "rewritten, would reset".to_string()
        } else {
            format!("rewritten, was {previous_tip}")
        };
        Self::new(
            branch,
            "reset-to-upstream",
            Some(&format!(
                "remote was rewritten; previous tip {previous_tip}"
            )),
            Group::Updated,
            detail,
        )
        .decided_by(DecidedBy::Auto)
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
        .decided_by(DecidedBy::Auto)
    }

    /// Only local commits on top of the upstream.
    pub fn ahead(branch: &str, commits: usize) -> Self {
        let plural = if commits == 1 { "" } else { "s" };
        Self::new(
            branch,
            "up-to-date",
            Some(&format!("ahead of upstream by {commits} commit{plural}")),
            Group::Quiet,
            format!("ahead, {commits} commit{plural} to push"),
        )
    }

    pub fn rebased(branch: &str, dry_run: bool) -> Self {
        let verb = if dry_run { "would rebase" } else { "rebased" };
        Self::new(
            branch,
            "rebased",
            Some("diverged; rebased onto upstream"),
            Group::Updated,
            format!("{verb} onto upstream"),
        )
        .decided_by(DecidedBy::Auto)
    }

    /// The default branch was merged into the current branch.
    pub fn merged_default(branch: &str, default_branch: &str, dry_run: bool) -> Self {
        let verb = if dry_run { "would merge" } else { "merged" };
        Self::new(
            branch,
            "merged-default",
            None,
            Group::Updated,
            format!("{verb} default branch '{default_branch}'"),
        )
        .decided_by(DecidedBy::Auto)
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

    /// A branch whose remote is gone, resolved with `action` (or not yet).
    fn gone(candidate: &Candidate, action: &str, reason: &str) -> Self {
        let mut outcome = Self::new(
            &candidate.branch,
            action,
            Some(reason),
            Group::Gone,
            String::new(),
        );
        outcome.candidate_detail = Some(candidate.detail.clone());
        outcome
    }

    pub fn deletion_candidate(candidate: &Candidate) -> Self {
        let mut outcome = Self::gone(candidate, "deletion-candidate", "remote deleted");
        outcome.recommended = Some(candidate.recommended);
        outcome.choices = Some(&Choice::ALL);
        outcome
    }

    pub fn deleted(candidate: &Candidate) -> Self {
        Self::gone(candidate, "deleted", "remote deleted")
    }

    /// Kept, and its deleted upstream no longer tracked.
    pub fn kept(candidate: &Candidate) -> Self {
        Self::gone(candidate, "kept", "upstream unset")
    }

    /// Remote gone, but nobody decided (the chooser was cancelled, or there
    /// was no terminal to ask in): nothing changed, so the next sync asks
    /// again.
    pub fn left_as_is(candidate: &Candidate) -> Self {
        Self::gone(candidate, "left-as-is", "remote deleted")
    }
}

/// A branch whose remote is gone and that is offered for deletion.
pub struct Candidate {
    pub branch: String,
    /// Why it is safe (or what to watch out for), e.g. "merged in #1156".
    pub detail: String,
    /// Show `detail` as a warning rather than dimmed context.
    pub warn: bool,
    /// The action preselected in the chooser.
    pub recommended: Choice,
}

/// What to do with a branch whose remote is gone.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Choice {
    Delete,
    Keep,
}

impl Choice {
    const ALL: [Choice; 2] = [Choice::Delete, Choice::Keep];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        let i = Self::ALL.iter().position(|&c| c == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    fn label(self) -> &'static str {
        match self {
            Choice::Delete => "delete",
            Choice::Keep => "keep",
        }
    }

    fn style(self) -> Style {
        match self {
            Choice::Delete => Style::new().red(),
            Choice::Keep => Style::new().magenta(),
        }
    }
}

/// The state of a candidate's row in the chooser, from picking an action to
/// its result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionRow {
    Choice(Choice),
    Applying(Choice),
    Deleted,
    /// Kept, and its deleted upstream no longer tracked.
    Kept,
    /// Left untouched because the chooser was cancelled.
    Left,
    Failed(String),
}

/// Width of the longest candidate detail, which the chooser's action column
/// is aligned after.
pub fn detail_column(candidates: &[Candidate]) -> usize {
    candidates
        .iter()
        .map(|c| console::measure_text_width(&c.detail))
        .max()
        .unwrap_or(0)
}

/// The lines of a candidate's row in the chooser: the entry as everywhere
/// else in the report, followed by the action column. Once decided, the
/// column keeps the action taken, which sets these rows apart from the ones
/// sync handled on its own.
pub fn decision_lines(
    candidate: &Candidate,
    row: &DecisionRow,
    focused: bool,
    column: usize,
    detail_column: usize,
) -> Vec<String> {
    let dim = Style::new().dim();
    let (glyph, cell) = match row {
        DecisionRow::Choice(choice) if focused => (
            style("❯").cyan().bold(),
            format!(
                "{} {} {}",
                style("‹").cyan(),
                choice.style().bold().apply_to(choice.label()),
                style("›").cyan()
            ),
        ),
        DecisionRow::Choice(choice) => (
            style("?").cyan(),
            format!("  {}", choice.style().apply_to(choice.label())),
        ),
        DecisionRow::Applying(choice) => (
            style(SPINNER_FRAMES[0]).cyan(),
            format!("  {}", dim.apply_to(choice.label())),
        ),
        DecisionRow::Deleted => (style("⌫").red(), format!("  {}", dim.apply_to("deleted"))),
        DecisionRow::Kept => (style("·").dim(), format!("  {}", dim.apply_to("kept"))),
        DecisionRow::Left => (
            style("·").dim(),
            format!("  {}", dim.apply_to("left as is")),
        ),
        DecisionRow::Failed(error) => {
            let first_line = error.lines().next().unwrap_or(error);
            return entry_lines(
                style("✗").red().bold(),
                &candidate.branch,
                &format!("failed: {first_line}"),
                &Style::new().red(),
                column,
                None,
            );
        }
    };
    entry_lines(
        glyph,
        &candidate.branch,
        &candidate.detail,
        &candidate_style(candidate),
        column,
        Some((&cell, detail_column)),
    )
}

/// Width of the branch-name column for `(name, detail)` entries: wide enough
/// for the longest name, as long as the longest detail still fits the
/// terminal next to it, but never capped below [`GUARANTEED_NAME_COLUMN`].
pub fn name_column<'a>(entries: impl Iterator<Item = (&'a str, &'a str)>) -> usize {
    let (longest_name, longest_detail) = entries.fold((0, 0), |(name, detail), (n, d)| {
        (
            name.max(console::measure_text_width(n)),
            detail.max(console::measure_text_width(d)),
        )
    });
    let available = console::Term::stderr()
        .size_checked()
        .map_or(usize::MAX, |(_, cols)| {
            (cols as usize).saturating_sub(3 + 2 + longest_detail)
        });
    longest_name.min(available.max(GUARANTEED_NAME_COLUMN))
}

/// The detail shown next to an outcome's branch name.
pub fn detail(outcome: &BranchOutcome) -> &str {
    &outcome.detail
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
        "fast-forwarded" => (style("↠").green(), dim),
        "reset-to-upstream" => (style("↯").blue(), dim),
        "merged" | "merged-default" => (style("⇄").blue(), dim),
        "rebased" => (style("⤴").blue(), dim),
        "failed" => (style("✗").red().bold(), Style::new().red()),
        _ => match outcome.group {
            Group::Attention => (style("⚠").yellow(), Style::new()),
            Group::NotMerged => (style("○").magenta().dim(), dim),
            _ => (style("·").dim(), dim),
        },
    }
}

/// Print the branches whose remote is gone and that are offered for
/// deletion, under `title`. They are only marked as candidates here; what
/// actually happened to each is printed by [`print_deleted`] and
/// [`print_kept`] once it did.
pub fn print_candidates(title: &str, candidates: &[Candidate], column: usize) {
    print_title(title);
    for candidate in candidates {
        print_entry(
            style("·").dim(),
            &candidate.branch,
            &candidate.detail,
            &candidate_style(candidate),
            column,
        );
    }
}

/// Print a section title whose entries follow one by one as they happen.
pub fn print_title(title: &str) {
    eprintln!();
    eprintln!(" {}", style(title).bold());
}

/// Print a branch that was deleted. With `candidate_detail`, its candidate
/// detail (e.g. "merged in #1156") is shown, for when the candidates were
/// not listed before; otherwise just "deleted".
pub fn print_deleted(candidate: &Candidate, column: usize) {
    print_entry(
        style("⌫").red(),
        &candidate.branch,
        &candidate.detail,
        &candidate_style(candidate),
        column,
    );
}

fn candidate_style(candidate: &Candidate) -> Style {
    if candidate.warn {
        Style::new().yellow()
    } else {
        Style::new().dim()
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
    for line in entry_lines(glyph, name, detail, detail_style, column, None) {
        eprintln!("{line}");
    }
}

/// The lines of an entry: on one line if it fits the terminal, else with the
/// detail on a second line. `cell`, with the detail width it is aligned
/// after, adds a column after the detail.
fn entry_lines(
    glyph: StyledObject<&str>,
    name: &str,
    detail: &str,
    detail_style: &Style,
    column: usize,
    cell: Option<(&str, usize)>,
) -> Vec<String> {
    let bold_name = style(name).bold();
    if detail.is_empty() && cell.is_none() {
        return vec![format!(" {glyph} {bold_name}")];
    }
    let name_width = console::measure_text_width(name);
    let detail_width = console::measure_text_width(detail);
    let (tail, tail_width) = match cell {
        Some((cell, detail_column)) => (
            format!(
                "{}{}  {cell}",
                detail_style.apply_to(detail),
                " ".repeat(detail_column.saturating_sub(detail_width))
            ),
            detail_column.max(detail_width) + 2 + console::measure_text_width(cell),
        ),
        None => (detail_style.apply_to(detail).to_string(), detail_width),
    };
    let fits_terminal = console::Term::stderr()
        .size_checked()
        .is_none_or(|(_, cols)| 3 + column + 2 + tail_width <= cols as usize);
    if name_width <= column && fits_terminal {
        let padding = " ".repeat(column - name_width);
        vec![format!(" {glyph} {bold_name}{padding}  {tail}")]
    } else {
        vec![format!(" {glyph} {bold_name}"), format!("     {tail}")]
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
    let order: [(&str, &str, Style); 13] = [
        ("deleted", "deleted", Style::new().red()),
        ("deletion-candidate", to_delete, Style::new().red()),
        ("fast-forwarded", "fast-forwarded", Style::new().green()),
        ("reset-to-upstream", "reset", Style::new().blue()),
        ("merged", "merged", Style::new().blue()),
        ("rebased", "rebased", Style::new().blue()),
        (
            "merged-default",
            "merged default branch",
            Style::new().blue(),
        ),
        ("failed", "failed", Style::new().red()),
        ("skipped", "need attention", Style::new().yellow()),
        ("kept", "kept", Style::new().magenta()),
        ("left-as-is", "left as is", Style::new().dim()),
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
