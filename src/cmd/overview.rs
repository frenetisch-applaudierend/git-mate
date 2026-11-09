use console::style;
use std::path::PathBuf;

use crate::git::Tracking;

#[derive(clap::Args)]
#[command(long_about = "\
Show every local branch: where it is checked out, whether that worktree has
uncommitted changes or an unfinished rebase or merge, and how it compares to its
upstream.

Only reads local state: nothing is fetched, so ahead/behind counts are as of the
last fetch (run `git mate sync` to update them).")]
pub struct OverviewArgs {}

/// What `overview` found, reported in `--json` mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OverviewData {
    branches: Vec<BranchEntry>,
    /// Worktrees with a detached HEAD (not mid-rebase).
    detached: Vec<WorktreeState>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchEntry {
    branch: String,
    /// Checked out in the worktree `overview` was run from.
    current: bool,
    default: bool,
    /// Where the branch is checked out, if anywhere.
    #[serde(flatten)]
    checkout: Option<WorktreeState>,
    /// Absent when no upstream is configured.
    upstream: Option<Upstream>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WorktreeState {
    /// "main" or "linked".
    worktree: &'static str,
    path: PathBuf,
    /// Uncommitted changes, tracked or untracked.
    dirty: bool,
    /// An unfinished git operation, e.g. "rebase" or "merge".
    #[serde(skip_serializing_if = "Option::is_none")]
    operation: Option<&'static str>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Upstream {
    name: String,
    /// "up-to-date", "ahead", "behind", "diverged" or "gone".
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ahead: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    behind: Option<usize>,
}

pub fn run(_args: OverviewArgs) -> Result<(), String> {
    let data = overview()?;
    if crate::output::json_mode() {
        crate::output::emit_json_success("overview", data);
    } else {
        print(&data);
    }
    Ok(())
}

fn overview() -> Result<OverviewData, String> {
    let worktrees = crate::git::list_worktrees()?;
    let main_path = worktrees.first().map(|wt| wt.path.clone());
    let current_root = crate::git::current_worktree_root().ok().map(canonical);
    let default_branch = crate::git::detect_default_branch(false).ok();

    let mut checkouts: Vec<(Option<String>, WorktreeState, bool)> = Vec::new();
    for wt in &worktrees {
        let path = wt.path.to_str().ok_or("worktree path is not valid UTF-8")?;
        // Directories deleted without git are listed until pruned.
        if !wt.path.exists() {
            continue;
        }
        let operation = crate::git::operation_in_progress(path);
        // Mid-rebase, git lists the worktree as detached.
        let branch = wt.branch.clone().or_else(|| {
            operation
                .filter(|op| *op == "rebase")
                .and_then(|_| crate::git::branch_being_rebased(path))
        });
        let state = WorktreeState {
            worktree: if Some(&wt.path) == main_path.as_ref() {
                "main"
            } else {
                "linked"
            },
            path: wt.path.clone(),
            dirty: !crate::git::is_worktree_clean(path)?,
            operation,
        };
        let is_current = current_root.as_ref() == Some(&canonical(wt.path.clone()));
        checkouts.push((branch, state, is_current));
    }

    let mut branches: Vec<BranchEntry> = crate::git::list_branch_tracking()?
        .into_iter()
        .map(|(branch, upstream, tracking)| {
            let index = checkouts
                .iter()
                .position(|(b, _, _)| b.as_deref() == Some(branch.as_str()));
            let (checkout, current) = match index {
                Some(i) => {
                    let (_, state, current) = checkouts.remove(i);
                    (Some(state), current)
                }
                None => (None, false),
            };
            BranchEntry {
                current,
                default: default_branch.as_deref() == Some(branch.as_str()),
                branch,
                checkout,
                upstream: upstream.map(|name| upstream_entry(name, tracking)),
            }
        })
        .collect();
    // The default branch first; the rest keep git's (alphabetical) order.
    branches.sort_by_key(|b| !b.default);

    let detached = checkouts.into_iter().map(|(_, state, _)| state).collect();
    Ok(OverviewData { branches, detached })
}

fn upstream_entry(name: String, tracking: Tracking) -> Upstream {
    let (state, ahead, behind) = match tracking {
        Tracking::None | Tracking::Gone => ("gone", None, None),
        Tracking::Counts { ahead, behind } => {
            let state = match (ahead, behind) {
                (0, 0) => "up-to-date",
                (_, 0) => "ahead",
                (0, _) => "behind",
                _ => "diverged",
            };
            (state, Some(ahead), Some(behind))
        }
    };
    Upstream {
        name,
        state,
        ahead,
        behind,
    }
}

fn canonical(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// One row of the human-readable table, before styling.
struct Row<'a> {
    current: bool,
    name: &'a str,
    location: String,
    upstream: (String, console::Style),
    notes: Vec<(String, console::Style)>,
}

fn print(data: &OverviewData) {
    let rows: Vec<Row> = data
        .branches
        .iter()
        .map(|b| Row {
            current: b.current,
            name: &b.branch,
            location: location(b.checkout.as_ref()),
            upstream: upstream_text(b.upstream.as_ref()),
            notes: b.checkout.as_ref().map(notes).unwrap_or_default(),
        })
        .chain(data.detached.iter().map(|wt| Row {
            current: false,
            name: "(detached)",
            location: location(Some(wt)),
            upstream: (String::new(), console::Style::new()),
            notes: notes(wt),
        }))
        .collect();

    let name_width = rows
        .iter()
        .map(|r| r.name.chars().count())
        .max()
        .unwrap_or(0);
    let location_width = rows
        .iter()
        .map(|r| r.location.chars().count())
        .max()
        .unwrap_or(0);
    let upstream_width = rows
        .iter()
        .map(|r| r.upstream.0.chars().count())
        .max()
        .unwrap_or(0);

    for row in rows {
        let marker = if row.current {
            style("●").green().bold().to_string()
        } else {
            " ".to_string()
        };
        let name = format!("{:<name_width$}", row.name);
        let name = if row.current {
            style(name).bold()
        } else {
            style(name)
        };
        let location = style(format!("{:<location_width$}", row.location)).dim();
        let (text, upstream_style) = row.upstream;
        let upstream = upstream_style.apply_to(format!("{text:<upstream_width$}"));
        let notes: Vec<String> = row
            .notes
            .iter()
            .map(|(note, style)| style.apply_to(note).to_string())
            .collect();
        let line = format!(
            "{marker} {name}  {location}  {upstream}  {}",
            notes.join(" ")
        );
        eprintln!("{}", line.trim_end());
    }
}

fn location(checkout: Option<&WorktreeState>) -> String {
    match checkout {
        Some(wt) if wt.worktree == "main" => "main worktree".to_string(),
        Some(wt) => crate::output::display_path(&wt.path),
        None => "–".to_string(),
    }
}

fn upstream_text(upstream: Option<&Upstream>) -> (String, console::Style) {
    let Some(upstream) = upstream else {
        return ("no upstream".to_string(), console::Style::new().dim());
    };
    let ahead = upstream.ahead.unwrap_or(0);
    let behind = upstream.behind.unwrap_or(0);
    match upstream.state {
        "up-to-date" => ("✓ up to date".to_string(), console::Style::new().green()),
        "gone" => ("remote gone".to_string(), console::Style::new().red()),
        "ahead" => (format!("↑{ahead} ahead"), console::Style::new().cyan()),
        "behind" => (format!("↓{behind} behind"), console::Style::new().yellow()),
        _ => (
            format!("↑{ahead} ↓{behind} diverged"),
            console::Style::new().yellow(),
        ),
    }
}

fn notes(wt: &WorktreeState) -> Vec<(String, console::Style)> {
    let mut notes = Vec::new();
    if wt.dirty {
        notes.push(("dirty".to_string(), console::Style::new().yellow()));
    }
    if let Some(operation) = wt.operation {
        notes.push((
            format!("{operation} in progress"),
            console::Style::new().red().bold(),
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::{Tracking, upstream_entry};

    #[test]
    fn upstream_state_from_counts() {
        let state = |ahead, behind| {
            upstream_entry("origin/x".into(), Tracking::Counts { ahead, behind }).state
        };
        assert_eq!(state(0, 0), "up-to-date");
        assert_eq!(state(2, 0), "ahead");
        assert_eq!(state(0, 3), "behind");
        assert_eq!(state(2, 3), "diverged");
        assert_eq!(
            upstream_entry("origin/x".into(), Tracking::Gone).state,
            "gone"
        );
    }
}
