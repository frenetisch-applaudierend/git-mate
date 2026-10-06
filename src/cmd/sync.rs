use super::sync_report::{self as report, BranchOutcome, Candidate, Hint};

#[derive(clap::Args)]
#[command(
    about = "Fetch and merge the latest changes",
    long_about = "Fetch the latest changes from all remotes and bring your local repository up to date.

Fetches all remotes, fast-forwards local branches that haven't diverged from their
upstream, and pulls the current branch. Use --rebase or --ff-only to control how the
pull is applied.

Other local branches that have diverged from their upstream are resolved instead of
skipped. If the remote was rewritten (e.g. rebased and force-pushed) and the local
branch has no work of its own, it is reset to the upstream; the previous tip is printed
and kept in the reflog. If the local branch does have its own commits, the upstream is
merged into it — but only when that merge is conflict-free; otherwise the branch is
left untouched. Pass --diverged=skip (or set mate.divergedStrategy=skip, or use
--ff-only) to skip diverged branches instead.

When a remote branch is deleted — typically after a PR is merged — sync lists every
local branch (and worktree) left without a remote and asks once whether to delete all
of them, keep all of them, or decide branch by branch. Branches with unpushed commits
or a dirty working tree are never offered for deletion. Branches whose remote was
already gone before this sync (e.g. pruned by an earlier fetch) are offered too, but
only if everything on them has landed in the default branch — whether it was merged,
rebase-merged or squash-merged, and even if those files were moved or edited since. Pass --delete-pruned to skip that prompt and delete
all of them (this also makes --json actually delete instead of just reporting
candidates). Branches you keep stop tracking their deleted upstream, so they aren't
offered again.

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
    #[arg(
        long,
        value_enum,
        help = "How to handle other branches that diverged from their upstream [default: merge]"
    )]
    pub diverged: Option<DivergedStrategy>,
}

/// How to bring a diverged non-current branch back in line with its upstream.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum DivergedStrategy {
    /// Reset to a rewritten upstream when nothing local would be lost,
    /// otherwise merge the upstream if that is conflict-free.
    Merge,
    /// Leave diverged branches untouched.
    Skip,
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
    let diverged_strategy = diverged_strategy(&args)?;

    let progress = report::Progress::new(json);

    // 1. Snapshot every local branch's tip and its upstream tip before fetching,
    //    so we can tell whether a branch had unique commits even after the upstream
    //    is pruned away.
    let branches_before = snapshot_branch_upstreams();

    // 2. Fetch everything and prune stale remote-tracking refs.
    progress.update("Fetching…");
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
    // Branches whose remote is gone are checked against the default branch.
    let default_branch = DefaultBranch {
        local: crate::git::detect_default_branch(false).ok(),
        remote: crate::git::detect_default_branch(true).ok(),
    };

    // 4. Process all local branches that have an upstream.
    let branches = crate::git::list_local_branches_with_upstream()?;

    let mut current_branch_pruned = false;
    let mut deletion_candidates: Vec<Candidate> = Vec::new();

    progress.update(&format!("Syncing {} branches…", branches.len()));
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

            let status = if had_unique {
                Ok(PrunedStatus::Skip(BranchOutcome::needs_attention(
                    branch,
                    "remote deleted but has unpushed commits, skipping",
                    "remote gone, but has unpushed commits",
                )))
            } else {
                pruned_branch_status(branch, &worktrees, &default_branch)
            };
            record_pruned_status(
                branch,
                status,
                &mut branch_outcomes,
                &mut deletion_candidates,
            );
        } else if crate::git::resolve_ref(upstream).is_err() {
            // The upstream is configured but its remote-tracking ref was
            // already gone before this fetch (pruned by an earlier fetch, an
            // IDE, or a previous sync where the branch was kept). Without a
            // pre-fetch snapshot, only offer it for deletion if its content
            // is already in the default branch.
            if is_current {
                current_branch_pruned = true;
            }
            let status = gone_branch_status(branch, upstream, &worktrees, &default_branch);
            record_pruned_status(
                branch,
                status,
                &mut branch_outcomes,
                &mut deletion_candidates,
            );
        } else if !is_current {
            // Remote still exists — try to fast-forward, resolving divergence
            // according to the configured strategy. A failure here (e.g. a
            // fast-forward merge conflict) is specific to this branch — report
            // it and move on rather than aborting the remaining branches.
            let upstream_before = branches_before
                .iter()
                .find(|(b, _, _)| b == branch)
                .and_then(|(_, _, upstream_sha)| upstream_sha.as_deref());
            match fast_forward_branch(
                branch,
                upstream,
                upstream_before,
                &worktrees,
                diverged_strategy,
                dry_run,
            ) {
                Ok(outcome) => branch_outcomes.push(outcome),
                Err(e) => branch_outcomes.push(BranchOutcome::failed(branch, &e)),
            }
        }
        // Current branch with live upstream is handled by pull below.
    }
    progress.clear();

    // Print everything at once, grouped by what needs doing, with branch
    // names aligned across all sections.
    let column = report::name_column(
        branch_outcomes
            .iter()
            .filter(|o| report::is_aligned(o))
            .map(|o| o.branch.as_str())
            .chain(deletion_candidates.iter().map(|c| c.branch.as_str())),
    );
    if !json {
        report::print_sections(&branch_outcomes, default_branch.remote.as_deref(), column);
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
        PruneMode {
            json,
            dry_run,
            delete_pruned: args.delete_pruned,
        },
        column,
    )?);

    if !json {
        report::print_summary(&branch_outcomes, dry_run);
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
            "fast-forwarded" | "reset-to-upstream" | "merged" | "deleted" | "deletion-candidate"
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

/// Resolves how diverged non-current branches are handled: `--ff-only` and an
/// explicit `--diverged` win, then `mate.divergedStrategy`, then `merge`.
fn diverged_strategy(args: &SyncArgs) -> Result<DivergedStrategy, String> {
    if args.ff_only {
        return Ok(DivergedStrategy::Skip);
    }
    if let Some(strategy) = args.diverged {
        return Ok(strategy);
    }
    let Some(value) = crate::git::config::read_string("mate.divergedStrategy") else {
        return Ok(DivergedStrategy::Merge);
    };

    <DivergedStrategy as clap::ValueEnum>::from_str(&value, true).map_err(|_| {
        format!(
            "invalid value for mate.divergedStrategy: {value:?}; expected \"merge\" or \"skip\""
        )
    })
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
        return Ok((
            false,
            Some("could not determine current branch".to_string()),
        ));
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
    upstream_before: Option<&str>,
    worktrees: &[crate::git::WorktreeEntry],
    diverged_strategy: DivergedStrategy,
    dry_run: bool,
) -> Result<BranchOutcome, String> {
    let local_sha = match crate::git::resolve_ref(branch) {
        Ok(sha) => sha,
        Err(_) => {
            return Ok(BranchOutcome::needs_attention(
                branch,
                "could not resolve local ref",
                "could not resolve the branch",
            ));
        }
    };
    let remote_sha = match crate::git::resolve_ref(upstream) {
        Ok(sha) => sha,
        Err(_) => {
            let reason = format!("upstream {upstream} could not be resolved");
            return Ok(BranchOutcome::needs_attention(branch, &reason, &reason));
        }
    };
    if local_sha == remote_sha {
        return Ok(BranchOutcome::up_to_date(branch));
    }
    if !crate::git::is_ancestor(&local_sha, &remote_sha)? {
        return resolve_diverged_branch(
            branch,
            upstream,
            &local_sha,
            &remote_sha,
            upstream_before,
            worktrees,
            diverged_strategy,
            dry_run,
        );
    }

    let commits = crate::git::count_commits(&local_sha, &remote_sha)?;
    if dry_run {
        return Ok(BranchOutcome::fast_forwarded(branch, commits, true));
    }

    // If the branch is checked out in a worktree, use `merge --ff-only` so the
    // index and working tree are updated alongside the ref.  Plain `update-ref`
    // would move the ref without touching the worktree, making `git status`
    // report phantom modifications there.
    if let Some(wt) = crate::git::worktree_for_branch(branch, worktrees) {
        let path = wt.path.to_str().ok_or("worktree path is not valid UTF-8")?;
        crate::git::merge_ff_only_in(path, upstream)?;
    } else {
        crate::git::update_ref(&format!("refs/heads/{branch}"), &remote_sha)?;
    }
    Ok(BranchOutcome::fast_forwarded(branch, commits, false))
}

/// Bring a branch that diverged from its upstream back in line without
/// checking it out. If the local branch has no work of its own — it was at or
/// behind the upstream before fetching (the remote was rewritten), or all its
/// patches are already upstream — it is reset to the upstream. Otherwise the
/// upstream is merged in, but only if the merge is conflict-free; a branch is
/// never left half-merged.
#[allow(clippy::too_many_arguments)]
fn resolve_diverged_branch(
    branch: &str,
    upstream: &str,
    local_sha: &str,
    remote_sha: &str,
    upstream_before: Option<&str>,
    worktrees: &[crate::git::WorktreeEntry],
    diverged_strategy: DivergedStrategy,
    dry_run: bool,
) -> Result<BranchOutcome, String> {
    let resolve_hint = || Hint::new("resolve with", format!("git checkout {branch} && git pull"));

    if diverged_strategy == DivergedStrategy::Skip {
        return Ok(BranchOutcome::needs_attention(
            branch,
            "cannot fast-forward (diverged)",
            "cannot fast-forward (diverged)",
        )
        .with_hint(resolve_hint()));
    }

    let worktree_path = match crate::git::worktree_for_branch(branch, worktrees) {
        Some(wt) => Some(
            wt.path
                .to_str()
                .ok_or("worktree path is not valid UTF-8")?
                .to_string(),
        ),
        None => None,
    };
    if let Some(path) = &worktree_path
        && !crate::git::is_worktree_clean(path)?
    {
        return Ok(BranchOutcome::needs_attention(
            branch,
            "diverged but working tree is dirty",
            "diverged, but worktree has uncommitted changes",
        )
        .with_hint(Hint::new("worktree", path.clone())));
    }

    let no_local_work = match upstream_before {
        Some(before) => crate::git::is_ancestor(local_sha, before)?,
        None => false,
    } || crate::git::patches_already_in(remote_sha, local_sha)?;

    let previous_tip = &local_sha[..local_sha.len().min(7)];

    if no_local_work {
        if dry_run {
            return Ok(BranchOutcome::reset_to_upstream(branch, previous_tip, true));
        }
        match &worktree_path {
            Some(path) => crate::git::reset_hard_in(path, upstream)?,
            None => crate::git::update_ref_from(
                &format!("refs/heads/{branch}"),
                remote_sha,
                local_sha,
                &format!("git-mate sync: reset to rewritten {upstream}"),
            )?,
        }
        return Ok(BranchOutcome::reset_to_upstream(
            branch,
            previous_tip,
            false,
        ));
    }

    let Some(tree) = crate::git::merge_tree(local_sha, remote_sha)? else {
        return Ok(BranchOutcome::needs_attention(
            branch,
            "diverged with conflicts",
            "diverged, merging upstream would conflict",
        )
        .with_hint(resolve_hint()));
    };

    if dry_run {
        return Ok(BranchOutcome::merged(branch, true));
    }

    match &worktree_path {
        // Merge inside the worktree so its index and files follow the ref.
        Some(path) => crate::git::merge_no_edit_in(path, upstream)?,
        None => {
            let message = format!("Merge remote-tracking branch '{upstream}' into {branch}");
            let merge_sha = crate::git::commit_tree(&tree, &[local_sha, remote_sha], &message)?;
            crate::git::update_ref_from(
                &format!("refs/heads/{branch}"),
                &merge_sha,
                local_sha,
                &format!("git-mate sync: merge {upstream}"),
            )?;
        }
    }
    Ok(BranchOutcome::merged(branch, false))
}

/// The default branch, by local name (`main`) and as the ref to compare
/// against (`origin/main`). Either is `None` if it could not be detected.
struct DefaultBranch {
    local: Option<String>,
    remote: Option<String>,
}

enum PrunedStatus {
    /// Not offered for deletion, for the reason the outcome records.
    Skip(BranchOutcome),
    Eligible(Candidate),
}

/// Record a pruned or gone branch's status and, if eligible, queue it for
/// the deletion prompt. A failure is specific to this branch — report it and
/// move on rather than aborting the remaining branches.
fn record_pruned_status(
    branch: &str,
    status: Result<PrunedStatus, String>,
    outcomes: &mut Vec<BranchOutcome>,
    deletion_candidates: &mut Vec<Candidate>,
) {
    match status {
        Ok(PrunedStatus::Skip(outcome)) => outcomes.push(outcome),
        Ok(PrunedStatus::Eligible(candidate)) => deletion_candidates.push(candidate),
        Err(e) => outcomes.push(BranchOutcome::failed(branch, &e)),
    }
}

/// Decide whether a branch whose upstream was gone before this sync is safe
/// to delete: only if everything on it is already in the default branch
/// (merged, rebase-merged or squash-merged). Otherwise it is left alone.
fn gone_branch_status(
    branch: &str,
    upstream: &str,
    worktrees: &[crate::git::WorktreeEntry],
    default_branch: &DefaultBranch,
) -> Result<PrunedStatus, String> {
    let (Some(local), Some(remote)) = (&default_branch.local, &default_branch.remote) else {
        return Err("could not detect the default branch to compare against".to_string());
    };
    if branch == local {
        let reason = format!("upstream {upstream} is gone");
        return Ok(PrunedStatus::Skip(
            BranchOutcome::needs_attention(branch, &format!("{reason}, skipping"), &reason)
                .with_hint(Hint::new(
                    "fix with",
                    format!("git branch --set-upstream-to <remote>/{branch} {branch}"),
                )),
        ));
    }
    let Some(merged_at) = crate::git::content_merged_into(remote, branch)? else {
        return Ok(PrunedStatus::Skip(BranchOutcome::not_merged(
            branch,
            &format!(
                "upstream {upstream} is gone and branch has changes not in {remote}, skipping"
            ),
        )));
    };
    deletable_unless_dirty(
        branch,
        worktrees,
        Candidate {
            branch: branch.to_string(),
            detail: merged_detail(&merged_at, remote),
            warn: false,
        },
    )
}

/// Decide whether a branch whose upstream was pruned during this sync (and
/// that has no unpushed commits) is safe to delete. Whether it was merged is
/// only shown, so the user can tell a merged PR from an abandoned one.
fn pruned_branch_status(
    branch: &str,
    worktrees: &[crate::git::WorktreeEntry],
    default_branch: &DefaultBranch,
) -> Result<PrunedStatus, String> {
    let merged_at = default_branch
        .remote
        .as_deref()
        .map(|remote| (remote, crate::git::content_merged_into(remote, branch)));
    let candidate = match merged_at {
        Some((remote, Ok(Some(merged_at)))) => Candidate {
            branch: branch.to_string(),
            detail: merged_detail(&merged_at, remote),
            warn: false,
        },
        Some((remote, Ok(None))) => Candidate {
            branch: branch.to_string(),
            detail: format!("not merged into {remote}"),
            warn: true,
        },
        _ => Candidate {
            branch: branch.to_string(),
            detail: "remote deleted".to_string(),
            warn: false,
        },
    };
    deletable_unless_dirty(branch, worktrees, candidate)
}

/// A branch checked out in a worktree with uncommitted changes is never a
/// deletion candidate; deleting it would remove the worktree.
fn deletable_unless_dirty(
    branch: &str,
    worktrees: &[crate::git::WorktreeEntry],
    candidate: Candidate,
) -> Result<PrunedStatus, String> {
    let checked_out_wt = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
        .map(|wt| &wt.path);

    if let Some(wt_path) = checked_out_wt {
        let wt_str = wt_path.to_str().ok_or("worktree path is not valid UTF-8")?;
        if !crate::git::is_worktree_clean(wt_str)? {
            return Ok(PrunedStatus::Skip(
                BranchOutcome::needs_attention(
                    branch,
                    "remote deleted but working tree is dirty, skipping",
                    &format!("{}, but worktree has uncommitted changes", candidate.detail),
                )
                .with_hint(Hint::new("worktree", wt_str)),
            ));
        }
    }

    Ok(PrunedStatus::Eligible(candidate))
}

/// "merged in #1156" if the merging commit names a PR, else its short hash.
fn merged_detail(merged_at: &crate::git::MergedAt, default_branch: &str) -> String {
    match merged_at {
        crate::git::MergedAt::Contained => format!("already in {default_branch}"),
        crate::git::MergedAt::Unknown => format!("merged into {default_branch}"),
        crate::git::MergedAt::Commit(sha) => match crate::git::commit_summary(sha) {
            Ok((short, subject)) => match pull_request_number(&subject) {
                Some(pr) => format!("merged in #{pr}"),
                None => format!("merged in {short}"),
            },
            Err(_) => format!("merged into {default_branch}"),
        },
    }
}

/// The PR number GitHub appends to squash and merge commit subjects:
/// "Fix the thing (#1234)" or "Merge pull request #1234 from …".
fn pull_request_number(subject: &str) -> Option<&str> {
    let start = subject.rfind("(#").map(|i| i + 2).or_else(|| {
        subject
            .strip_prefix("Merge pull request #")
            .map(|_| "Merge pull request #".len())
    })?;
    let digits = &subject[start..];
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    (end > 0).then(|| &digits[..end])
}

/// How the deletion of branches whose remote is gone is resolved.
struct PruneMode {
    json: bool,
    dry_run: bool,
    delete_pruned: bool,
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
    candidates: &[Candidate],
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
    mode: PruneMode,
    column: usize,
) -> Result<Vec<BranchOutcome>, String> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let as_candidates = || {
        candidates
            .iter()
            .map(|c| BranchOutcome::deletion_candidate(&c.branch, "remote deleted"))
            .collect()
    };

    if mode.dry_run {
        if !mode.json {
            report::print_candidates("Remote gone, would be deleted", candidates, column);
            report::print_note("dry run: not deleting anything");
        }
        return Ok(as_candidates());
    }

    if mode.delete_pruned {
        if !mode.json {
            report::print_candidates("Remote gone, deleted", candidates, column);
        }
        return Ok(candidates
            .iter()
            .map(|c| {
                delete_candidate(
                    &c.branch,
                    current_wt,
                    worktrees,
                    main_wt_path,
                    mode.json,
                    column,
                )
            })
            .collect());
    }

    if mode.json {
        return Ok(as_candidates());
    }

    report::print_candidates("Remote gone, can be deleted", candidates, column);
    let to_delete: Vec<&str> = match prompt_bulk_choice(candidates.len()) {
        BulkChoice::All => candidates.iter().map(|c| c.branch.as_str()).collect(),
        BulkChoice::None => Vec::new(),
        BulkChoice::Decide => candidates
            .iter()
            .filter(|c| prompt_yes_no(&format!("   Delete {}?", c.branch)))
            .map(|c| c.branch.as_str())
            .collect(),
    };

    let mut outcomes = Vec::with_capacity(candidates.len());
    let mut kept = 0;
    for candidate in candidates {
        let branch = candidate.branch.as_str();
        if to_delete.contains(&branch) {
            outcomes.push(delete_candidate(
                branch,
                current_wt,
                worktrees,
                main_wt_path,
                false,
                column,
            ));
        } else {
            outcomes.push(keep_pruned_branch(branch, column));
            kept += 1;
        }
    }
    if kept > 0 {
        let plural = if kept == 1 { "" } else { "es" };
        report::print_note(&format!(
            "kept {kept} branch{plural}; stopped tracking the deleted upstream so sync won't ask again"
        ));
    }

    Ok(outcomes)
}

fn delete_candidate(
    branch: &str,
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
    json: bool,
    column: usize,
) -> BranchOutcome {
    match delete_pruned_branch(branch, current_wt, worktrees, main_wt_path) {
        Ok(()) => BranchOutcome::deleted(branch),
        Err(e) => {
            if !json {
                report::print_failure(branch, &e, column);
            }
            BranchOutcome::failed(branch, &e)
        }
    }
}

/// Keep a branch whose remote was deleted and stop tracking the missing
/// upstream, so it moves to the "no upstream" bucket instead of being
/// offered for deletion again on every sync.
fn keep_pruned_branch(branch: &str, column: usize) -> BranchOutcome {
    match crate::git::unset_upstream(branch) {
        Ok(()) => BranchOutcome::kept(branch, Some("upstream unset")),
        Err(e) => {
            let reason = format!("could not unset upstream: {e}");
            report::print_failure(branch, &reason, column);
            BranchOutcome::kept(branch, Some(&reason))
        }
    }
}

enum BulkChoice {
    All,
    None,
    Decide,
}

fn prompt_bulk_choice(count: usize) -> BulkChoice {
    use std::io::Write as _;
    let what = if count == 1 {
        "this branch".to_string()
    } else {
        format!("these {count} branches")
    };
    eprint!(
        " Delete {what}? {} ",
        console::style("[a]ll / [N]one / [d]ecide").dim()
    );
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return BulkChoice::None;
    }
    end_prompt_line();
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
    end_prompt_line();
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Without a terminal the typed answer (and its newline) is never echoed, so
/// end the prompt's line ourselves before printing anything else.
fn end_prompt_line() {
    use std::io::IsTerminal as _;
    if !std::io::stdin().is_terminal() {
        eprintln!();
    }
}

#[cfg(test)]
mod tests {
    use super::pull_request_number;

    #[test]
    fn pull_request_number_from_squash_subject() {
        assert_eq!(pull_request_number("Fix the thing (#1234)"), Some("1234"));
    }

    #[test]
    fn pull_request_number_from_merge_subject() {
        assert_eq!(
            pull_request_number("Merge pull request #77 from org/branch"),
            Some("77")
        );
    }

    #[test]
    fn pull_request_number_absent() {
        assert_eq!(pull_request_number("Plain commit"), None);
        assert_eq!(pull_request_number("Odd (#) subject"), None);
    }
}
