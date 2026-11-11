use super::sync_chooser;
use super::sync_report::{
    self as report, BranchOutcome, Candidate, CandidateKind, Choice, DecidedBy, DecisionRow, Hint,
    NO_UPSTREAM_CHOICES, NO_UPSTREAM_DIRTY_CHOICES, REMOTE_GONE_CHOICES, Unpushed,
};

#[derive(clap::Args, Default)]
#[command(
    about = "Fetch and bring local branches up to date with their remotes",
    long_about = "Fetch the latest changes from all remotes and bring your local repository up to date.

Fetches all remotes and fast-forwards every local branch that hasn't diverged from
its upstream, including the current one.

Branches that have diverged from their upstream are resolved instead of skipped. If the
remote was rewritten (e.g. rebased and force-pushed) and the local branch has no work of
its own, it is reset to the upstream; the previous tip is printed and kept in the reflog.
If the local branch does have its own commits, the upstream is merged into it — but only
when that merge is conflict-free; otherwise the branch is left untouched. Pass
--diverged=rebase (or set mate.sync.diverged=rebase) to rebase local commits onto the
upstream instead, or --diverged=skip to leave diverged branches alone. A branch that was
rebased locally but not pushed yet (every upstream commit has an equivalent local one) is
never merged or reset: it is listed as needing a force-push. A branch whose working tree
has uncommitted changes in the way, or whose merge or rebase would conflict, is left as
it was and listed under \"Needs attention\".

When a remote branch is deleted — typically after a PR is merged — sync lists every
local branch (and worktree) left without a remote, each with an action to choose:
delete it, keep it and stop tracking the deleted remote, or ask again next time.
Branches already merged into the default branch — whether merged, rebase-merged or
squash-merged, even if those files were moved or edited since — start out as delete,
the others as ask later; use the arrow keys to change them and Enter to apply, or Esc
to leave them all as they are. Without a terminal there is nobody to ask, so they are
listed and left as they are. A branch with unpushed commits is treated like a merged
one if everything on it is already in the default branch (e.g. the PR was squash-merged
and the unpushed commit only merged the default branch in); otherwise it starts out as
ask later and is only deleted when you pick it; keep it to push it again later with
`git push -u origin <branch>`. Under --json, `unpushedCommits` counts such commits (null if unknown).
Branches whose worktree has uncommitted changes are never offered for deletion.
Branches whose remote was already gone before this sync (e.g. pruned by an earlier
fetch) are offered too; since sync cannot tell whether they had unpushed commits, an
unmerged one is only deleted when you pick it. Pass --apply-recommended to skip the
chooser and apply the preselected actions, or --delete-pruned to delete all of them
except unmerged ones with unpushed commits or whose remote was already gone (either
also makes --json act instead of just reporting candidates). To decide per branch
without the chooser, name branches with --delete <branch> and --keep <branch> (both
repeatable); with --apply-recommended the others get their preselected action, otherwise
they are left as they are (and, under --json, reported as candidates). Branches you keep
stop tracking their deleted upstream, so they aren't offered again.

Local branches without an upstream (never pushed, or pushed without -u) are listed
next, each with an action to choose: push it (setting the pushed branch as its
upstream), delete it, keep it local (sync stops asking about it), or ask again next
time. One whose commits are already in the default branch starts out as delete, the
others as ask later; one whose worktree has uncommitted changes can't be deleted.
--push <branch>, --delete <branch> and --keep <branch> decide for them without the
chooser, and --apply-recommended applies the preselected action to the others. Under
--json, they are reported as `no-upstream` with `detail`, `recommended` and `choices`.
The default branch and repositories without a remote are left out.

Sync never merges or rebases a branch onto the branch it is based on; if the current
branch is behind its base, it suggests `git mate update`, which does.

Pass --dry-run to preview what sync would do (fast-forwards, merges, rebases, deletion
candidates) without changing anything or prompting."
)]
pub struct SyncArgs {
    #[arg(
        long,
        conflicts_with = "dry_run",
        help = "Delete every local branch whose remote was pruned, without prompting"
    )]
    pub delete_pruned: bool,
    #[arg(
        long,
        conflicts_with_all = ["dry_run", "delete_pruned"],
        help = "Delete merged branches whose remote was pruned and leave the rest, without prompting"
    )]
    pub apply_recommended: bool,
    #[arg(
        long = "delete",
        value_name = "BRANCH",
        conflicts_with_all = ["dry_run", "delete_pruned"],
        help = "Delete this branch if its remote is gone or it has no upstream, without prompting (repeatable)"
    )]
    pub delete: Vec<String>,
    #[arg(
        long = "keep",
        value_name = "BRANCH",
        conflicts_with_all = ["dry_run", "delete_pruned"],
        help = "Keep this branch if its remote is gone or it has no upstream, and stop asking about it, without prompting (repeatable)"
    )]
    pub keep: Vec<String>,
    #[arg(
        long = "push",
        value_name = "BRANCH",
        conflicts_with_all = ["dry_run", "delete_pruned"],
        help = "Push this branch if it has no upstream and set the pushed branch as its upstream, without prompting (repeatable)"
    )]
    pub push: Vec<String>,
    #[arg(
        long,
        help = "Preview planned actions without making any changes or prompting"
    )]
    pub dry_run: bool,
    #[arg(
        long,
        value_enum,
        help = "How to handle branches that diverged from their upstream [default: mate.sync.diverged, else merge]"
    )]
    pub diverged: Option<DivergedStrategy>,
    /// Don't check whether the current branch is behind its base (when
    /// `git mate update --sync` is about to update it anyway).
    #[arg(skip)]
    pub skip_behind_base: bool,
}

/// How to bring a diverged branch back in line with its upstream.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum DivergedStrategy {
    /// Reset to a rewritten upstream when nothing local would be lost,
    /// otherwise merge the upstream if that is conflict-free.
    Merge,
    /// Like `merge`, but rebase local commits onto the upstream instead of
    /// merging it, if that is conflict-free.
    Rebase,
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
    /// The current branch's base has commits it doesn't, so
    /// `git mate update` would update it.
    #[serde(skip_serializing_if = "Option::is_none")]
    behind_base: Option<BehindBase>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BehindBase {
    base: String,
    count: usize,
    /// The branch it was stacked on, since merged into `base`.
    #[serde(skip_serializing_if = "Option::is_none")]
    merged_parent: Option<String>,
}

/// What sync did, as reported by `--json`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncData {
    dry_run: bool,
    branches: Vec<BranchOutcome>,
    current_branch: CurrentBranchOutcome,
    /// Branches named by `--delete`, `--keep` or `--push` that weren't up for
    /// a decision (their remote still exists, or they need attention); left
    /// alone.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ignored_decisions: Vec<String>,
}

pub fn run(args: SyncArgs) -> Result<(), crate::output::CommandError> {
    validate_decisions(&args)?;
    let data = sync(args)?;
    if crate::output::json_mode() {
        crate::output::emit_json_success("sync", data);
    }
    Ok(())
}

/// Fail before changing anything when `--delete`/`--keep`/`--push` name a
/// branch that doesn't exist, or the same branch with different actions.
fn validate_decisions(args: &SyncArgs) -> Result<(), crate::output::CommandError> {
    for branch in args.delete.iter().chain(&args.keep).chain(&args.push) {
        if !crate::git::commit_exists(&format!("refs/heads/{branch}")) {
            return Err(crate::output::CommandError::branch(
                format!("branch {branch} not found"),
                "branch_not_found",
                branch,
            ));
        }
    }
    let named = [
        ("--delete", &args.delete),
        ("--keep", &args.keep),
        ("--push", &args.push),
    ];
    for (i, (flag, branches)) in named.iter().enumerate() {
        for (other, other_branches) in &named[i + 1..] {
            if let Some(branch) = branches.iter().find(|b| other_branches.contains(b)) {
                return Err(format!("branch {branch} is named by both {flag} and {other}").into());
            }
        }
    }
    Ok(())
}

/// The action named for each branch by `--delete`, `--keep` and `--push`.
fn decisions(args: &SyncArgs) -> std::collections::HashMap<String, Choice> {
    let delete = args.delete.iter().map(|b| (b.clone(), Choice::Delete));
    let keep = args.keep.iter().map(|b| (b.clone(), Choice::Keep));
    let push = args.push.iter().map(|b| (b.clone(), Choice::Push));
    delete.chain(keep).chain(push).collect()
}

/// Sync, printing the human-readable report (unless in JSON mode) and
/// returning what was done.
pub fn sync(args: SyncArgs) -> Result<SyncData, String> {
    let json = crate::output::json_mode();
    let dry_run = args.dry_run;
    let mut branch_outcomes: Vec<BranchOutcome> = Vec::new();
    let diverged_strategy = diverged_strategy(&args)?;
    warn_removed_config();

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

    // Branches without upstream are offered to be pushed, unless there is
    // nowhere to push to or the user chose to keep them local.
    let has_remote = !crate::git::list_remotes()?.is_empty();
    let kept_local = crate::git::kept_local_branches();

    // 4. Process all local branches that have an upstream.
    let branches = crate::git::list_branch_tips()?;

    let mut current_branch_pruned = false;
    let mut deletion_candidates: Vec<Candidate> = Vec::new();
    let mut local_candidates: Vec<Candidate> = Vec::new();

    progress.update(&format!("Syncing {} branches…", branches.len()));
    for tip in &branches {
        let branch = &tip.branch;
        let Some(upstream) = &tip.upstream else {
            let outcome = if !has_remote || default_branch.local.as_deref() == Some(branch) {
                BranchOutcome::no_upstream(branch)
            } else if kept_local.contains(branch) {
                BranchOutcome::kept_local(branch)
            } else {
                match no_upstream_candidate(branch, &worktrees, &default_branch) {
                    Ok(candidate) => {
                        local_candidates.push(candidate);
                        continue;
                    }
                    Err(e) => BranchOutcome::failed(branch, &e),
                }
            };
            branch_outcomes.push(outcome);
            continue;
        };

        let is_current = current_branch.as_deref() == Some(branch.as_str());

        if pruned.contains(upstream) {
            if is_current {
                current_branch_pruned = true;
            }

            let unpushed =
                unpushed_before_prune(&branches_before, branch, default_branch.remote.as_deref());
            let status = match unpushed {
                Unpushed::None => pruned_branch_status(branch, &worktrees, &default_branch),
                unpushed => unpushed_branch_status(branch, unpushed, &worktrees, &default_branch),
            };
            record_pruned_status(
                branch,
                status,
                &mut branch_outcomes,
                &mut deletion_candidates,
            );
        } else if tip.upstream_sha.is_none() {
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
                rebase: diverged_strategy == DivergedStrategy::Rebase,
            };
            // A local upstream may have moved earlier in this loop.
            let Some(remote_sha) = tip.current_upstream_sha() else {
                let reason = format!("upstream {upstream} could not be resolved");
                branch_outcomes.push(BranchOutcome::needs_attention(branch, &reason, &reason));
                continue;
            };
            match fast_forward_branch(
                &target,
                &tip.sha,
                &remote_sha,
                &worktrees,
                diverged_strategy,
                dry_run,
            ) {
                Ok(outcome) => branch_outcomes.push(outcome),
                Err(e) => branch_outcomes
                    .push(BranchOutcome::failed(branch, &e).decided_by(DecidedBy::Auto)),
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
                    .chain(&local_candidates)
                    .map(|c| (c.branch.as_str(), c.detail.as_str())),
            ),
    );
    if !json {
        report::print_sections(&branch_outcomes, column);
    }

    let decisions = decisions(&args);
    let mut ignored_decisions: Vec<String> = decisions
        .keys()
        .filter(|branch| {
            !deletion_candidates
                .iter()
                .chain(&local_candidates)
                .any(|c| &c.branch == *branch)
        })
        .cloned()
        .collect();
    ignored_decisions.sort();
    if !json {
        for branch in &ignored_decisions {
            warn(&format!(
                "{branch} isn't up for a decision (it has a remote, needs attention or is kept local); left it alone"
            ));
        }
    }

    // Ask once, up front, about every branch whose remote was deleted, then
    // about every branch without upstream, rather than prompting one at a
    // time. Under --json or --dry-run, no prompt is issued: candidates are
    // reported but never acted on (see pruned_branch deletion policy above).
    let mode = PruneMode {
        json,
        dry_run,
        delete_pruned: args.delete_pruned,
        apply_recommended: args.apply_recommended,
        decisions: &decisions,
    };
    for candidates in [&deletion_candidates, &local_candidates] {
        branch_outcomes.extend(resolve_pruned_deletions(
            candidates,
            &current_wt,
            &worktrees,
            &main_wt_path,
            &mode,
            column,
        )?);
    }

    if !json {
        report::print_summary(&branch_outcomes);
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

    let behind_base = match current_branch.as_deref() {
        Some(branch)
            if !args.skip_behind_base
                && !current_branch_pruned
                && branch != "HEAD"
                && crate::git::commit_exists(&format!("refs/heads/{branch}")) =>
        {
            // Only a hint: a branch whose base can't be told is just not hinted at.
            super::update::configured_strategy()
                .and_then(|strategy| {
                    crate::git::behind_base(
                        branch,
                        strategy == super::update_report::Strategy::Rebase,
                    )
                })
                .ok()
                .flatten()
                .map(|(base, count)| BehindBase {
                    base: base.branch,
                    count,
                    merged_parent: base.merged_parent,
                })
        }
        _ => None,
    };

    if !json {
        // A current branch offered a decision was already listed with it.
        let current_listed = current_outcome.is_some_and(|o| o.group == report::Group::Gone);
        if !current_branch_pruned
            && !current_listed
            && current_outcome.is_none_or(|o| o.action == "no-upstream")
        {
            crate::output::info("No upstream configured for current branch, skipping sync.");
        }
        let any_work = any_branch_action_taken(&branch_outcomes);
        // "Already up to date" would contradict a "Needs attention" entry.
        let needs_attention = branch_outcomes
            .iter()
            .any(|o| o.group == report::Group::Attention);
        if any_work || !needs_attention {
            crate::output::success(final_status_message(any_work, dry_run));
        }
        if let (Some(branch), Some(behind)) = (current_branch.as_deref(), &behind_base) {
            print_behind_base(branch, behind);
        }
    }

    Ok(SyncData {
        dry_run,
        branches: branch_outcomes,
        current_branch: CurrentBranchOutcome {
            name: current_branch,
            pulled,
            pull_skipped_reason,
            behind_base,
        },
        ignored_decisions,
    })
}

/// Point out that `git mate update` would bring the current branch up to
/// date with its base.
fn print_behind_base(branch: &str, behind: &BehindBase) {
    let state = match &behind.merged_parent {
        Some(parent) => format!(
            "{parent}, which {branch} is stacked on, was merged into {}",
            behind.base
        ),
        None => {
            let plural = if behind.count == 1 { "" } else { "s" };
            format!(
                "{branch} is {} commit{plural} behind {}",
                behind.count, behind.base
            )
        }
    };
    crate::output::info(&format!(
        "{state} {} {}",
        console::style("→").dim(),
        console::style("git mate update").cyan()
    ));
}

/// Whether a branch is in line with its upstream after this run (or, under
/// `--dry-run`, would be). A branch pushed by sync is: it is its upstream.
fn is_synced(outcome: &BranchOutcome) -> bool {
    matches!(
        outcome.action.as_str(),
        "up-to-date" | "fast-forwarded" | "reset-to-upstream" | "merged" | "rebased" | "pushed"
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
                | "deleted"
                | "deletion-candidate"
                | "pushed"
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

/// Resolves how diverged branches are handled: an explicit `--diverged`
/// wins, then `mate.sync.diverged`, then the deprecated
/// `mate.divergedStrategy`, then `merge`.
fn diverged_strategy(args: &SyncArgs) -> Result<DivergedStrategy, String> {
    if let Some(strategy) = args.diverged {
        return Ok(strategy);
    }
    let (key, value) = match crate::git::config::read_string("mate.sync.diverged") {
        Some(value) => ("mate.sync.diverged", value),
        None => match crate::git::config::read_string("mate.divergedStrategy") {
            Some(value) => {
                warn("mate.divergedStrategy is deprecated; rename it to mate.sync.diverged");
                ("mate.divergedStrategy", value)
            }
            None => return Ok(DivergedStrategy::Merge),
        },
    };

    <DivergedStrategy as clap::ValueEnum>::from_str(&value, true).map_err(|_| {
        format!("invalid value for {key}: {value:?}; expected \"merge\", \"rebase\" or \"skip\"")
    })
}

/// Point out settings sync no longer reads, so they don't silently stop
/// working.
fn warn_removed_config() {
    if crate::git::config::read_string("mate.autoMerge").is_some() {
        warn("mate.autoMerge was removed and is ignored; sync no longer merges the default branch");
    }
}

fn warn(message: &str) {
    eprintln!("{} {message}", console::style("warning:").yellow().bold());
}

/// Returns (branch, local_sha, upstream_sha) for every local branch that has an upstream.
/// Called before fetching, so upstream SHAs reflect the state on the remote right now.
fn snapshot_branch_upstreams() -> Vec<(String, Option<String>, Option<String>)> {
    let Ok(branches) = crate::git::list_branch_tips() else {
        return vec![];
    };
    branches
        .into_iter()
        .filter(|tip| tip.upstream.is_some())
        .map(|tip| (tip.branch, Some(tip.sha), tip.upstream_sha))
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
    local_sha: &str,
    remote_sha: &str,
    worktrees: &[crate::git::WorktreeEntry],
    diverged_strategy: DivergedStrategy,
    dry_run: bool,
) -> Result<BranchOutcome, String> {
    let branch = target.branch;
    let upstream = target.upstream;
    if local_sha == remote_sha {
        return Ok(BranchOutcome::up_to_date(branch));
    }
    if crate::git::is_ancestor(remote_sha, local_sha)? {
        // Only local commits on top of the upstream: nothing to bring in.
        let commits = crate::git::count_commits(remote_sha, local_sha)?;
        return Ok(BranchOutcome::ahead(branch, commits));
    }
    if !crate::git::is_ancestor(local_sha, remote_sha)? {
        return resolve_diverged_branch(
            target,
            local_sha,
            remote_sha,
            worktrees,
            diverged_strategy,
            dry_run,
        );
    }

    let commits = crate::git::count_commits(local_sha, remote_sha)?;
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
        crate::git::update_ref(&format!("refs/heads/{branch}"), remote_sha)?;
    }
    Ok(BranchOutcome::fast_forwarded(branch, commits, false))
}

/// Bring a branch that diverged from its upstream back in line, without
/// checking it out.
///
/// - Rebased locally but not pushed yet (every upstream commit has an
///   equivalent local one, and the local side has more): left alone, it needs a force-push. Merging the
///   upstream back in would duplicate every commit.
/// - No work of its own (it was at or behind the upstream before fetching,
///   so the remote was rewritten, or all its patches are already upstream):
///   reset to the upstream.
/// - Otherwise its commits are rebased onto the upstream (`rebase`) or the
///   upstream is merged in (`merge`), but only if that is conflict-free.
///
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

    let remote_rewritten = match target.upstream_before {
        Some(before) => crate::git::is_ancestor(local_sha, before)?,
        None => false,
    };
    // Both sides having the same patches (e.g. only reworded) says nothing
    // about which one was rewritten; that case is left to the checks below,
    // as a branch without work of its own.
    let local_patches_upstream = crate::git::patches_already_in(remote_sha, local_sha)?;
    if !remote_rewritten
        && !local_patches_upstream
        && crate::git::patches_already_in(local_sha, remote_sha)?
    {
        return Ok(BranchOutcome::needs_force_push(branch)
            .with_hint(Hint::new("push with", force_push_command(branch, upstream))));
    }

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

    let no_local_work = remote_rewritten || local_patches_upstream;
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

    let worktree = worktree_path.as_deref();
    let rebase = diverged_strategy == DivergedStrategy::Rebase;
    let integration = if rebase {
        crate::git::rebase_branch(
            &crate::git::Rebase {
                branch,
                onto: remote_sha,
                boundary: remote_sha,
                worktree,
                autostash: false,
                reflog: &format!("git-mate sync: rebase onto {upstream}"),
            },
            dry_run,
        )?
    } else {
        crate::git::merge_into_branch(
            &crate::git::Merge {
                branch,
                theirs: upstream,
                worktree,
                autostash: false,
                message: &format!("Merge remote-tracking branch '{upstream}' into {branch}"),
                reflog: &format!("git-mate sync: merge {upstream}"),
            },
            dry_run,
        )?
    };

    Ok(match integration {
        crate::git::Integration::Clean if rebase => BranchOutcome::rebased(branch, dry_run),
        crate::git::Integration::Clean => BranchOutcome::merged(branch, dry_run),
        crate::git::Integration::Conflict if rebase => BranchOutcome::needs_attention(
            branch,
            "diverged with conflicts; rebase aborted",
            "diverged, rebasing onto upstream would conflict",
        )
        .with_hint(resolve_hint()),
        crate::git::Integration::Conflict => BranchOutcome::needs_attention(
            branch,
            "diverged with conflicts",
            "diverged, merging upstream would conflict",
        )
        .with_hint(resolve_hint()),
        crate::git::Integration::Unavailable(e) => {
            let first_line = e.lines().next().unwrap_or(&e).to_string();
            BranchOutcome::needs_attention(
                branch,
                &format!("could not rebase: {e}"),
                &format!("could not rebase: {first_line}"),
            )
            .with_hint(resolve_hint())
        }
    })
}

/// `git push --force-with-lease <remote> <branch>`, naming the remote only
/// when it can be told from the upstream (`origin/feature` → `origin`).
fn force_push_command(branch: &str, upstream: &str) -> String {
    let remote = crate::git::list_remotes()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| upstream.starts_with(&format!("{r}/")))
        .max_by_key(|r| r.len());
    match remote {
        Some(remote) => format!("git push --force-with-lease {remote} {branch}"),
        None => "git push --force-with-lease".to_string(),
    }
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

/// Decide how to offer a branch whose upstream was gone before this sync for
/// deletion. Without a snapshot from before the remote went away, whether it
/// had unpushed commits is unknown, so it is only recommended for deletion if
/// everything on it is already in the default branch (merged, rebase-merged
/// or squash-merged); otherwise only the user may delete it.
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
    let candidate = match crate::git::content_merged_into(remote, branch)? {
        Some(merged_at) => Candidate {
            branch: branch.to_string(),
            kind: CandidateKind::RemoteGone,
            choices: REMOTE_GONE_CHOICES,
            detail: merged_detail(&merged_at, remote),
            warn: false,
            recommended: Choice::Delete,
            bulk_delete_blocker: None,
            unpushed: Unpushed::Unknown,
        },
        None => Candidate {
            branch: branch.to_string(),
            kind: CandidateKind::RemoteGone,
            choices: REMOTE_GONE_CHOICES,
            detail: format!("not merged into {remote}"),
            warn: true,
            recommended: Choice::Later,
            bulk_delete_blocker: Some(format!(
                "upstream {upstream} is gone and branch has changes not in {remote}, skipping"
            )),
            unpushed: Unpushed::Unknown,
        },
    };
    deletable_unless_dirty(branch, worktrees, candidate)
}

/// How many commits a branch whose upstream was pruned during this sync had
/// that its upstream did not, as of the snapshot taken before fetching.
/// Commits that only came in from the default branch (e.g. by merging it into
/// the branch) exist there too, so they don't count.
fn unpushed_before_prune(
    branches_before: &[(String, Option<String>, Option<String>)],
    branch: &str,
    default_remote: Option<&str>,
) -> Unpushed {
    let Some((_, Some(local), Some(upstream))) =
        branches_before.iter().find(|(b, _, _)| b == branch)
    else {
        return Unpushed::Unknown;
    };
    let mut pushed = vec![upstream.as_str()];
    pushed.extend(default_remote);
    match crate::git::count_commits_not_in(&pushed, local) {
        Ok(0) => Unpushed::None,
        Ok(n) => Unpushed::Commits(n),
        Err(_) => Unpushed::Unknown,
    }
}

/// A branch whose upstream was pruned during this sync while it still had
/// unpushed commits. If everything on it — the unpushed commits included —
/// is already in the default branch (e.g. the PR was squash-merged and the
/// local commit only merged the default branch in), deleting it loses
/// nothing, so it is treated like any merged branch. Otherwise it is offered
/// so the user can keep it (and push it again later), delete it or decide
/// later — but never preselected for deletion, and never deleted by
/// `--delete-pruned`.
fn unpushed_branch_status(
    branch: &str,
    unpushed: Unpushed,
    worktrees: &[crate::git::WorktreeEntry],
    default_branch: &DefaultBranch,
) -> Result<PrunedStatus, String> {
    let unpushed_detail = match unpushed {
        Unpushed::Commits(n) => {
            let plural = if n == 1 { "" } else { "s" };
            format!("{n} unpushed commit{plural}")
        }
        _ => "may have unpushed commits".to_string(),
    };
    let merged = default_branch.remote.as_deref().and_then(|remote| {
        crate::git::content_merged_into(remote, branch)
            .ok()
            .flatten()
            .map(|merged_at| merged_detail(&merged_at, remote))
    });
    let candidate = match merged {
        Some(merged) => Candidate {
            branch: branch.to_string(),
            kind: CandidateKind::RemoteGone,
            choices: REMOTE_GONE_CHOICES,
            detail: match unpushed {
                Unpushed::Commits(_) => format!("{merged}, incl. {unpushed_detail}"),
                _ => merged,
            },
            warn: false,
            recommended: Choice::Delete,
            bulk_delete_blocker: None,
            unpushed,
        },
        None => Candidate {
            branch: branch.to_string(),
            kind: CandidateKind::RemoteGone,
            choices: REMOTE_GONE_CHOICES,
            bulk_delete_blocker: Some(match default_branch.remote.as_deref() {
                Some(remote) => format!("remote deleted and {unpushed_detail} not in {remote}"),
                None => format!("remote deleted and {unpushed_detail}"),
            }),
            detail: unpushed_detail,
            warn: true,
            recommended: Choice::Later,
            unpushed,
        },
    };
    deletable_unless_dirty(branch, worktrees, candidate)
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
            kind: CandidateKind::RemoteGone,
            choices: REMOTE_GONE_CHOICES,
            detail: merged_detail(&merged_at, remote),
            warn: false,
            recommended: Choice::Delete,
            bulk_delete_blocker: None,
            unpushed: Unpushed::None,
        },
        Some((remote, Ok(None))) => Candidate {
            branch: branch.to_string(),
            kind: CandidateKind::RemoteGone,
            choices: REMOTE_GONE_CHOICES,
            detail: format!("not merged into {remote}"),
            warn: true,
            recommended: Choice::Later,
            bulk_delete_blocker: None,
            unpushed: Unpushed::None,
        },
        _ => Candidate {
            branch: branch.to_string(),
            kind: CandidateKind::RemoteGone,
            choices: REMOTE_GONE_CHOICES,
            detail: "remote deleted".to_string(),
            warn: false,
            recommended: Choice::Later,
            bulk_delete_blocker: None,
            unpushed: Unpushed::None,
        },
    };
    deletable_unless_dirty(branch, worktrees, candidate)
}

/// Offer a branch without upstream to be pushed, deleted or kept local.
/// Only one whose commits already landed in the default branch is
/// preselected for deletion; one without commits of its own (e.g. just
/// created) or with commits that exist nowhere else is left for the user to
/// decide. Deleting is not offered when its worktree has uncommitted changes.
fn no_upstream_candidate(
    branch: &str,
    worktrees: &[crate::git::WorktreeEntry],
    default_branch: &DefaultBranch,
) -> Result<Candidate, String> {
    let dirty = match worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
    {
        Some(wt) => !crate::git::is_worktree_clean(
            wt.path.to_str().ok_or("worktree path is not valid UTF-8")?,
        )?,
        None => false,
    };
    let (detail, warn, recommended, unpushed) = match default_branch.remote.as_deref() {
        Some(remote) => match crate::git::content_merged_into(remote, branch)? {
            Some(crate::git::MergedAt::Contained) => (
                "no commits of its own".to_string(),
                false,
                Choice::Later,
                Unpushed::None,
            ),
            Some(merged_at) => (
                merged_detail(&merged_at, remote),
                false,
                Choice::Delete,
                Unpushed::None,
            ),
            None => {
                let n = crate::git::count_commits_not_in(&[remote], branch)?;
                let plural = if n == 1 { "" } else { "s" };
                (
                    format!("{n} commit{plural} not in {remote}"),
                    true,
                    Choice::Later,
                    Unpushed::Commits(n),
                )
            }
        },
        None => (
            "never pushed".to_string(),
            false,
            Choice::Later,
            Unpushed::Unknown,
        ),
    };
    let (detail, recommended, choices) = if dirty {
        (
            format!("{detail}, uncommitted changes"),
            match recommended {
                Choice::Delete => Choice::Later,
                other => other,
            },
            NO_UPSTREAM_DIRTY_CHOICES,
        )
    } else {
        (detail, recommended, NO_UPSTREAM_CHOICES)
    };
    Ok(Candidate {
        branch: branch.to_string(),
        kind: CandidateKind::NoUpstream,
        choices,
        detail,
        warn,
        recommended,
        bulk_delete_blocker: None,
        unpushed,
    })
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
                .with_hint(Hint::new("worktree", wt_str))
                .with_unpushed(candidate.unpushed),
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
struct PruneMode<'a> {
    json: bool,
    dry_run: bool,
    delete_pruned: bool,
    apply_recommended: bool,
    /// Actions named per branch by `--delete` and `--keep`.
    decisions: &'a std::collections::HashMap<String, Choice>,
}

/// Resolves what to do with branches whose remote was deleted, in priority
/// order: `--dry-run` always previews only, regardless of the other flags;
/// then `--delete-pruned` deletes all candidates (except those it must not
/// delete without asking, which are left as they are), and `--delete`/`--keep`
/// apply the action named for a branch and `--apply-recommended` each other
/// one's recommended action, all without prompting (this is
/// what makes `--json` actually act rather than just report); otherwise
/// plain `--json` never prompts (which would block on stdin) and reports
/// candidates without acting; otherwise let the user pick an action per
/// branch in the chooser, or, without a terminal for it, leave them all as
/// they are.
///
/// All `candidates` are of the same kind. `--delete-pruned` only concerns
/// branches whose remote is gone; branches without upstream are then left
/// to `--delete`/`--keep`/`--push`, without prompting.
fn resolve_pruned_deletions(
    candidates: &[Candidate],
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
    mode: &PruneMode<'_>,
    column: usize,
) -> Result<Vec<BranchOutcome>, String> {
    let Some(kind) = candidates.first().map(|c| c.kind) else {
        return Ok(Vec::new());
    };
    let title = kind.title();

    let as_candidates = || candidates.iter().map(BranchOutcome::candidate).collect();

    if mode.dry_run {
        if !mode.json {
            report::print_preview(title, candidates, column);
            report::print_note("dry run: showing the preselected actions; nothing was changed");
        }
        return Ok(as_candidates());
    }

    if mode.delete_pruned && kind == CandidateKind::RemoteGone {
        let (blocked, deletable): (Vec<&Candidate>, Vec<&Candidate>) = candidates
            .iter()
            .partition(|c| c.bulk_delete_blocker.is_some());
        let mut outcomes = Vec::new();
        if !deletable.is_empty() {
            if !mode.json {
                report::print_title("Remote gone, deleted");
            }
            outcomes.extend(deletable.into_iter().map(|c| {
                delete_candidate(c, current_wt, worktrees, main_wt_path, mode.json, column)
                    .decided_by(DecidedBy::Flag)
            }));
        }
        if !blocked.is_empty() {
            if !mode.json {
                report::print_title("Remote gone, not deleted");
                let detail_column = report::detail_column(candidates);
                for candidate in &blocked {
                    for line in report::decision_lines(
                        candidate,
                        &DecisionRow::Left,
                        false,
                        column,
                        detail_column,
                    ) {
                        eprintln!("{line}");
                    }
                }
                report::print_note(
                    "these may hold work that exists nowhere else; run sync without --delete-pruned to choose",
                );
            }
            outcomes.extend(blocked.into_iter().map(|c| {
                BranchOutcome::left_unasked(c, c.bulk_delete_blocker.as_deref().unwrap_or_default())
            }));
        }
        return Ok(outcomes);
    }

    let apply = |candidate: &Candidate, choice: Choice| {
        apply_choice(candidate, choice, current_wt, worktrees, main_wt_path)
    };

    if mode.apply_recommended || !mode.decisions.is_empty() || mode.delete_pruned {
        if !mode.json {
            report::print_title(title);
        }
        let detail_column = report::detail_column(candidates);
        let mut kept = 0;
        let outcomes = candidates
            .iter()
            .map(|candidate| {
                let choice = mode
                    .decisions
                    .get(&candidate.branch)
                    .copied()
                    .or(mode.apply_recommended.then_some(candidate.recommended));
                let row = match choice {
                    Some(choice) => apply(candidate, choice),
                    None => DecisionRow::Left,
                };
                if !mode.json {
                    for line in
                        report::decision_lines(candidate, &row, false, column, detail_column)
                    {
                        eprintln!("{line}");
                    }
                }
                kept += usize::from(row == DecisionRow::Kept);
                match choice {
                    Some(_) => decision_outcome(candidate, &row, DecidedBy::Flag),
                    // Nobody decided: still a candidate for the next run.
                    None => BranchOutcome::candidate(candidate),
                }
            })
            .collect();
        if !mode.json {
            print_kept_note(kept);
        }
        return Ok(outcomes);
    }

    if mode.json {
        return Ok(as_candidates());
    }

    let chosen = sync_chooser::resolve(title, candidates, column, apply);
    let Some(rows) = chosen else {
        // Without a terminal nobody can answer a prompt: leave every branch
        // as it is and say how to resolve them.
        report::print_title(title);
        let detail_column = report::detail_column(candidates);
        for candidate in candidates {
            for line in
                report::decision_lines(candidate, &DecisionRow::Left, false, column, detail_column)
            {
                eprintln!("{line}");
            }
        }
        report::print_note(match kind {
            CandidateKind::RemoteGone => {
                "no terminal to choose in; run sync in one, or pass --apply-recommended or --delete-pruned"
            }
            CandidateKind::NoUpstream => {
                "no terminal to choose in; run sync in one, or pass --push, --delete or --keep <branch>"
            }
        });
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

/// Push a branch, delete it, keep it so it isn't offered again (no longer
/// tracking a deleted upstream, and marked as kept local), or leave it to
/// ask again.
fn apply_choice(
    candidate: &Candidate,
    choice: Choice,
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
) -> DecisionRow {
    let branch = candidate.branch.as_str();
    if !candidate.choices.contains(&choice) {
        return DecisionRow::Failed(match (choice, candidate.kind) {
            (Choice::Push, CandidateKind::RemoteGone) => format!(
                "not pushed: its remote branch was deleted; keep it and push it with git push -u origin {branch}"
            ),
            _ => format!(
                "not {}: its worktree has uncommitted changes",
                past_tense(choice)
            ),
        });
    }
    match choice {
        Choice::Push => match push_new_branch(branch) {
            Ok(()) => DecisionRow::Pushed,
            Err(e) => DecisionRow::Failed(format!("could not push: {e}")),
        },
        Choice::Delete => match delete_pruned_branch(branch, current_wt, worktrees, main_wt_path) {
            Ok(()) => DecisionRow::Deleted,
            Err(e) => DecisionRow::Failed(e),
        },
        Choice::Keep => {
            let kept = match candidate.kind {
                CandidateKind::RemoteGone => crate::git::unset_upstream(branch)
                    .map_err(|e| format!("could not unset upstream: {e}")),
                CandidateKind::NoUpstream => Ok(()),
            }
            // Once it has no upstream, it would otherwise be offered to be
            // pushed next time.
            .and_then(|()| {
                crate::git::mark_kept_local(branch)
                    .map_err(|e| format!("could not mark as kept local: {e}"))
            });
            match kept {
                Ok(()) => DecisionRow::Kept,
                Err(e) => DecisionRow::Failed(e),
            }
        }
        Choice::Later => DecisionRow::Left,
    }
}

fn past_tense(choice: Choice) -> &'static str {
    match choice {
        Choice::Push => "pushed",
        Choice::Delete => "deleted",
        Choice::Keep => "kept",
        Choice::Later => "left",
    }
}

/// Push a branch that has no upstream, setting the pushed branch as its
/// upstream: to its push remote if one is configured, else to `origin`.
fn push_new_branch(branch: &str) -> Result<(), String> {
    let mut target = crate::git::push_target(branch)?
        .ok_or("no remote to push to (no origin, and no remote.pushDefault set)")?;
    target.set_upstream = true;
    crate::git::push_branch(branch, &target, None)
}

fn print_kept_note(kept: usize) {
    if kept > 0 {
        let (plural, them) = if kept == 1 {
            ("", "it")
        } else {
            ("es", "them")
        };
        report::print_note(&format!(
            "kept {kept} branch{plural}; sync won't ask about {them} again"
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
        DecisionRow::Pushed => BranchOutcome::pushed(candidate),
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
