use super::update_push::{self, PushMode};
use super::update_report::{self as report, ParentBehind, Strategy, UpdateOutcome, commits};
use crate::git::{Base, BaseSource, Detection, Integration};

#[derive(clap::Args)]
#[command(
    about = "Bring branches up to date with the branch they are based on",
    long_about = "Bring branches up to date with the branch they are based on, by rebasing them onto it
(the default) or merging it in (--strategy=merge, or --merge for short).

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

When a named branch is stacked on a branch that is itself behind its own base, update
asks whether to update that branch first (and so on down the stack). Pass --stack to do
so without asking, or --no-stack to only update the named branches. Without a terminal
to ask on, the stack is left alone and the report says the base is behind.

Pass --push to push every updated (or already up-to-date) branch afterwards, if that is
a fast-forward or creates the remote branch. Branches go where `git push` would push them
(branch.<name>.pushRemote, else remote.pushDefault, else the upstream); an already
merged branch whose upstream was deleted is not pushed. A branch whose rebase rewrote commits that were
already pushed is listed as needing a force-push. --push=ask asks before each force-push,
and --push=force force-pushes without asking. A force-push only goes through if the
remote branch is still where the last fetch saw it (--force-with-lease).

Pass --sync to run `git mate sync` first, bringing the base branches up to date with their
remotes.

Settings: mate.update.strategy (`rebase` or `merge`), mate.update.stack (`ask`, `true` or
`false`), mate.update.push (`false`, `ff`, `ask` or `force`) and mate.update.sync (a
boolean) set the defaults for the flags above."
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
        value_enum,
        value_name = "STRATEGY",
        conflicts_with_all = ["rebase", "merge"],
        help = "How to bring in the base [default: mate.update.strategy, else rebase]"
    )]
    pub strategy: Option<Strategy>,
    #[arg(long, conflicts_with = "merge", help = "Short for --strategy=rebase")]
    pub rebase: bool,
    #[arg(long, help = "Short for --strategy=merge")]
    pub merge: bool,
    #[arg(
        long,
        conflicts_with = "no_stack",
        help = "Also update the branches a branch is stacked on when they are behind, without asking [default: mate.update.stack, else ask]"
    )]
    pub stack: bool,
    #[arg(
        long,
        help = "Only update the named branches, never the ones they are stacked on"
    )]
    pub no_stack: bool,
    #[arg(
        long,
        value_enum,
        value_name = "MODE",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "ff",
        conflicts_with = "no_push",
        help = "Push the branches afterwards; force-pushes only with `ask` or `force` [default: mate.update.push, else no push]"
    )]
    pub push: Option<PushMode>,
    #[arg(long, help = "Don't push, even if mate.update.push is set")]
    pub no_push: bool,
    #[arg(
        long,
        conflicts_with = "no_sync",
        help = "Run `git mate sync` first [default: mate.update.sync, else false]"
    )]
    pub sync: bool,
    #[arg(long, help = "Don't sync first, even if mate.update.sync is set")]
    pub no_sync: bool,
    #[arg(long, help = "Preview what would be updated without changing anything")]
    pub dry_run: bool,
}

/// Whether to also update a branch's stacked parent when it is behind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StackMode {
    Ask,
    Always,
    Never,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateData {
    dry_run: bool,
    /// What `--sync` did first.
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<super::sync::SyncData>,
    branches: Vec<UpdateOutcome>,
}

/// A branch to update and its base, detected before anything changed.
struct Plan {
    branch: String,
    detection: Result<Detection, String>,
    /// The branch it is stacked on is behind, but isn't being updated.
    parent_behind: Option<ParentBehind>,
}

/// The branches bases are detected among.
struct Branches<'a> {
    default: &'a str,
    local: &'a [String],
    /// Deleted by `--sync`, but possibly a merged parent.
    deleted: &'a [crate::git::DeletedBranch],
}

impl Branches<'_> {
    fn detect_base(&self, branch: &str) -> Result<Detection, String> {
        crate::git::detect_base(branch, self.default, self.local, self.deleted)
    }
}

/// Every local branch and the commit it is at.
fn local_branch_tips() -> Result<Vec<(String, String)>, String> {
    crate::git::list_local_branches_with_upstream()?
        .into_iter()
        .map(|(branch, _)| {
            let tip = crate::git::resolve_ref(&format!("refs/heads/{branch}"))?;
            Ok((branch, tip))
        })
        .collect()
}

pub fn run(args: UpdateArgs) -> Result<(), String> {
    let strategy = strategy(&args)?;
    let stack = stack_mode(&args)?;
    let push = push_mode(&args)?;
    let dry_run = args.dry_run;

    // Sync may delete branches (whose remote is gone, e.g. after being
    // merged). Remember where they were, so a branch stacked on one that was
    // merged still drops its commits.
    let tips_before_sync = if sync_enabled(&args)? {
        Some(local_branch_tips()?)
    } else {
        None
    };
    let sync = if tips_before_sync.is_some() {
        Some(super::sync::sync(super::sync::SyncArgs {
            dry_run,
            // Update is about to bring the branch up to date with its base.
            skip_behind_base: true,
            ..Default::default()
        })?)
    } else {
        None
    };

    let default_branch = crate::git::default_base_branch()?;
    let local_branches = crate::git::list_local_branches_with_upstream()?
        .into_iter()
        .map(|(branch, _)| branch)
        .collect::<Vec<_>>();
    let deleted: Vec<crate::git::DeletedBranch> = tips_before_sync
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, _)| !local_branches.contains(name))
        .map(|(name, tip)| crate::git::DeletedBranch { name, tip })
        .collect();
    let branches = Branches {
        default: &default_branch,
        local: &local_branches,
        deleted: &deleted,
    };
    let targets = targets(&args, &default_branch, &local_branches)?;
    let onto = match &args.onto {
        Some(onto) => Some(onto_base(onto, &targets)?),
        None => None,
    };

    let mut plans = targets
        .into_iter()
        .map(|branch| {
            let detection = match &onto {
                Some(base) => Ok(Detection::Found(base.clone())),
                None => branches.detect_base(&branch),
            };
            Plan {
                branch,
                detection,
                parent_behind: None,
            }
        })
        .collect::<Vec<_>>();
    if !args.all {
        add_stacked_parents(&mut plans, stack, strategy, dry_run, &branches)?;
    }

    let worktrees = crate::git::list_worktrees()?;
    // Branches this run moved (or, under --dry-run, would move), so a
    // dry run can tell what would happen to the branches stacked on them.
    let mut moved = std::collections::HashSet::new();
    let mut outcomes = Vec::new();
    for plan in stack_order(plans, &default_branch) {
        let mut outcome = UpdateOutcome::new(&plan.branch, strategy, dry_run);
        outcome.parent_behind = plan.parent_behind;
        let mut outcome = match plan.detection {
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
                .map(|mut updated| {
                    updated.parent_behind = outcome.parent_behind.take();
                    updated
                })
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
        if let Some(mode) = push {
            update_push::push(&mut outcome, mode, dry_run);
        }
        outcomes.push(outcome);
    }

    if crate::output::json_mode() {
        crate::output::emit_json_success(
            "update",
            UpdateData {
                dry_run,
                sync,
                branches: outcomes,
            },
        );
    } else {
        report::print(&outcomes);
    }
    Ok(())
}

/// `--strategy`, `--rebase` and `--merge` win, then `mate.update.strategy`,
/// then rebase.
fn strategy(args: &UpdateArgs) -> Result<Strategy, String> {
    if let Some(strategy) = args.strategy {
        return Ok(strategy);
    }
    if args.rebase {
        return Ok(Strategy::Rebase);
    }
    if args.merge {
        return Ok(Strategy::Merge);
    }
    configured_strategy()
}

/// `mate.update.strategy`, else rebase.
pub(crate) fn configured_strategy() -> Result<Strategy, String> {
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

/// `--stack` and `--no-stack` win, then `mate.update.stack`, then ask.
fn stack_mode(args: &UpdateArgs) -> Result<StackMode, String> {
    if args.stack {
        return Ok(StackMode::Always);
    }
    if args.no_stack {
        return Ok(StackMode::Never);
    }
    let Some(value) = crate::git::config::read_string("mate.update.stack") else {
        return Ok(StackMode::Ask);
    };
    match value.to_ascii_lowercase().as_str() {
        "ask" => Ok(StackMode::Ask),
        "true" | "yes" | "on" | "1" => Ok(StackMode::Always),
        "false" | "no" | "off" | "0" => Ok(StackMode::Never),
        _ => Err(format!(
            "invalid value for mate.update.stack: {value:?}; expected \"ask\", \"true\" or \"false\""
        )),
    }
}

/// `--push[=<mode>]` and `--no-push` win, then `mate.update.push`, then no
/// push.
fn push_mode(args: &UpdateArgs) -> Result<Option<PushMode>, String> {
    if args.push.is_some() {
        return Ok(args.push);
    }
    if args.no_push {
        return Ok(None);
    }
    let Some(value) = crate::git::config::read_string("mate.update.push") else {
        return Ok(None);
    };
    match value.to_ascii_lowercase().as_str() {
        "false" | "no" | "off" | "0" => Ok(None),
        "ff" | "true" | "yes" | "on" | "1" => Ok(Some(PushMode::Ff)),
        "ask" => Ok(Some(PushMode::Ask)),
        "force" => Ok(Some(PushMode::Force)),
        _ => Err(format!(
            "invalid value for mate.update.push: {value:?}; expected \"false\", \"ff\", \"ask\" or \"force\""
        )),
    }
}

/// `--sync` and `--no-sync` win, then `mate.update.sync`, then no sync.
fn sync_enabled(args: &UpdateArgs) -> Result<bool, String> {
    if args.sync {
        return Ok(true);
    }
    if args.no_sync {
        return Ok(false);
    }
    Ok(crate::git::config::read_bool("mate.update.sync")?.unwrap_or(false))
}

/// For every planned branch stacked on branches of which one is behind its
/// own base, plan the branches down to that one too (asking first in `Ask`
/// mode), or else note on the planned branch which base is behind.
fn add_stacked_parents(
    plans: &mut Vec<Plan>,
    stack: StackMode,
    strategy: Strategy,
    dry_run: bool,
    branches: &Branches,
) -> Result<(), String> {
    for i in 0..plans.len() {
        let (chain, behind) = stack_below(&plans[i], plans, strategy, branches)?;
        let Some(behind) = behind else {
            continue;
        };
        let include = match stack {
            StackMode::Always => true,
            StackMode::Never => false,
            StackMode::Ask if dry_run => false,
            StackMode::Ask => {
                let names: Vec<&str> = chain.iter().map(|p| p.branch.as_str()).collect();
                super::confirm::confirm(
                    &format!(
                        "{} is stacked on {}; {} is {} behind {}. Update {} first?",
                        plans[i].branch,
                        names.join(" ← "),
                        behind.branch,
                        commits(behind.behind),
                        behind.base,
                        names.join(" and ")
                    ),
                    true,
                ) == Some(true)
            }
        };
        if include {
            plans.extend(chain);
        } else {
            plans[i].parent_behind = Some(behind);
        }
    }
    Ok(())
}

/// The branches `plan`'s branch is stacked on, nearest first, down to the
/// deepest one that is behind its own base (and which one that is), leaving
/// out branches already planned. Empty if none is behind.
fn stack_below(
    plan: &Plan,
    plans: &[Plan],
    strategy: Strategy,
    branches: &Branches,
) -> Result<(Vec<Plan>, Option<ParentBehind>), String> {
    let mut chain = Vec::new();
    let mut deepest_behind = None;
    let mut detection = match &plan.detection {
        Ok(Detection::Found(base)) => Some(base.clone()),
        _ => None,
    };
    while let Some(base) = detection.take() {
        if base.source != BaseSource::Stacked
            || plans.iter().any(|p| p.branch == base.branch)
            || chain.iter().any(|p: &Plan| p.branch == base.branch)
        {
            break;
        }
        let parent = base.branch;
        let Ok(Detection::Found(parent_base)) = branches.detect_base(&parent) else {
            break;
        };
        if let Some(behind) =
            crate::git::behind(&parent, &parent_base, strategy == Strategy::Rebase)?
        {
            deepest_behind = Some((
                chain.len() + 1,
                ParentBehind {
                    branch: parent.clone(),
                    base: parent_base.branch.clone(),
                    behind,
                },
            ));
        }
        chain.push(Plan {
            branch: parent,
            detection: Ok(Detection::Found(parent_base.clone())),
            parent_behind: None,
        });
        detection = Some(parent_base);
    }
    Ok(match deepest_behind {
        Some((len, behind)) => {
            chain.truncate(len);
            (chain, Some(behind))
        }
        None => (Vec::new(), None),
    })
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
        boundary: crate::git::resolve_ref(&crate::git::branch_ref(onto))?,
        merged_parent: None,
    })
}

/// Order plans so that a branch comes after the branch it is stacked on,
/// keeping the given order otherwise. Branches whose bases form a cycle
/// can't be ordered (nor updated): their base is reported as ambiguous.
fn stack_order(mut plans: Vec<Plan>, default_branch: &str) -> Vec<Plan> {
    let base_of = |plan: &Plan| match &plan.detection {
        Ok(Detection::Found(base)) => Some(base.branch.clone()),
        _ => None,
    };
    let mut ordered = Vec::with_capacity(plans.len());
    while !plans.is_empty() {
        let pending: Vec<String> = plans.iter().map(|p| p.branch.clone()).collect();
        match plans
            .iter()
            .position(|p| base_of(p).is_none_or(|base| !pending.contains(&base)))
        {
            Some(ready) => ordered.push(plans.remove(ready)),
            None => {
                // Every pending branch waits for another: follow the bases
                // from the first one until they come round. None of the
                // branches in that cycle has a trustworthy base, so each is
                // reported as ambiguous between the others and the default.
                let mut seen = vec![plans[0].branch.clone()];
                let mut at = 0;
                loop {
                    let base = base_of(&plans[at]).expect("pending plans have a base");
                    if let Some(start) = seen.iter().position(|b| *b == base) {
                        let cycle = seen.split_off(start);
                        for member in &cycle {
                            let mut others: Vec<String> =
                                cycle.iter().filter(|b| *b != member).cloned().collect();
                            others.push(default_branch.to_string());
                            let index = plans.iter().position(|p| p.branch == *member).unwrap();
                            plans[index].detection = Ok(Detection::Ambiguous(others));
                        }
                        break;
                    }
                    at = plans.iter().position(|p| p.branch == base).unwrap();
                    seen.push(base);
                }
            }
        }
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
    let tip = crate::git::resolve_ref(&format!("refs/heads/{branch}"))?;
    let outcome = UpdateOutcome {
        old_tip: Some(tip.clone()),
        ..UpdateOutcome::new(branch, strategy, dry_run).with_base(base)
    };
    let base_ref = crate::git::branch_ref(&base.branch);
    let base_tip = crate::git::resolve_ref(&base_ref)?;

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

    let behind = crate::git::behind(branch, base, strategy == Strategy::Rebase)?;
    let mut outcome = UpdateOutcome {
        behind: Some(behind.unwrap_or(0)),
        ..outcome
    };
    if behind.is_none() {
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
                theirs: &base_ref,
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
                    rebase_command(branch, base, worktree.as_deref()),
                ),
                Strategy::Merge => (
                    format!("merging {} would conflict", base.branch),
                    match git_in(worktree.as_deref()) {
                        Some(git) => format!("{git} merge {}", base.branch),
                        None => format!("git checkout {branch} && git merge {}", base.branch),
                    },
                ),
            };
            outcome.skipped(verb, Some(hint))
        }
        Integration::Unavailable(e) => outcome.skipped(
            format!("could not rebase: {e}"),
            Some(rebase_command(branch, base, worktree.as_deref())),
        ),
    })
}

/// The `git rebase` that does what update could not, run in `worktree` if
/// the branch is checked out there.
fn rebase_command(branch: &str, base: &Base, worktree: Option<&str>) -> String {
    let boundary_is_base = crate::git::resolve_ref(&crate::git::branch_ref(&base.branch))
        .is_ok_and(|tip| tip == base.boundary);
    let (git, branch) = match git_in(worktree) {
        Some(git) => (git, String::new()),
        None => ("git".to_string(), format!(" {branch}")),
    };
    if boundary_is_base {
        format!("{git} rebase {}{branch}", base.branch)
    } else {
        let short = &base.boundary[..base.boundary.len().min(12)];
        format!("{git} rebase --onto {} {short}{branch}", base.branch)
    }
}

/// How to run git on a branch checked out in `worktree`: there, since it
/// can't be checked out anywhere else. `None` if it isn't checked out.
fn git_in(worktree: Option<&str>) -> Option<String> {
    let path = worktree?;
    let canonical = |p: &std::path::Path| std::fs::canonicalize(p).ok();
    let here = crate::git::current_worktree_root().ok();
    if here.is_some_and(|here| canonical(&here) == canonical(std::path::Path::new(path))) {
        Some("git".to_string())
    } else {
        Some(format!("git -C {}", shell_quote(path)))
    }
}

/// `arg`, quoted for a POSIX shell if it needs to be.
fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:,=@%".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(branch: &str, base: &str) -> Plan {
        Plan {
            branch: branch.to_string(),
            detection: Ok(Detection::Found(Base {
                branch: base.to_string(),
                source: BaseSource::Stacked,
                boundary: String::new(),
                merged_parent: None,
            })),
            parent_behind: None,
        }
    }

    fn summary(plans: &[Plan]) -> Vec<(String, String)> {
        plans
            .iter()
            .map(|p| {
                let base = match &p.detection {
                    Ok(Detection::Found(base)) => base.branch.clone(),
                    Ok(Detection::Ambiguous(names)) => format!("?{}", names.join(",")),
                    Err(e) => format!("!{e}"),
                };
                (p.branch.clone(), base)
            })
            .collect()
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn stack_order_puts_parents_first() {
        let ordered = stack_order(
            vec![plan("c", "b"), plan("b", "a"), plan("a", "main")],
            "main",
        );
        assert_eq!(
            summary(&ordered),
            pairs(&[("a", "main"), ("b", "a"), ("c", "b")])
        );
    }

    #[test]
    fn stack_order_reports_a_cycle_as_ambiguous() {
        let ordered = stack_order(vec![plan("x", "a"), plan("a", "b"), plan("b", "a")], "main");
        assert_eq!(
            summary(&ordered),
            pairs(&[("a", "?b,main"), ("x", "a"), ("b", "?a,main")])
        );
    }
}
