//! Which branch a branch was based on, read from the commit graph alone:
//! nothing is recorded when a branch is created, so any branch — however it
//! was created or rebased since — gets the same answer.
//!
//! A branch `B` is *stacked* on another local branch `C` when `B` contains
//! commits from `C` that the default branch doesn't have. Otherwise its base
//! is the default branch.

use super::run::run_output;

/// Where a branch's base came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BaseSource {
    /// No other branch it is stacked on: the default branch.
    Default,
    /// Stacked on another local branch.
    Stacked,
    /// Stacked on a branch whose changes have since landed in the default
    /// branch (e.g. squash-merged): the default branch, leaving out the
    /// parent's commits.
    MergedParent,
    /// Given explicitly with `--onto`.
    Onto,
}

/// The branch to bring `branch` up to date with.
#[derive(Clone, Debug)]
pub struct Base {
    /// The branch to rebase onto or merge in.
    pub branch: String,
    pub source: BaseSource,
    /// The commit `branch`'s own commits start after (the base's tip, or the
    /// merged parent's), resolved now so it stays valid once the base moves.
    pub boundary: String,
    /// For [`BaseSource::MergedParent`], the parent that was merged.
    pub merged_parent: Option<String>,
}

pub enum Detection {
    Found(Base),
    /// Several branches could be the base; the graph can't tell which.
    Ambiguous(Vec<String>),
}

/// The default branch to fall back to as a base: the local branch, or its
/// remote-tracking branch if there is no local one.
pub fn default_base_branch() -> Result<String, String> {
    let local = super::branch::detect_default_branch(false)?;
    if super::refs::commit_exists(&format!("refs/heads/{local}")) {
        return Ok(local);
    }
    super::branch::detect_default_branch(true)
}

/// The full ref of a base branch: the local branch, or else the
/// remote-tracking branch of that name (the default branch can be one).
/// Never a tag or other ref that happens to share the name.
pub fn branch_ref(name: &str) -> String {
    let local = format!("refs/heads/{name}");
    if super::refs::commit_exists(&local) {
        local
    } else {
        format!("refs/remotes/{name}")
    }
}

/// How many commits `branch` is behind `base`, if bringing it up to date
/// would change it: the base has commits it lacks, or, when rebasing, it
/// still carries the commits of a parent that has since been merged (a merge
/// keeps them, so they don't count). `None` if it is up to date.
pub fn behind(branch: &str, base: &Base, rebase: bool) -> Result<Option<usize>, String> {
    let tip = rev_parse(&format!("refs/heads/{branch}"))?;
    let base_tip = rev_parse(&branch_ref(&base.branch))?;
    let drops_merged_parent = rebase && base.source == BaseSource::MergedParent;
    if !drops_merged_parent && super::refs::is_ancestor(&base_tip, &tip)? {
        return Ok(None);
    }
    super::refs::count_commits(&tip, &base_tip).map(Some)
}

/// `branch`'s detected base, and how many commits it is behind it (see
/// [`behind`]). `None` for the default branch, an up-to-date branch, or one
/// whose base is ambiguous.
pub fn behind_base(branch: &str, rebase: bool) -> Result<Option<(Base, usize)>, String> {
    let default_branch = default_base_branch()?;
    if branch == default_branch || Some(branch) == default_branch.split_once('/').map(|(_, b)| b) {
        return Ok(None);
    }
    let candidates: Vec<String> =
        run_output(&["for-each-ref", "--format=%(refname:short)", "refs/heads/"])?
            .lines()
            .map(str::to_string)
            .collect();
    let Detection::Found(base) = detect_base(branch, &default_branch, &candidates, &[])? else {
        return Ok(None);
    };
    Ok(behind(branch, &base, rebase)?.map(|count| (base, count)))
}

/// A local branch that was deleted after bases were first looked at (by
/// `git mate sync`, say), and where it pointed.
#[derive(Clone, Debug)]
pub struct DeletedBranch {
    pub name: String,
    pub tip: String,
}

/// Detect `branch`'s base among `candidates` (the local branches), with
/// `default_branch` as the fallback. A branch in `deleted` is only considered
/// if it was merged into the default branch: then `branch`, stacked on it,
/// still drops its commits.
pub fn detect_base(
    branch: &str,
    default_branch: &str,
    candidates: &[String],
    deleted: &[DeletedBranch],
) -> Result<Detection, String> {
    let tip = rev_parse(&format!("refs/heads/{branch}"))?;
    let default_ref = branch_ref(default_branch);
    let default_tip = rev_parse(&default_ref)?;
    let linked = sharing_commits_beyond(&tip, &default_tip)?;
    // Each candidate's name, the revision it is at, and whether it is deleted.
    // Only branches sharing commits beyond the default branch can be stacked
    // on or under `branch`; deleted ones are few and checked as they are.
    let all: Vec<(String, String, bool)> = candidates
        .iter()
        .filter(|c| c.as_str() != branch && c.as_str() != default_branch && linked.contains(*c))
        .map(|c| (c.clone(), format!("refs/heads/{c}"), false))
        .chain(
            deleted
                .iter()
                .map(|d| (d.name.clone(), d.tip.clone(), true)),
        )
        .collect();
    let names: Vec<String> = all.iter().map(|(name, _, _)| name.clone()).collect();
    let created_from = if all.is_empty() {
        None
    } else {
        created_from(branch, &names)
    };

    struct Parent {
        name: String,
        rev: String,
        distance: usize,
        /// The graph alone can't tell whether it is a parent or a child.
        uncertain: bool,
    }
    let mut parents = Vec::new();
    for (candidate, rev, is_deleted) in &all {
        if candidate == branch || candidate == default_branch {
            continue;
        }
        let Some(link) = stack_link(&tip, rev, &default_tip)? else {
            continue;
        };
        if *is_deleted && super::merge::content_merged_into(&default_ref, rev)?.is_none() {
            continue;
        }
        let named = created_from.as_deref() == Some(candidate.as_str());
        // The candidate was created from `branch`: it is the child (e.g. a
        // backup of `branch` taken before `branch` moved on).
        let child = || self::created_from(candidate, &[branch.to_string()]).is_some();
        let uncertain = match link {
            // `branch` has nothing of its own beyond the candidate: they are
            // the same stack, but only the reflog says which came first.
            Link::Contained => {
                if !named {
                    continue;
                }
                false
            }
            Link::Below { mutual: false } => {
                if !named && child() {
                    continue;
                }
                false
            }
            // Both advanced since one was created from the other.
            Link::Below { mutual: true } => {
                if !named && child() {
                    continue;
                }
                !named
            }
        };
        parents.push(Parent {
            distance: super::refs::count_commits(&merge_base(&tip, rev)?, &tip)?,
            name: candidate.clone(),
            rev: rev.clone(),
            uncertain,
        });
    }

    let Some(closest) = parents.iter().map(|p| p.distance).min() else {
        return Ok(Detection::Found(Base {
            branch: default_branch.to_string(),
            source: BaseSource::Default,
            boundary: default_tip,
            merged_parent: None,
        }));
    };
    let nearest: Vec<&Parent> = parents.iter().filter(|p| p.distance == closest).collect();
    let chosen = match nearest.as_slice() {
        [only] if !only.uncertain => *only,
        _ => match nearest
            .iter()
            .find(|p| created_from.as_deref() == Some(p.name.as_str()))
        {
            Some(named) => *named,
            None => {
                let mut names: Vec<String> = nearest.iter().map(|p| p.name.clone()).collect();
                if nearest.iter().all(|p| p.uncertain) {
                    names.push(default_branch.to_string());
                }
                return Ok(Detection::Ambiguous(names));
            }
        },
    };

    let parent_tip = rev_parse(&chosen.rev)?;
    if super::merge::content_merged_into(&default_ref, &parent_tip)?.is_some() {
        return Ok(Detection::Found(Base {
            branch: default_branch.to_string(),
            source: BaseSource::MergedParent,
            boundary: parent_tip,
            merged_parent: Some(chosen.name.clone()),
        }));
    }
    Ok(Detection::Found(Base {
        branch: chosen.name.clone(),
        source: BaseSource::Stacked,
        boundary: parent_tip,
        merged_parent: None,
    }))
}

/// The local branches that share commits with `tip` the default branch (at
/// `default_tip`) doesn't have — the only ones it can be stacked on or under.
/// One `for-each-ref` instead of a merge-base per branch: a branch shares such
/// a commit exactly when it contains one of the oldest of them.
fn sharing_commits_beyond(
    tip: &str,
    default_tip: &str,
) -> Result<std::collections::HashSet<String>, String> {
    let own = run_output(&["rev-list", "--parents", tip, &format!("^{default_tip}")])?;
    let own: Vec<Vec<&str>> = own
        .lines()
        .map(|line| line.split_whitespace().collect())
        .collect();
    let in_own: std::collections::HashSet<&str> = own.iter().map(|c| c[0]).collect();
    let oldest: Vec<String> = own
        .iter()
        .filter(|c| c[1..].iter().all(|parent| !in_own.contains(parent)))
        .map(|c| format!("--contains={}", c[0]))
        .collect();
    if oldest.is_empty() {
        return Ok(Default::default());
    }
    let mut args = vec!["for-each-ref", "--format=%(refname:short)"];
    args.extend(oldest.iter().map(String::as_str));
    args.push("refs/heads/");
    Ok(run_output(&args)?.lines().map(str::to_string).collect())
}

/// How a branch at `tip` relates to `candidate`, if it is stacked on it.
enum Link {
    /// `tip` is contained in `candidate`.
    Contained,
    /// `tip` has commits beyond where it shares history with `candidate`.
    /// `mutual` when `candidate` also has commits beyond that point, so
    /// either could have been created from the other.
    Below { mutual: bool },
}

/// `candidate_ref` is the revision the candidate branch is at.
fn stack_link(tip: &str, candidate_ref: &str, default_tip: &str) -> Result<Option<Link>, String> {
    let Ok(shared) = merge_base(tip, candidate_ref) else {
        return Ok(None);
    };
    // Only history the default branch also has: siblings, not a stack.
    if super::refs::is_ancestor(&shared, default_tip)? {
        return Ok(None);
    }
    if shared == tip {
        return Ok(Some(Link::Contained));
    }
    let candidate_tip = rev_parse(candidate_ref)?;
    Ok(Some(Link::Below {
        mutual: shared != candidate_tip,
    }))
}

/// The local branch among `candidates` that `branch` was created from,
/// according to the oldest entry of its reflog ("branch: Created from main").
/// Created from `HEAD` (`git checkout -b`), it is the branch HEAD's reflog
/// says was checked out before first switching to `branch`.
fn created_from(branch: &str, candidates: &[String]) -> Option<String> {
    let created = reflog(&format!("refs/heads/{branch}"))?
        .into_iter()
        .last()?;
    let start = created
        .subject
        .strip_prefix("branch: Created from ")?
        .trim()
        .to_string();
    let start = if start == "HEAD" {
        switched_from(branch, created.time)?
    } else {
        start
    };
    let start = start.strip_prefix("refs/heads/").unwrap_or(&start);
    candidates.iter().find(|c| c.as_str() == start).cloned()
}

/// How long after creating a branch HEAD may move to it for that to count
/// as `git checkout -b` / `git switch -c`, which do both at once.
const CREATE_AND_SWITCH_SECONDS: u64 = 2;

/// The branch HEAD moved away from when it moved to `branch` as it was
/// created at `created` (unix time). HEAD moving to an earlier branch of the
/// same name, or to this one later on, says nothing about where it started;
/// neither does a branch created in another worktree, whose HEAD this isn't.
fn switched_from(branch: &str, created: u64) -> Option<String> {
    let suffix = format!(" to {branch}");
    reflog("HEAD")?
        .into_iter()
        .rev()
        .filter(|entry| (created..=created + CREATE_AND_SWITCH_SECONDS).contains(&entry.time))
        .find_map(|entry| {
            entry
                .subject
                .strip_prefix("checkout: moving from ")
                .or_else(|| entry.subject.strip_prefix("switch: moving from "))?
                .strip_suffix(&suffix)
                .map(str::to_string)
        })
}

struct ReflogEntry {
    /// Unix time.
    time: u64,
    subject: String,
}

/// `refname`'s reflog, newest first.
fn reflog(refname: &str) -> Option<Vec<ReflogEntry>> {
    let log = run_output(&[
        "reflog",
        "show",
        "--date=unix",
        "--format=%gd%x09%gs",
        refname,
    ])
    .ok()?;
    Some(
        log.lines()
            .filter_map(|line| {
                // "<ref>@{<time>}\t<subject>"
                let (selector, subject) = line.split_once('\t')?;
                let time = selector
                    .rsplit_once("@{")?
                    .1
                    .strip_suffix('}')?
                    .parse()
                    .ok()?;
                Some(ReflogEntry {
                    time,
                    subject: subject.to_string(),
                })
            })
            .collect(),
    )
}

fn merge_base(a: &str, b: &str) -> Result<String, String> {
    run_output(&["merge-base", a, b]).map(|s| s.trim().to_string())
}

fn rev_parse(rev: &str) -> Result<String, String> {
    run_output(&["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
        .map(|s| s.trim().to_string())
}
