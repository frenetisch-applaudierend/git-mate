//! What `git mate update` did to each branch, as reported in `--json` and
//! printed for humans.

use console::{Style, style};

/// How the base is brought into a branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Strategy {
    Rebase,
    Merge,
}

/// What happened to a branch. Serialized verbatim for `--json`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOutcome {
    pub branch: String,
    /// `up-to-date`, `rebased`, `merged`, `skipped` or `failed`.
    pub action: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_source: Option<crate::git::BaseSource>,
    /// The stacked parent whose changes already landed in the base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_parent: Option<String>,
    pub strategy: Strategy,
    /// Commits the base had that the branch didn't, before updating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<usize>,
    /// Uncommitted changes could not be reapplied and were left in the stash.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub stash_kept: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip)]
    pub dry_run: bool,
}

impl UpdateOutcome {
    pub fn new(branch: &str, strategy: Strategy, dry_run: bool) -> Self {
        Self {
            branch: branch.to_string(),
            action: "skipped",
            base: None,
            base_source: None,
            merged_parent: None,
            strategy,
            behind: None,
            stash_kept: false,
            reason: None,
            hint: None,
            dry_run,
        }
    }

    pub fn with_base(mut self, base: &crate::git::Base) -> Self {
        self.base = Some(base.branch.clone());
        self.base_source = Some(base.source);
        self.merged_parent = base.merged_parent.clone();
        self
    }

    pub fn skipped(mut self, reason: impl Into<String>, hint: Option<String>) -> Self {
        self.action = "skipped";
        self.reason = Some(reason.into());
        self.hint = hint;
        self
    }

    pub fn failed(mut self, error: impl Into<String>) -> Self {
        self.action = "failed";
        self.reason = Some(error.into());
        self
    }

    fn needs_attention(&self) -> bool {
        matches!(self.action, "skipped" | "failed") || self.stash_kept
    }

    fn updated(&self) -> bool {
        matches!(self.action, "rebased" | "merged")
    }

    /// The text next to the branch name.
    fn detail(&self) -> String {
        let base = self.base.as_deref().unwrap_or("its base");
        let stacked = match (self.base_source, &self.merged_parent) {
            (Some(crate::git::BaseSource::Stacked), _) => " (stacked)".to_string(),
            (_, Some(parent)) => format!(" ({parent} was merged; dropped its commits)"),
            _ => String::new(),
        };
        let behind = match self.behind {
            Some(1) => ", 1 new commit".to_string(),
            Some(n) if n > 0 => format!(", {n} new commits"),
            _ => String::new(),
        };
        let would = if self.dry_run { "would be " } else { "" };
        match self.action {
            "up-to-date" => format!("up to date with {base}{stacked}"),
            "rebased" => format!("{would}rebased onto {base}{stacked}{behind}"),
            "merged" => format!("{would}merged {base}{stacked}{behind}"),
            _ => self
                .reason
                .as_deref()
                .and_then(|r| r.lines().next())
                .unwrap_or_default()
                .to_string(),
        }
    }
}

pub fn print(outcomes: &[UpdateOutcome]) {
    let column = outcomes
        .iter()
        .map(|o| console::measure_text_width(&o.branch))
        .max()
        .unwrap_or(0);
    print_section(
        style("Updated").bold(),
        outcomes
            .iter()
            .filter(|o| o.updated() && !o.needs_attention()),
        column,
    );
    print_section(
        style("Needs attention").yellow().bold(),
        outcomes.iter().filter(|o| o.needs_attention()),
        column,
    );
    print_section(
        style("Up to date").dim().bold(),
        outcomes.iter().filter(|o| o.action == "up-to-date"),
        column,
    );

    let updated = outcomes.iter().filter(|o| o.updated()).count();
    let message = match (updated > 0, outcomes.first().is_some_and(|o| o.dry_run)) {
        (true, true) => "Would update (dry run).",
        (true, false) => "Updated.",
        (false, true) => "Already up to date (dry run).",
        (false, false) => "Already up to date.",
    };
    if updated > 0 || !outcomes.iter().any(UpdateOutcome::needs_attention) {
        eprintln!();
        crate::output::success(message);
    }
}

fn print_section<'a>(
    title: console::StyledObject<&str>,
    outcomes: impl Iterator<Item = &'a UpdateOutcome>,
    column: usize,
) {
    let mut outcomes = outcomes.peekable();
    if outcomes.peek().is_none() {
        return;
    }
    eprintln!();
    eprintln!(" {title}");
    for outcome in outcomes {
        let dim = Style::new().dim();
        let (glyph, detail_style) = match outcome.action {
            "rebased" => (style("⤴").blue(), dim),
            "merged" => (style("⇄").blue(), dim),
            "failed" => (style("✗").red().bold(), Style::new().red()),
            "skipped" => (style("⚠").yellow(), Style::new()),
            _ => (style("·").dim(), dim),
        };
        let padding =
            " ".repeat(column.saturating_sub(console::measure_text_width(&outcome.branch)));
        eprintln!(
            " {glyph} {}{padding}  {}",
            style(&outcome.branch).bold(),
            detail_style.apply_to(outcome.detail())
        );
        if outcome.stash_kept {
            eprintln!(
                "     {} {}",
                style("uncommitted changes could not be reapplied; restore with:").yellow(),
                style("git stash pop").cyan()
            );
        }
        if let Some(hint) = &outcome.hint {
            eprintln!(
                "     {} {}",
                style("resolve with:").dim(),
                style(hint).cyan()
            );
        }
    }
}
