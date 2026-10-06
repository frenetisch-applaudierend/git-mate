use super::run::{run, run_output};

/// Merge `theirs` into `ours` purely in the object database, without touching
/// any index or working tree. Returns the resulting tree SHA, or `None` if the
/// merge has conflicts.
pub fn merge_tree(ours: &str, theirs: &str) -> Result<Option<String>, String> {
    // exit 0 = clean merge, exit 1 = conflicts; anything else is an error
    let output = std::process::Command::new("git")
        .args(["merge-tree", "--write-tree", "--no-messages", ours, theirs])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    match output.status.code() {
        Some(0) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let tree = stdout.lines().next().unwrap_or("").trim().to_string();
            if tree.is_empty() {
                Err("`git merge-tree` produced no tree".to_string())
            } else {
                Ok(Some(tree))
            }
        }
        Some(1) => Ok(None),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("`git merge-tree` failed: {}", stderr.trim()))
        }
    }
}

/// Create a commit object for `tree` with the given parents and return its SHA.
pub fn commit_tree(tree: &str, parents: &[&str], message: &str) -> Result<String, String> {
    let mut args = vec!["commit-tree", tree];
    for parent in parents {
        args.extend_from_slice(&["-p", parent]);
    }
    args.extend_from_slice(&["-m", message]);
    run_output(&args).map(|s| s.trim().to_string())
}

/// Whether every commit on `branch` that is not on `upstream` has an
/// equivalent patch already on `upstream` (per `git cherry`), i.e. resetting
/// `branch` to `upstream` would lose no changes.
pub fn patches_already_in(upstream: &str, branch: &str) -> Result<bool, String> {
    let output = run_output(&["cherry", upstream, branch])?;
    Ok(output.lines().all(|line| line.starts_with('-')))
}

/// Merge `refspec` into the branch checked out at `path`. On failure the
/// merge is aborted so the worktree is left as it was.
pub fn merge_no_edit_in(path: &str, refspec: &str) -> Result<(), String> {
    run(&["-C", path, "merge", "--no-edit", refspec]).inspect_err(|_| {
        let _ = run(&["-C", path, "merge", "--abort"]);
    })
}

pub fn reset_hard_in(path: &str, refspec: &str) -> Result<(), String> {
    run(&["-C", path, "reset", "--hard", refspec])
}

/// How many of `target`'s first-parent commits since the merge-base
/// `content_merged_into` will test before giving up.
const MAX_HISTORY_COMMITS: usize = 500;

/// Where a branch's changes landed in the target branch.
pub enum MergedAt {
    /// The branch tip is itself on `target`'s first-parent line: it never
    /// had commits of its own, or it was fast-forwarded into `target`.
    Contained,
    /// The first of `target`'s first-parent commits that contains them.
    Commit(String),
    /// Merging into `target`'s tip changes nothing, but the commit that
    /// brought the changes in is beyond the history search limit.
    Unknown,
}

/// Whether every change on `branch` has landed in `target` at some point:
/// merging `branch` into `target` — or into one of `target`'s first-parent
/// commits since they diverged — would leave that commit's tree unchanged.
/// This catches regular merges, rebase-merges and squash-merges alike, even
/// when `target` has since moved or rewritten the merged files. Returns
/// `None` if `branch` has changes no such commit contains.
pub fn content_merged_into(target: &str, branch: &str) -> Result<Option<MergedAt>, String> {
    // No common history: nothing on `branch` can have been merged.
    let Ok(base) = run_output(&["merge-base", target, branch]) else {
        return Ok(None);
    };
    let base = base.trim();
    let tip = run_output(&["rev-parse", branch])?;
    if base == tip.trim() {
        return merged_ancestor(target, base);
    }
    let merged_at_tip = merge_is_noop(target, branch)?;
    // Oldest first, so the hit is the commit that brought the changes in.
    let history = run_output(&[
        "rev-list",
        "--first-parent",
        "--reverse",
        &format!("{base}..{target}"),
    ])?;
    for commit in history.lines().take(MAX_HISTORY_COMMITS) {
        if merge_is_noop(commit, branch)? {
            return Ok(Some(MergedAt::Commit(commit.to_string())));
        }
    }
    Ok(merged_at_tip.then_some(MergedAt::Unknown))
}

/// Where `tip`, an ancestor of `target`, was merged: the oldest of
/// `target`'s first-parent commits that contains it.
fn merged_ancestor(target: &str, tip: &str) -> Result<Option<MergedAt>, String> {
    let history = run_output(&[
        "rev-list",
        "--first-parent",
        "--reverse",
        &format!("{tip}..{target}"),
    ])?;
    for commit in history.lines().take(MAX_HISTORY_COMMITS) {
        if super::refs::is_ancestor(tip, commit)? {
            let first_parent = run_output(&["rev-parse", &format!("{commit}^1")])?;
            return Ok(Some(if first_parent.trim() == tip {
                MergedAt::Contained
            } else {
                MergedAt::Commit(commit.to_string())
            }));
        }
    }
    Ok(Some(if history.is_empty() {
        MergedAt::Contained
    } else {
        MergedAt::Unknown
    }))
}

/// Whether merging `branch` into `target` would leave `target`'s tree
/// unchanged. A conflicting merge counts as changing it.
fn merge_is_noop(target: &str, branch: &str) -> Result<bool, String> {
    let Some(merged) = merge_tree(target, branch)? else {
        return Ok(false);
    };
    let target_tree = run_output(&["rev-parse", &format!("{target}^{{tree}}")])?;
    Ok(merged == target_tree.trim())
}
