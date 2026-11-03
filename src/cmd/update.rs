use super::update_report::{self as report, Strategy, UpdateOutcome};
use crate::git::{Base, BaseSource, Detection, Integration};

#[derive(clap::Args)]
#[command(
    about = "Bring branches up to date with the branch they are based on",
    long_about = "Bring branches up to date with the branch they are based on, by rebasing them onto it
(the default) or merging it in.

Without arguments, updates the current branch. Name branches to update those instead, or
pass --all to update every local branch except the default branch.

A branch's base is read from the commit graph: a branch that contains commits of another
local branch which the default branch doesn't have is stacked on that branch; any other
branch is based on the default branch. If the branch it is stacked on has since been
merged into the default branch (also squash- or rebase-merged), the branch is rebased onto
the default branch and the parent's commits are dropped. If the graph can't tell which of
several branches is the base, the branch is skipped; pass --onto to name it.

Bases are detected before anything changes, and branches are updated after the branches
they are stacked on, so a whole stack moves along in one run.

A rebase or merge only goes through if it is conflict-free; otherwise the branch is left
as it was and listed under \"Needs attention\". Uncommitted changes in a branch's
worktree are stashed and reapplied. Branches that aren't checked out are updated without
touching any worktree (rebasing them needs Git 2.44 or newer).

Set mate.update.strategy to `rebase` or `merge` to choose the default strategy.

Run `git mate sync` first to update the base branches from their remotes."
)]
pub struct UpdateArgs {
    #[arg(
        value_name = "BRANCH",
        conflicts_with = "all",
        help = "Branches to update [default: the current branch]",
        add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer)
    )]
    pub branches: Vec<String>,
    #[arg(long, help = "Update every local branch except the default branch")]
    pub all: bool,
    #[arg(
        long,
        value_name = "BRANCH",
        conflicts_with = "all",
        help = "Use this branch as the base instead of detecting it (one branch only)",
        add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer)
    )]
    pub onto: Option<String>,
    #[arg(
        long,
        conflicts_with = "merge",
        help = "Rebase onto the base [default: mate.update.strategy, else rebase]"
    )]
    pub rebase: bool,
    #[arg(long, help = "Merge the base in instead of rebasing")]
    pub merge: bool,
    #[arg(long, help = "Preview what would be updated without changing anything")]
    pub dry_run: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateData {
    dry_run: bool,
    branches: Vec<UpdateOutcome>,
}

/// A branch to update and its base, detected before anything changed.
struct Plan {
    branch: String,
    detection: Result<Detection, String>,
}

pub fn run(args: UpdateArgs) -> Result<(), String> {
    let strategy = strategy(&args)?;
    let dry_run = args.dry_run;
    let default_branch = default_branch()?;
    let local_branches = crate::git::list_local_branches_with_upstream()?
        .into_iter()
        .map(|(branch, _)| branch)
        .collect::<Vec<_>>();
    let targets = targets(&args, &default_branch, &local_branches)?;
    let onto = match &args.onto {
        Some(onto) => Some(onto_base(onto, &targets)?),
        None => None,
    };

    let plans = targets
        .into_iter()
        .map(|branch| {
            let detection = match &onto {
                Some(base) => Ok(Detection::Found(base.clone())),
                None => crate::git::detect_base(&branch, &default_branch, &local_branches),
            };
            Plan { branch, detection }
        })
        .collect::<Vec<_>>();

    let worktrees = crate::git::list_worktrees()?;
    // Branches this run moved (or, under --dry-run, would move), so a
    // dry run can tell what would happen to the branches stacked on them.
    let mut moved = std::collections::HashSet::new();
    let mut outcomes = Vec::new();
    for plan in stack_order(plans) {
        let outcome = UpdateOutcome::new(&plan.branch, strategy, dry_run);
        let outcome = match plan.detection {
            Ok(Detection::Found(base)) => {
                let base_moved = moved.contains(&base.branch);
                update_branch(
                    &plan.branch,
                    &base,
                    strategy,
                    &worktrees,
                    dry_run,
                    base_moved,
                )
                .unwrap_or_else(|e| outcome.with_base(&base).failed(e))
            }
            Ok(Detection::Ambiguous(candidates)) => outcome.skipped(
                format!(
                    "could not tell which branch it is based on: {}",
                    candidates.join(", ")
                ),
                Some(format!(
                    "git mate update {} --onto <{}>",
                    plan.branch,
                    candidates.join("|")
                )),
            ),
            Err(e) => outcome.failed(e),
        };
        if matches!(outcome.action, "rebased" | "merged") {
            moved.insert(outcome.branch.clone());
        }
        outcomes.push(outcome);
    }

    if crate::output::json_mode() {
        crate::output::emit_json_success(
            "update",
            UpdateData {
                dry_run,
                branches: outcomes,
            },
        );
    } else {
        report::print(&outcomes);
    }
    Ok(())
}

/// `--rebase` and `--merge` win, then `mate.update.strategy`, then rebase.
fn strategy(args: &UpdateArgs) -> Result<Strategy, String> {
    if args.rebase {
        return Ok(Strategy::Rebase);
    }
    if args.merge {
        return Ok(Strategy::Merge);
    }
    match crate::git::config::read_string("mate.update.strategy").as_deref() {
        None => Ok(Strategy::Rebase),
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "rebase" => Ok(Strategy::Rebase),
            "merge" => Ok(Strategy::Merge),
            _ => Err(format!(
                "invalid value for mate.update.strategy: {value:?}; expected \"rebase\" or \"merge\""
            )),
        },
    }
}

/// The default branch to fall back to as a base: the local branch, or its
/// remote-tracking branch if there is no local one.
fn default_branch() -> Result<String, String> {
    let local = crate::git::detect_default_branch(false)?;
    if crate::git::commit_exists(&format!("refs/heads/{local}")) {
        return Ok(local);
    }
    crate::git::detect_default_branch(true)
}

/// The branches to update, validated.
fn targets(
    args: &UpdateArgs,
    default_branch: &str,
    local_branches: &[String],
) -> Result<Vec<String>, String> {
    if args.all {
        return Ok(local_branches
            .iter()
            .filter(|b| b.as_str() != default_branch)
            .cloned()
            .collect());
    }
    let targets = if args.branches.is_empty() {
        let current = crate::git::current_branch()?;
        if current == "HEAD" {
            return Err("not on a branch (detached HEAD); name the branch to update".to_string());
        }
        vec![current]
    } else {
        let mut targets: Vec<String> = Vec::new();
        for branch in &args.branches {
            if !targets.contains(branch) {
                targets.push(branch.clone());
            }
        }
        targets
    };
    for branch in &targets {
        if !local_branches.contains(branch) {
            return Err(format!("no local branch named '{branch}'"));
        }
        if branch == default_branch {
            return Err(format!(
                "'{branch}' is the default branch; it has no base to update from"
            ));
        }
    }
    Ok(targets)
}

/// The base given with `--onto`, for the single branch it applies to.
fn onto_base(onto: &str, targets: &[String]) -> Result<Base, String> {
    let [target] = targets else {
        return Err("--onto needs exactly one branch to update".to_string());
    };
    if onto == target {
        return Err(format!("cannot update '{target}' onto itself"));
    }
    let is_branch = crate::git::commit_exists(&format!("refs/heads/{onto}"))
        || crate::git::commit_exists(&format!("refs/remotes/{onto}"));
    if !is_branch {
        return Err(format!("--onto: no branch named '{onto}'"));
    }
    Ok(Base {
        branch: onto.to_string(),
        source: BaseSource::Onto,
        boundary: crate::git::resolve_ref(&format!("{onto}^{{commit}}"))?,
        merged_parent: None,
    })
}

/// Order plans so that a branch comes after the branch it is stacked on,
/// keeping the given order otherwise.
fn stack_order(mut plans: Vec<Plan>) -> Vec<Plan> {
    let base_of = |plan: &Plan| match &plan.detection {
        Ok(Detection::Found(base)) => Some(base.branch.clone()),
        _ => None,
    };
    let mut ordered = Vec::with_capacity(plans.len());
    while !plans.is_empty() {
        let pending: Vec<String> = plans.iter().map(|p| p.branch.clone()).collect();
        let ready = plans
            .iter()
            .position(|p| base_of(p).is_none_or(|base| !pending.contains(&base)))
            // A cycle can't be ordered; go on in the given order.
            .unwrap_or(0);
        ordered.push(plans.remove(ready));
    }
    ordered
}

fn update_branch(
    branch: &str,
    base: &Base,
    strategy: Strategy,
    worktrees: &[crate::git::WorktreeEntry],
    dry_run: bool,
    base_moved: bool,
) -> Result<UpdateOutcome, String> {
    let outcome = UpdateOutcome::new(branch, strategy, dry_run).with_base(base);
    let tip = crate::git::resolve_ref(&format!("refs/heads/{branch}"))?;
    let base_tip = crate::git::resolve_ref(&format!("{}^{{commit}}", base.branch))?;

    let worktree = worktree_of(branch, worktrees)?;
    if let Some(path) = &worktree
        && let Some(operation) = crate::git::operation_in_progress(path)
    {
        return Ok(outcome.skipped(
            format!("a {operation} is in progress in its worktree"),
            Some(format!("finish or abort it in {path}")),
        ));
    }

    let verb = match strategy {
        Strategy::Rebase => "rebased",
        Strategy::Merge => "merged",
    };
    if dry_run && base_moved {
        // The base would have moved by now; what it would look like, and so
        // whether this would conflict, can't be known without moving it.
        let mut outcome = outcome;
        outcome.action = verb;
        return Ok(outcome);
    }

    let behind = crate::git::count_commits(&tip, &base_tip)?;
    let up_to_date =
        base.source != BaseSource::MergedParent && crate::git::is_ancestor(&base_tip, &tip)?;
    let mut outcome = UpdateOutcome {
        behind: Some(behind),
        ..outcome
    };
    if up_to_date {
        outcome.action = "up-to-date";
        return Ok(outcome);
    }

    let stash_before = crate::git::resolve_ref("refs/stash").ok();
    let integration = match strategy {
        Strategy::Rebase => crate::git::rebase_branch(
            &crate::git::Rebase {
                branch,
                onto: &base_tip,
                boundary: &base.boundary,
                worktree: worktree.as_deref(),
                autostash: true,
                reflog: &format!("git-mate update: rebase onto {}", base.branch),
            },
            dry_run,
        )?,
        Strategy::Merge => crate::git::merge_into_branch(
            &crate::git::Merge {
                branch,
                theirs: &base.branch,
                worktree: worktree.as_deref(),
                autostash: true,
                message: &format!("Merge branch '{}' into {branch}", base.branch),
                reflog: &format!("git-mate update: merge {}", base.branch),
            },
            dry_run,
        )?,
    };

    Ok(match integration {
        Integration::Clean => {
            outcome.action = verb;
            outcome.stash_kept =
                worktree.is_some() && crate::git::resolve_ref("refs/stash").ok() != stash_before;
            outcome
        }
        Integration::Conflict => {
            let (verb, hint) = match strategy {
                Strategy::Rebase => (
                    format!("rebasing onto {} would conflict", base.branch),
                    rebase_command(branch, base),
                ),
                Strategy::Merge => (
                    format!("merging {} would conflict", base.branch),
                    format!("git checkout {branch} && git merge {}", base.branch),
                ),
            };
            outcome.skipped(verb, Some(hint))
        }
        Integration::Unavailable(e) => outcome.skipped(
            format!("could not rebase: {e}"),
            Some(rebase_command(branch, base)),
        ),
    })
}

/// The `git rebase` that does what update could not.
fn rebase_command(branch: &str, base: &Base) -> String {
    let boundary_is_base = crate::git::resolve_ref(&format!("{}^{{commit}}", base.branch))
        .is_ok_and(|tip| tip == base.boundary);
    if boundary_is_base {
        format!("git rebase {} {branch}", base.branch)
    } else {
        let short = &base.boundary[..base.boundary.len().min(12)];
        format!("git rebase --onto {} {short} {branch}", base.branch)
    }
}

/// The worktree `branch` is checked out in, or being rebased in.
fn worktree_of(
    branch: &str,
    worktrees: &[crate::git::WorktreeEntry],
) -> Result<Option<String>, String> {
    let entry = crate::git::worktree_for_branch(branch, worktrees).or_else(|| {
        worktrees.iter().find(|wt| {
            wt.branch.is_none()
                && wt
                    .path
                    .to_str()
                    .and_then(crate::git::branch_being_rebased)
                    .as_deref()
                    == Some(branch)
        })
    });
    entry
        .map(|wt| {
            wt.path
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| "worktree path is not valid UTF-8".to_string())
        })
        .transpose()
}
