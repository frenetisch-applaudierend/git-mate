#[derive(clap::Args)]
#[command(
    about = "Fetch and merge the latest changes",
    long_about = "Fetch the latest changes from all remotes and bring your local repository up to date.

Fetches all remotes, fast-forwards local branches that haven't diverged from their
upstream, and pulls the current branch. Use --rebase or --ff-only to control how the
pull is applied.

When a remote branch is deleted — typically after a PR is merged — sync lists every
local branch (and worktree) left without a remote and asks once whether to delete all
of them, keep all of them, or decide branch by branch. Branches with unpushed commits
or a dirty working tree are never offered for deletion. Pass --delete-pruned to skip
that prompt and delete all of them (this also makes --json actually delete instead of
just reporting candidates).

Pass --merge (or set mate.autoMerge=true in git config) to also merge the default
branch into the current branch after pulling, keeping feature branches up to date
with main.

Pass --dry-run to preview what sync would do (fast-forwards, deletion candidates,
pull, auto-merge) without changing anything or prompting."
)]
pub struct SyncArgs {
    #[arg(long, help = "Pull with --rebase")]
    pub rebase: bool,
    #[arg(long, help = "Pull with --ff-only")]
    pub ff_only: bool,
    #[arg(
        long,
        conflicts_with = "no_merge",
        help = "Merge the default branch into the current branch"
    )]
    pub merge: bool,
    #[arg(
        long,
        conflicts_with = "merge",
        help = "Skip auto-merge, even if enabled in git config"
    )]
    pub no_merge: bool,
    #[arg(
        long,
        conflicts_with = "dry_run",
        help = "Delete every local branch whose remote was pruned, without prompting"
    )]
    pub delete_pruned: bool,
    #[arg(
        long,
        help = "Preview planned actions without making any changes or prompting"
    )]
    pub dry_run: bool,
}

/// The action taken (or not taken) for a single local branch during sync,
/// reported verbatim in `--json` mode; human mode narrates the same facts
/// via `output::info`/`output::success` as they happen.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchOutcome {
    branch: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl BranchOutcome {
    fn new(branch: &str, action: &str, reason: Option<&str>) -> Self {
        Self {
            branch: branch.to_string(),
            action: action.to_string(),
            reason: reason.map(str::to_string),
        }
    }

    fn no_upstream(branch: &str) -> Self {
        Self::new(branch, "no-upstream", None)
    }

    fn skipped(branch: &str, reason: &str) -> Self {
        Self::new(branch, "skipped", Some(reason))
    }

    fn up_to_date(branch: &str) -> Self {
        Self::new(branch, "up-to-date", None)
    }

    fn fast_forwarded(branch: &str) -> Self {
        Self::new(branch, "fast-forwarded", None)
    }

    fn deletion_candidate(branch: &str, reason: &str) -> Self {
        Self::new(branch, "deletion-candidate", Some(reason))
    }

    fn deleted(branch: &str) -> Self {
        Self::new(branch, "deleted", Some("remote deleted"))
    }

    fn kept(branch: &str) -> Self {
        Self::new(branch, "kept", None)
    }
}

/// What happened to the branch `sync` was run from.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentBranchOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    pulled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pull_skipped_reason: Option<String>,
    merged_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    merge_skipped_reason: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncData {
    dry_run: bool,
    branches: Vec<BranchOutcome>,
    current_branch: CurrentBranchOutcome,
}

pub fn run(args: SyncArgs) -> Result<(), String> {
    let json = crate::output::json_mode();
    let dry_run = args.dry_run;
    let mut branch_outcomes: Vec<BranchOutcome> = Vec::new();

    // 1. Snapshot every local branch's tip and its upstream tip before fetching,
    //    so we can tell whether a branch had unique commits even after the upstream
    //    is pruned away.
    let branches_before = snapshot_branch_upstreams();

    // 2. Fetch everything and prune stale remote-tracking refs.
    let tracking_before: std::collections::HashSet<String> =
        crate::git::list_remote_tracking_refs()?
            .into_iter()
            .collect();

    crate::git::fetch_all()?;

    let tracking_after: std::collections::HashSet<String> =
        crate::git::list_remote_tracking_refs()?
            .into_iter()
            .collect();

    let pruned: std::collections::HashSet<&String> =
        tracking_before.difference(&tracking_after).collect();

    // 3. Gather context we need throughout.
    let current_branch = crate::git::current_branch().ok();
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let main_wt_path = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?
        .to_string();
    let current_wt = {
        let cwd = std::env::current_dir()
            .map_err(|e| format!("could not determine current directory: {e}"))?;
        worktrees
            .iter()
            .filter(|wt| cwd.starts_with(&wt.path))
            .max_by_key(|wt| wt.path.components().count())
            .map(|wt| wt.path.clone())
    };

    // 4. Process all local branches that have an upstream.
    let branches = crate::git::list_local_branches_with_upstream()?;

    let mut current_branch_pruned = false;
    let mut deletion_candidates: Vec<String> = Vec::new();

    for (branch, upstream) in &branches {
        let Some(upstream) = upstream else {
            branch_outcomes.push(BranchOutcome::no_upstream(branch));
            continue;
        };

        let is_current = current_branch.as_deref() == Some(branch.as_str());

        if pruned.contains(upstream) {
            if is_current {
                current_branch_pruned = true;
            }

            let had_unique = branches_before
                .iter()
                .find(|(b, _, _)| b == branch)
                .map(|(_, local_sha, upstream_sha)| {
                    // The branch had unique commits if its tip was NOT an ancestor of
                    // (i.e., was ahead of) the upstream tip at snapshot time.
                    match (local_sha, upstream_sha) {
                        (Some(l), Some(u)) => !crate::git::is_ancestor(l, u).unwrap_or(false),
                        // No upstream SHA recorded means we couldn't check → be safe.
                        _ => true,
                    }
                })
                .unwrap_or(true);

            match pruned_branch_status(branch, had_unique, &worktrees)? {
                PrunedStatus::Skip(reason) => {
                    crate::output::info(&format!("{branch}: {reason}"));
                    branch_outcomes.push(BranchOutcome::skipped(branch, reason));
                }
                PrunedStatus::Eligible => deletion_candidates.push(branch.clone()),
            }
        } else if !is_current {
            // Remote still exists — try to fast-forward.
            branch_outcomes.push(fast_forward_branch(branch, upstream, &worktrees, dry_run)?);
        }
        // Current branch with live upstream is handled by pull below.
    }

    // Ask once, up front, about every branch whose remote was deleted rather
    // than prompting one at a time. Under --json or --dry-run, no prompt is
    // issued: candidates are reported but never deleted (see pruned_branch
    // deletion policy above).
    branch_outcomes.extend(resolve_pruned_deletions(
        &deletion_candidates,
        &current_wt,
        &worktrees,
        &main_wt_path,
        json,
        dry_run,
        args.delete_pruned,
    )?);

    if !json {
        print_summary(&branch_outcomes);
    }

    // 5. Pull the current branch (if it still has an upstream).
    if current_branch_pruned {
        if json {
            crate::output::emit_json_success(
                "sync",
                SyncData {
                    dry_run,
                    branches: branch_outcomes,
                    current_branch: CurrentBranchOutcome {
                        name: current_branch,
                        pulled: false,
                        pull_skipped_reason: Some(
                            "current branch's upstream was deleted".to_string(),
                        ),
                        merged_default: false,
                        merge_skipped_reason: None,
                    },
                },
            );
        } else {
            crate::output::success(final_status_message(
                any_branch_action_taken(&branch_outcomes),
                dry_run,
            ));
        }
        return Ok(());
    }

    let auto_merge = auto_merge_enabled(&args)?;
    let has_upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let mut synced_current_branch = false;
    let mut pull_skipped_reason = None;
    if !has_upstream {
        crate::output::info("No upstream configured for current branch, skipping pull.");
        pull_skipped_reason = Some("no upstream configured for current branch".to_string());
    } else {
        let mut extra_flags = vec![];
        if args.rebase {
            extra_flags.push("--rebase");
        }
        if args.ff_only {
            extra_flags.push("--ff-only");
        }
        if dry_run {
            crate::output::info(&format!(
                "{}: would pull",
                current_branch.as_deref().unwrap_or("current branch")
            ));
        } else {
            crate::git::pull(&extra_flags)?;
        }
        synced_current_branch = true;
    }

    let (merged_default, merge_skipped_reason) = if auto_merge {
        merge_default_branch(current_branch.as_deref(), dry_run)?
    } else {
        (false, None)
    };

    if !json {
        let any_work =
            synced_current_branch || merged_default || any_branch_action_taken(&branch_outcomes);
        crate::output::success(final_status_message(any_work, dry_run));
    }

    if json {
        crate::output::emit_json_success(
            "sync",
            SyncData {
                dry_run,
                branches: branch_outcomes,
                current_branch: CurrentBranchOutcome {
                    name: current_branch,
                    pulled: synced_current_branch,
                    pull_skipped_reason,
                    merged_default,
                    merge_skipped_reason,
                },
            },
        );
    }

    Ok(())
}

/// Whether any branch was actually (or, under `--dry-run`, would be) changed
/// during this run. Used to pick the final status message: the old logic
/// based this solely on the current branch's own pull/merge, so a run that
/// fast-forwarded or deleted several other branches but left the current
/// branch untouched printed no final status at all.
fn any_branch_action_taken(outcomes: &[BranchOutcome]) -> bool {
    outcomes.iter().any(|o| {
        matches!(
            o.action.as_str(),
            "fast-forwarded" | "deleted" | "deletion-candidate"
        )
    })
}

/// The final status line, covering both real work and the "nothing to do"
/// case that used to print no message at all.
fn final_status_message(any_work: bool, dry_run: bool) -> &'static str {
    match (any_work, dry_run) {
        (true, true) => "Would sync (dry run).",
        (true, false) => "Synced.",
        (false, true) => "Already up to date (dry run).",
        (false, false) => "Already up to date.",
    }
}

/// Print a one-line, grouped-by-outcome summary of everything done to local
/// branches, plus a reason line for each skipped branch (reasons vary, so a
/// bare count isn't enough to act on). Per-branch detail is still narrated as
/// it happens via `output::info`; this recap is what makes the results
/// scannable once more than a couple of branches are involved.
fn print_summary(outcomes: &[BranchOutcome]) {
    if outcomes.is_empty() {
        return;
    }

    const ORDER: [(&str, &str); 7] = [
        ("fast-forwarded", "fast-forwarded"),
        ("up-to-date", "already up to date"),
        ("deleted", "deleted"),
        ("deletion-candidate", "flagged for deletion"),
        ("kept", "kept"),
        ("skipped", "skipped"),
        ("no-upstream", "with no upstream"),
    ];

    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for outcome in outcomes {
        *counts.entry(outcome.action.as_str()).or_insert(0) += 1;
    }

    let parts: Vec<String> = ORDER
        .iter()
        .filter_map(|(action, label)| counts.get(action).map(|count| format!("{count} {label}")))
        .collect();

    crate::output::info(&format!("Summary: {}", parts.join(", ")));

    for outcome in outcomes {
        if outcome.action == "skipped" {
            if let Some(reason) = &outcome.reason {
                crate::output::info(&format!("  {}: {reason}", outcome.branch));
            }
        }
    }
}

fn auto_merge_enabled(args: &SyncArgs) -> Result<bool, String> {
    if args.merge {
        return Ok(true);
    }
    if args.no_merge {
        return Ok(false);
    }
    let Some(value) = crate::git::config::read_string("mate.autoMerge") else {
        return Ok(false);
    };

    match value.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(true),
        "false" | "no" | "off" | "0" => Ok(false),
        _ => Err(format!(
            "invalid value for mate.autoMerge: {value:?}; expected a boolean"
        )),
    }
}

/// Returns (merged, skip_reason). `skip_reason` is set whenever `merged` is
/// `false`, describing why auto-merge did not happen.
fn merge_default_branch(
    current_branch: Option<&str>,
    dry_run: bool,
) -> Result<(bool, Option<String>), String> {
    let Some(current_branch) = current_branch else {
        crate::output::info("Could not determine current branch, skipping auto-merge.");
        return Ok((false, Some("could not determine current branch".to_string())));
    };
    if current_branch == "HEAD" {
        crate::output::info("Detached HEAD, skipping auto-merge.");
        return Ok((false, Some("detached HEAD".to_string())));
    }

    let default_branch = crate::git::detect_default_branch(false)?;
    if current_branch == default_branch {
        crate::output::info("Current branch is the default branch, skipping auto-merge.");
        return Ok((
            false,
            Some("current branch is the default branch".to_string()),
        ));
    }

    if dry_run {
        crate::output::info(&format!(
            "{current_branch}: would merge default branch '{default_branch}'"
        ));
        return Ok((true, None));
    }

    crate::git::merge(&["--no-edit", &default_branch])?;
    crate::output::info(&format!(
        "{current_branch}: merged default branch '{default_branch}'"
    ));
    Ok((true, None))
}

/// Returns (branch, local_sha, upstream_sha) for every local branch that has an upstream.
/// Called before fetching, so upstream SHAs reflect the state on the remote right now.
fn snapshot_branch_upstreams() -> Vec<(String, Option<String>, Option<String>)> {
    let Ok(branches) = crate::git::list_local_branches_with_upstream() else {
        return vec![];
    };
    branches
        .into_iter()
        .map(|(branch, upstream)| {
            let local_sha = crate::git::resolve_ref(&branch).ok();
            let upstream_sha = upstream
                .as_deref()
                .and_then(|u| crate::git::resolve_ref(u).ok());
            (branch, local_sha, upstream_sha)
        })
        .collect()
}

fn fast_forward_branch(
    branch: &str,
    upstream: &str,
    worktrees: &[crate::git::WorktreeEntry],
    dry_run: bool,
) -> Result<BranchOutcome, String> {
    let local_sha = match crate::git::resolve_ref(branch) {
        Ok(sha) => sha,
        Err(_) => return Ok(BranchOutcome::skipped(branch, "could not resolve local ref")),
    };
    let remote_sha = match crate::git::resolve_ref(upstream) {
        Ok(sha) => sha,
        Err(_) => {
            return Ok(BranchOutcome::skipped(
                branch,
                "could not resolve upstream ref",
            ));
        }
    };
    if local_sha == remote_sha {
        return Ok(BranchOutcome::up_to_date(branch));
    }
    if !crate::git::is_ancestor(&local_sha, &remote_sha)? {
        crate::output::info(&format!(
            "{branch}: cannot fast-forward (diverged), skipping"
        ));
        return Ok(BranchOutcome::skipped(
            branch,
            "cannot fast-forward (diverged)",
        ));
    }

    if dry_run {
        crate::output::info(&format!("{branch}: would fast-forward"));
        return Ok(BranchOutcome::fast_forwarded(branch));
    }

    // If the branch is checked out in a worktree, use `merge --ff-only` so the
    // index and working tree are updated alongside the ref.  Plain `update-ref`
    // would move the ref without touching the worktree, making `git status`
    // report phantom modifications there.
    if let Some(wt) = crate::git::worktree_for_branch(branch, worktrees) {
        let path = wt
            .path
            .to_str()
            .ok_or("worktree path is not valid UTF-8")?;
        crate::git::merge_ff_only_in(path, upstream)?;
    } else {
        crate::git::update_ref(&format!("refs/heads/{branch}"), &remote_sha)?;
    }
    crate::output::info(&format!("{branch}: fast-forwarded"));
    Ok(BranchOutcome::fast_forwarded(branch))
}

enum PrunedStatus {
    Skip(&'static str),
    Eligible,
}

/// Decide whether a branch whose upstream was pruned is safe to delete,
/// without prompting or acting. Branches with unpushed commits or a dirty
/// checked-out working tree are never candidates for deletion.
fn pruned_branch_status(
    branch: &str,
    had_unique_commits: bool,
    worktrees: &[crate::git::WorktreeEntry],
) -> Result<PrunedStatus, String> {
    if had_unique_commits {
        return Ok(PrunedStatus::Skip(
            "remote deleted but has unpushed commits, skipping",
        ));
    }

    let checked_out_wt = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
        .map(|wt| &wt.path);

    if let Some(wt_path) = checked_out_wt {
        let wt_str = wt_path.to_str().ok_or("worktree path is not valid UTF-8")?;
        if !crate::git::is_worktree_clean(wt_str)? {
            return Ok(PrunedStatus::Skip(
                "remote deleted but working tree is dirty, skipping",
            ));
        }
    }

    Ok(PrunedStatus::Eligible)
}

/// Resolves what to do with branches whose remote was deleted, in priority
/// order: `--dry-run` always previews only, regardless of the other flags;
/// then `--delete-pruned` deletes all candidates without prompting (this is
/// what makes `--json` actually delete rather than just report); otherwise
/// plain `--json` never prompts (which would block on stdin) and reports
/// candidates without acting; otherwise ask once about every branch and act
/// on the choice: delete them all, keep them all, or decide branch by
/// branch.
fn resolve_pruned_deletions(
    candidates: &[String],
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
    json: bool,
    dry_run: bool,
    delete_pruned: bool,
) -> Result<Vec<BranchOutcome>, String> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    if dry_run {
        crate::output::info("Local branches whose remote was deleted:");
        for branch in candidates {
            crate::output::info(&format!("  {branch}"));
        }
        crate::output::info("Dry run: not deleting any of these.");
        return Ok(candidates
            .iter()
            .map(|branch| BranchOutcome::deletion_candidate(branch, "remote deleted"))
            .collect());
    }

    if delete_pruned {
        let mut outcomes = Vec::with_capacity(candidates.len());
        for branch in candidates {
            delete_pruned_branch(branch, current_wt, worktrees, main_wt_path)?;
            outcomes.push(BranchOutcome::deleted(branch));
        }
        return Ok(outcomes);
    }

    if json {
        return Ok(candidates
            .iter()
            .map(|branch| BranchOutcome::deletion_candidate(branch, "remote deleted"))
            .collect());
    }

    crate::output::info("Local branches whose remote was deleted:");
    for branch in candidates {
        crate::output::info(&format!("  {branch}"));
    }

    let to_delete: Vec<&str> = match prompt_bulk_choice(candidates.len()) {
        BulkChoice::All => candidates.iter().map(String::as_str).collect(),
        BulkChoice::None => Vec::new(),
        BulkChoice::Decide => candidates
            .iter()
            .filter(|branch| prompt_yes_no(&format!("Delete local branch '{branch}'?")))
            .map(String::as_str)
            .collect(),
    };

    let mut outcomes = Vec::with_capacity(candidates.len());
    for branch in candidates {
        if to_delete.contains(&branch.as_str()) {
            delete_pruned_branch(branch, current_wt, worktrees, main_wt_path)?;
            outcomes.push(BranchOutcome::deleted(branch));
        } else {
            crate::output::info(&format!("{branch}: kept"));
            outcomes.push(BranchOutcome::kept(branch));
        }
    }

    Ok(outcomes)
}

enum BulkChoice {
    All,
    None,
    Decide,
}

fn prompt_bulk_choice(count: usize) -> BulkChoice {
    use std::io::Write as _;
    eprint!("Delete all {count}, keep all, or decide for each? [a/N/d] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return BulkChoice::None;
    }
    match line.trim().to_lowercase().as_str() {
        "a" | "all" => BulkChoice::All,
        "d" | "decide" => BulkChoice::Decide,
        _ => BulkChoice::None,
    }
}

/// Remove the worktree (if any) and force-delete the local branch ref.
/// Caller has already confirmed this branch should be deleted.
fn delete_pruned_branch(
    branch: &str,
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
) -> Result<(), String> {
    let checked_out_wt = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
        .map(|wt| &wt.path);

    if let Some(wt_path) = checked_out_wt {
        let main_wt = std::path::Path::new(main_wt_path);
        if wt_path == main_wt {
            // Branch is in the main worktree — switch to default branch first.
            let default = crate::git::detect_default_branch(false)?;
            crate::git::checkout_in(main_wt_path, &default)?;
        } else {
            // Branch is in a linked worktree — remove it.
            crate::git::remove_worktree(wt_path, false)?;
            if let Ok(root) = crate::git::read_worktree_root() {
                crate::fs::remove_empty_parent_dirs(wt_path, &root);
            }
            // If the user was inside that worktree, navigate them to main.
            if current_wt.as_deref() == Some(wt_path.as_path()) {
                let canonical = std::fs::canonicalize(main_wt)
                    .map_err(|e| format!("could not canonicalize path: {e}"))?;
                crate::shell_protocol::emit_cd(&canonical);
            }
        }
    }

    crate::git::delete_branch_force_in(main_wt_path, branch)?;
    crate::output::info(&format!("{branch}: deleted (remote was deleted)"));
    Ok(())
}

fn prompt_yes_no(question: &str) -> bool {
    use std::io::Write as _;
    eprint!("{} [y/N] ", question);
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}
