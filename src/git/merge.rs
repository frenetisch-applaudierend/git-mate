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

/// Whether merging `branch` into `target` would leave `target`'s tree
/// unchanged, i.e. every change on `branch` is already in `target`. This
/// catches regular merges, rebase-merges and squash-merges alike; if
/// `target` has since edited the same lines, the merge differs or conflicts
/// and the answer is a (safe) `false`.
pub fn content_merged_into(target: &str, branch: &str) -> Result<bool, String> {
    let Some(merged) = merge_tree(target, branch)? else {
        return Ok(false);
    };
    let target_tree = run_output(&["rev-parse", &format!("{target}^{{tree}}")])?;
    Ok(merged == target_tree.trim())
}
