use super::sync_chooser;
use super::sync_report::{
    self as report, BranchOutcome, Candidate, Choice, DecidedBy, DecisionRow, Hint,
};

#[derive(clap::Args)]
#[command(
    about = "Fetch and merge the latest changes",
    long_about = "Fetch the latest changes from all remotes and bring your local repository up to date.

Fetches all remotes and fast-forwards every local branch that hasn't diverged from
its upstream, including the current one.

Branches that have diverged from their upstream are resolved instead of skipped. If the remote was rewritten (e.g. rebased and force-pushed) and the local
branch has no work of its own, it is reset to the upstream; the previous tip is printed
and kept in the reflog. If the local branch does have its own commits, the upstream is
merged into it — but only when that merge is conflict-free; otherwise the branch is
left untouched. Pass --diverged=skip (or set mate.divergedStrategy=skip, or use
--ff-only) to skip diverged branches instead. Pass --rebase (or set pull.rebase=true)
to rebase the current branch onto its upstream instead of merging. A branch whose
working tree has uncommitted changes in the way, or whose rebase would conflict, is
left as it was and listed under \"Needs attention\".

When a remote branch is deleted — typically after a PR is merged — sync lists every
local branch (and worktree) left without a remote, each with an action to choose:
delete it, or keep it and stop tracking the deleted remote. Branches already merged
into the default branch start out as delete, the others as keep; use the arrow keys
to change them and Enter to apply, or Esc to leave them all as they are. Without a
terminal there is nobody to ask, so they are listed and left as they are. Branches
with unpushed commits or a dirty working tree are never offered for deletion. Branches whose remote was
already gone before this sync (e.g. pruned by an earlier fetch) are offered too, but
only if everything on them has landed in the default branch — whether it was merged,
rebase-merged or squash-merged, and even if those files were moved or edited since.
Pass --apply-recommended to skip the chooser and apply the preselected actions, or
--delete-pruned to delete all of them (either also makes --json act instead of just
reporting candidates). Branches you keep stop tracking their deleted upstream, so they
aren't offered again.

Pass --merge (or set mate.autoMerge=true in git config) to also merge the default
branch into the current branch after syncing it, keeping feature branches up to date
with main. The merge only happens when it is conflict-free.

Pass --dry-run to preview what sync would do (fast-forwards, deletion candidates,
pull, auto-merge) without changing anything or prompting."
)]
pub struct SyncArgs {
    #[arg(
        long,
        help = "Rebase the current branch onto its upstream if it diverged"
    )]
    pub rebase: bool,
    #[arg(long, help = "Only fast-forward; skip diverged branches")]
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
        conflicts_with_all = ["dry_run", "delete_pruned"],
        help = "Delete merged branches whose remote was pruned and keep the rest, without prompting"
    )]
    pub apply_recommended: bool,
    #[arg(
        long,
        help = "Preview planned actions without making any changes or prompting"
    )]
    pub dry_run: bool,
    #[arg(
        long,
        value_enum,
        help = "How to handle branches that diverged from their upstream [default: merge]"
    )]
    pub diverged: Option<DivergedStrategy>,
}

/// How to bring a diverged branch back in line with its upstream.
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
    let rebase_current = rebase_current(&args);

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
        } else {
            // Remote still exists — try to fast-forward, resolving divergence
            // according to the configured strategy. The current branch is
            // treated like any other: it is checked out in a worktree, so the
            // worktree-aware paths apply. A failure here (e.g. a fast-forward
            // merge conflict) is specific to this branch — report it and move
            // on rather than aborting the remaining branches.
            let upstream_before = branches_before
                .iter()
                .find(|(b, _, _)| b == branch)
                .and_then(|(_, _, upstream_sha)| upstream_sha.as_deref());
            let target = SyncTarget {
                branch,
                upstream,
                upstream_before,
                is_current,
                rebase: is_current && rebase_current,
            };
            match fast_forward_branch(&target, &worktrees, diverged_strategy, dry_run) {
                Ok(outcome) => branch_outcomes.push(outcome),
                Err(e) => branch_outcomes
                    .push(BranchOutcome::failed(branch, &e).decided_by(DecidedBy::Auto)),
            }
        }
    }

    // 5. Merge the default branch into the current branch, if enabled. This
    //    happens before the report so its outcome is listed with the rest.
    let mut merge_skipped_reason = None;
    let mut merge_skipped_note = None;
    let mut merged_default = false;
    if !current_branch_pruned && auto_merge_enabled(&args)? {
        match merge_default_branch(current_branch.as_deref(), current_wt.as_deref(), dry_run) {
            DefaultMerge::Done(outcome) => {
                merged_default = outcome.action == "merged-default";
                if !merged_default {
                    merge_skipped_reason = outcome.reason.clone();
                }
                branch_outcomes.push(outcome);
            }
            DefaultMerge::Skipped(reason) => {
                merge_skipped_note = Some(reason.clone());
                merge_skipped_reason = Some(reason);
            }
        }
    }
    progress.clear();

    // Print everything at once, grouped by what needs doing, with branch
    // names aligned across all sections.
    let column = report::name_column(
        branch_outcomes
            .iter()
            .filter(|o| report::is_aligned(o))
            .map(|o| (o.branch.as_str(), report::detail(o)))
            .chain(
                deletion_candidates
                    .iter()
                    .map(|c| (c.branch.as_str(), c.detail.as_str())),
            ),
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
            apply_recommended: args.apply_recommended,
        },
        column,
    )?);

    if !json {
        report::print_summary(&branch_outcomes, dry_run);
    }

    let current_outcome = current_branch
        .as_deref()
        .and_then(|name| branch_outcomes.iter().find(|o| o.branch == name));
    let (pulled, pull_skipped_reason) = if current_branch_pruned {
        (
            false,
            Some("current branch's upstream was deleted".to_string()),
        )
    } else {
        match current_outcome {
            Some(o) if is_synced(o) => (true, None),
            Some(o) if o.action == "no-upstream" => (
                false,
                Some("no upstream configured for current branch".to_string()),
            ),
            Some(o) => (false, o.reason.clone()),
            None => (
                false,
                Some("no upstream configured for current branch".to_string()),
            ),
        }
    };

    if !json {
        if !current_branch_pruned && current_outcome.is_none_or(|o| o.action == "no-upstream") {
            crate::output::info("No upstream configured for current branch, skipping sync.");
        }
        if let Some(reason) = &merge_skipped_note {
            crate::output::info(&format!("Skipping auto-merge: {reason}."));
        }
        let any_work = any_branch_action_taken(&branch_outcomes);
        // "Already up to date" would contradict a "Needs attention" entry.
        let needs_attention = branch_outcomes
            .iter()
            .any(|o| o.group == report::Group::Attention);
        if any_work || !needs_attention {
            crate::output::success(final_status_message(any_work, dry_run));
        }
    }

    if json {
        crate::output::emit_json_success(
            "sync",
            SyncData {
                dry_run,
                branches: branch_outcomes,
                current_branch: CurrentBranchOutcome {
                    name: current_branch,
                    pulled,
                    pull_skipped_reason,
                    merged_default,
                    merge_skipped_reason,
                },
            },
        );
    }

    Ok(())
}

/// Whether a branch is in line with its upstream after this run (or, under
/// `--dry-run`, would be).
fn is_synced(outcome: &BranchOutcome) -> bool {
    matches!(
        outcome.action.as_str(),
        "up-to-date" | "fast-forwarded" | "reset-to-upstream" | "merged" | "rebased"
    )
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
            "fast-forwarded"
                | "reset-to-upstream"
                | "merged"
                | "rebased"
                | "merged-default"
                | "deleted"
                | "deletion-candidate"
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

/// Whether a diverged current branch is rebased rather than merged:
/// `--rebase`, or `pull.rebase` set to anything but false (as `git pull`
/// would).
fn rebase_current(args: &SyncArgs) -> bool {
    args.rebase
        || crate::git::config::read_string("pull.rebase").is_some_and(|value| {
            !matches!(value.to_lowercase().as_str(), "false" | "no" | "off" | "0")
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

enum DefaultMerge {
    /// Merged, or (for a reason the user should look at) not merged.
    Done(BranchOutcome),
    /// Not applicable, e.g. the current branch is the default branch.
    Skipped(String),
}

/// Merge the default branch into the current branch, but only if that is
/// conflict-free; otherwise nothing is touched and the branch is reported
/// as needing attention.
fn merge_default_branch(
    current_branch: Option<&str>,
    current_wt: Option<&std::path::Path>,
    dry_run: bool,
) -> DefaultMerge {
    let Some(current_branch) = current_branch else {
        return DefaultMerge::Skipped("could not determine current branch".to_string());
    };
    if current_branch == "HEAD" {
        return DefaultMerge::Skipped("detached HEAD".to_string());
    }
    let Ok(default_branch) = crate::git::detect_default_branch(false) else {
        return DefaultMerge::Skipped("could not detect the default branch".to_string());
    };
    if current_branch == default_branch {
        return DefaultMerge::Skipped("current branch is the default branch".to_string());
    }
    let Some(path) = current_wt.and_then(|p| p.to_str()) else {
        return DefaultMerge::Skipped("could not determine the current worktree".to_string());
    };

    let resolve_hint = || Hint::new("resolve with", format!("git merge {default_branch}"));
    let conflicts = match crate::git::merge_tree("HEAD", &default_branch) {
        Ok(tree) => tree.is_none(),
        Err(e) => {
            return DefaultMerge::Done(
                BranchOutcome::failed(current_branch, &e).decided_by(DecidedBy::Auto),
            );
        }
    };
    if conflicts {
        return DefaultMerge::Done(
            BranchOutcome::needs_attention(
                current_branch,
                &format!("merging default branch '{default_branch}' would conflict"),
                &format!("merging {default_branch} would conflict"),
            )
            .with_hint(resolve_hint()),
        );
    }

    if !dry_run && let Err(e) = crate::git::merge_no_edit_in(path, &default_branch) {
        let first_line = e.lines().next().unwrap_or(&e).to_string();
        return DefaultMerge::Done(
            BranchOutcome::needs_attention(
                current_branch,
                &format!("could not merge default branch '{default_branch}': {e}"),
                &format!("could not merge {default_branch}: {first_line}"),
            )
            .with_hint(resolve_hint()),
        );
    }
    DefaultMerge::Done(BranchOutcome::merged_default(
        current_branch,
        &default_branch,
        dry_run,
    ))
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

/// A branch whose upstream still exists, to bring in line with it.
struct SyncTarget<'a> {
    branch: &'a str,
    upstream: &'a str,
    /// The upstream's tip before this sync fetched.
    upstream_before: Option<&'a str>,
    /// The branch `sync` was run from.
    is_current: bool,
    /// Rebase onto the upstream instead of merging it when diverged.
    rebase: bool,
}

impl SyncTarget<'_> {
    /// The command that finishes what sync could not, from where the user is.
    fn pull_command(&self) -> String {
        let pull = if self.rebase {
            "git pull --rebase"
        } else {
            "git pull"
        };
        if self.is_current {
            pull.to_string()
        } else {
            format!("git checkout {} && {pull}", self.branch)
        }
    }

    /// Uncommitted changes are in the way: point at them.
    fn dirty_hint(&self, worktree: &str) -> Hint {
        if self.is_current {
            Hint::new("commit or stash, then", self.pull_command())
        } else {
            Hint::new("worktree", worktree)
        }
    }
}

fn fast_forward_branch(
    target: &SyncTarget,
    worktrees: &[crate::git::WorktreeEntry],
    diverged_strategy: DivergedStrategy,
    dry_run: bool,
) -> Result<BranchOutcome, String> {
    let branch = target.branch;
    let upstream = target.upstream;
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
    if crate::git::is_ancestor(&remote_sha, &local_sha)? {
        // Only local commits on top of the upstream: nothing to bring in.
        let commits = crate::git::count_commits(&remote_sha, &local_sha)?;
        return Ok(BranchOutcome::ahead(branch, commits));
    }
    if !crate::git::is_ancestor(&local_sha, &remote_sha)? {
        return resolve_diverged_branch(
            target,
            &local_sha,
            &remote_sha,
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
        if let Err(e) = crate::git::merge_ff_only_in(path, upstream) {
            // Typically uncommitted changes the upstream would overwrite.
            // Nothing was changed, so this is for the user to sort out.
            let first_line = e.lines().next().unwrap_or(&e).to_string();
            return Ok(BranchOutcome::needs_attention(
                branch,
                &format!("could not fast-forward: {e}"),
                &format!("could not fast-forward: {first_line}"),
            )
            .with_hint(target.dirty_hint(path)));
        }
    } else {
        crate::git::update_ref(&format!("refs/heads/{branch}"), &remote_sha)?;
    }
    Ok(BranchOutcome::fast_forwarded(branch, commits, false))
}

/// Bring a branch that diverged from its upstream back in line, without
/// checking it out. If the local branch has no work of its own — it was at or
/// behind the upstream before fetching (the remote was rewritten), or all its
/// patches are already upstream — it is reset to the upstream. Otherwise the
/// current branch is rebased onto the upstream if asked to, and any other
/// branch has the upstream merged in, but only if the merge is conflict-free.
/// A branch is never left half-merged or half-rebased.
fn resolve_diverged_branch(
    target: &SyncTarget,
    local_sha: &str,
    remote_sha: &str,
    worktrees: &[crate::git::WorktreeEntry],
    diverged_strategy: DivergedStrategy,
    dry_run: bool,
) -> Result<BranchOutcome, String> {
    let branch = target.branch;
    let upstream = target.upstream;
    let resolve_hint = || Hint::new("resolve with", target.pull_command());

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
        .with_hint(target.dirty_hint(path)));
    }

    let no_local_work = match target.upstream_before {
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

    if target.rebase
        && let Some(path) = &worktree_path
    {
        if dry_run {
            return Ok(BranchOutcome::rebased(branch, true));
        }
        if crate::git::rebase_in(path, upstream).is_err() {
            return Ok(BranchOutcome::needs_attention(
                branch,
                "diverged with conflicts; rebase aborted",
                "diverged, rebasing onto upstream would conflict",
            )
            .with_hint(resolve_hint()));
        }
        return Ok(BranchOutcome::rebased(branch, false));
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
            recommended: Choice::Delete,
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
            recommended: Choice::Delete,
        },
        Some((remote, Ok(None))) => Candidate {
            branch: branch.to_string(),
            detail: format!("not merged into {remote}"),
            warn: true,
            recommended: Choice::Keep,
        },
        _ => Candidate {
            branch: branch.to_string(),
            detail: "remote deleted".to_string(),
            warn: false,
            recommended: Choice::Keep,
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
    apply_recommended: bool,
}

/// Resolves what to do with branches whose remote was deleted, in priority
/// order: `--dry-run` always previews only, regardless of the other flags;
/// then `--delete-pruned` deletes all candidates and `--apply-recommended`
/// applies each one's recommended action, both without prompting (this is
/// what makes `--json` actually act rather than just report); otherwise
/// plain `--json` never prompts (which would block on stdin) and reports
/// candidates without acting; otherwise let the user pick an action per
/// branch in the chooser, or, without a terminal for it, leave them all as
/// they are.
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
            .map(BranchOutcome::deletion_candidate)
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
            report::print_title("Remote gone, deleted");
        }
        return Ok(candidates
            .iter()
            .map(|c| {
                delete_candidate(c, current_wt, worktrees, main_wt_path, mode.json, column)
                    .decided_by(DecidedBy::Flag)
            })
            .collect());
    }

    let apply = |candidate: &Candidate, choice: Choice| {
        apply_choice(candidate, choice, current_wt, worktrees, main_wt_path)
    };

    if mode.apply_recommended {
        if !mode.json {
            report::print_title("Remote gone");
        }
        let detail_column = report::detail_column(candidates);
        let rows: Vec<DecisionRow> = candidates
            .iter()
            .map(|candidate| {
                let row = apply(candidate, candidate.recommended);
                if !mode.json {
                    for line in
                        report::decision_lines(candidate, &row, false, column, detail_column)
                    {
                        eprintln!("{line}");
                    }
                }
                row
            })
            .collect();
        if !mode.json {
            print_kept_note(rows.iter().filter(|r| **r == DecisionRow::Kept).count());
        }
        return Ok(candidates
            .iter()
            .zip(&rows)
            .map(|(candidate, row)| decision_outcome(candidate, row, DecidedBy::Flag))
            .collect());
    }

    if mode.json {
        return Ok(as_candidates());
    }

    let chosen = sync_chooser::resolve(candidates, column, apply);
    let Some(rows) = chosen else {
        // Without a terminal nobody can answer a prompt: leave every branch
        // as it is and say how to resolve them.
        report::print_title("Remote gone");
        let detail_column = report::detail_column(candidates);
        for candidate in candidates {
            for line in
                report::decision_lines(candidate, &DecisionRow::Left, false, column, detail_column)
            {
                eprintln!("{line}");
            }
        }
        report::print_note(
            "no terminal to choose in; run sync in one, or pass --apply-recommended or --delete-pruned",
        );
        return Ok(candidates.iter().map(BranchOutcome::left_as_is).collect());
    };
    let kept = rows.iter().filter(|r| **r == DecisionRow::Kept).count();
    print_kept_note(kept);
    Ok(candidates
        .iter()
        .zip(&rows)
        .map(|(candidate, row)| decision_outcome(candidate, row, DecidedBy::User))
        .collect())
}

/// Delete a branch whose remote is gone, or keep it and stop tracking the
/// missing upstream so it isn't offered again.
fn apply_choice(
    candidate: &Candidate,
    choice: Choice,
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
) -> DecisionRow {
    let branch = candidate.branch.as_str();
    match choice {
        Choice::Delete => match delete_pruned_branch(branch, current_wt, worktrees, main_wt_path) {
            Ok(()) => DecisionRow::Deleted,
            Err(e) => DecisionRow::Failed(e),
        },
        Choice::Keep => match crate::git::unset_upstream(branch) {
            Ok(()) => DecisionRow::Kept,
            Err(e) => DecisionRow::Failed(format!("could not unset upstream: {e}")),
        },
    }
}

fn print_kept_note(kept: usize) {
    if kept > 0 {
        let plural = if kept == 1 { "" } else { "es" };
        report::print_note(&format!(
            "kept {kept} branch{plural}; stopped tracking the deleted upstream so sync won't ask again"
        ));
    }
}

/// The outcome of a resolved row.
fn decision_outcome(
    candidate: &Candidate,
    row: &DecisionRow,
    decided_by: DecidedBy,
) -> BranchOutcome {
    let outcome = match row {
        DecisionRow::Deleted => BranchOutcome::deleted(candidate),
        DecisionRow::Kept => BranchOutcome::kept(candidate),
        DecisionRow::Failed(e) => BranchOutcome::failed(&candidate.branch, e),
        DecisionRow::Left | DecisionRow::Choice(_) | DecisionRow::Applying(_) => {
            BranchOutcome::left_as_is(candidate)
        }
    };
    outcome.decided_by(decided_by)
}

fn delete_candidate(
    candidate: &Candidate,
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
    json: bool,
    column: usize,
) -> BranchOutcome {
    let branch = candidate.branch.as_str();
    match delete_pruned_branch(branch, current_wt, worktrees, main_wt_path) {
        Ok(()) => {
            if !json {
                report::print_deleted(candidate, column);
            }
            BranchOutcome::deleted(candidate)
        }
        Err(e) => {
            if !json {
                report::print_failure(branch, &e, column);
            }
            BranchOutcome::failed(branch, &e)
        }
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
