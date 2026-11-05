//! Bring another line of history into a branch, by rebase or by merge,
//! whether or not the branch is checked out. Either way a branch is never
//! left half-rebased or half-merged: a conflict changes nothing.

use super::run::run;

/// How integrating went (or, under `dry_run`, would go).
#[derive(Debug, PartialEq, Eq)]
pub enum Integration {
    /// Applied cleanly.
    Clean,
    /// It would conflict; nothing was changed.
    Conflict,
    /// It could not be attempted (e.g. `git replay` is missing); nothing was
    /// changed.
    Unavailable(String),
}

/// Rebase `branch`'s commits after `boundary` onto `onto`, as
/// `git rebase --onto <onto> <boundary> <branch>` would.
pub struct Rebase<'a> {
    pub branch: &'a str,
    pub onto: &'a str,
    pub boundary: &'a str,
    /// The worktree `branch` is checked out in, if any.
    pub worktree: Option<&'a str>,
    /// Stash and reapply uncommitted changes in `worktree` around the rebase.
    pub autostash: bool,
    /// Reflog message when the branch is not checked out.
    pub reflog: &'a str,
}

/// Inside the branch's worktree with `git rebase`, otherwise with
/// `git replay`, which never touches a working tree.
pub fn rebase_branch(rebase: &Rebase, dry_run: bool) -> Result<Integration, String> {
    match rebase.worktree {
        Some(_) if dry_run => {
            // `git replay` predicts conflicts without touching the worktree;
            // if it is missing, assume the rebase would go through.
            Ok(
                match replay_onto(rebase.onto, rebase.boundary, rebase.branch) {
                    Ok(Some(_)) | Err(_) => Integration::Clean,
                    Ok(None) => Integration::Conflict,
                },
            )
        }
        Some(path) => {
            let mut args = vec!["-C", path, "rebase"];
            if rebase.autostash {
                args.push("--autostash");
            }
            args.extend(["--onto", rebase.onto, rebase.boundary]);
            if let Err(e) = run(&args) {
                // Stopped partway (at a conflict): undo it. Refused to start
                // at all (untracked files in the way, a hook, a lock): that
                // is an error of its own, not a conflict.
                if super::worktree::operation_in_progress(path) != Some("rebase") {
                    return Err(e);
                }
                let _ = run(&["-C", path, "rebase", "--abort"]);
                return Ok(Integration::Conflict);
            }
            Ok(Integration::Clean)
        }
        None => {
            let local_sha = super::refs::resolve_ref(&format!("refs/heads/{}", rebase.branch))?;
            let replayed = match replay_onto(rebase.onto, rebase.boundary, rebase.branch) {
                Ok(Some(sha)) => sha,
                Ok(None) => return Ok(Integration::Conflict),
                Err(e) => return Ok(Integration::Unavailable(e)),
            };
            if !dry_run {
                super::refs::update_ref_from(
                    &format!("refs/heads/{}", rebase.branch),
                    &replayed,
                    &local_sha,
                    rebase.reflog,
                )?;
            }
            Ok(Integration::Clean)
        }
    }
}

/// Merge `theirs` into `branch`.
pub struct Merge<'a> {
    pub branch: &'a str,
    pub theirs: &'a str,
    /// The worktree `branch` is checked out in, if any.
    pub worktree: Option<&'a str>,
    /// Stash and reapply uncommitted changes in `worktree` around the merge.
    pub autostash: bool,
    /// Commit message and reflog message when the branch is not checked out
    /// (inside a worktree, git writes its usual merge message).
    pub message: &'a str,
    pub reflog: &'a str,
}

/// Merge only if `git merge-tree` says it is conflict-free: inside the
/// branch's worktree, so its index and files follow, or else purely in the
/// object database.
pub fn merge_into_branch(merge: &Merge, dry_run: bool) -> Result<Integration, String> {
    let local_sha = super::refs::resolve_ref(&format!("refs/heads/{}", merge.branch))?;
    let theirs_sha = super::refs::resolve_ref(merge.theirs)?;
    let Some(tree) = super::merge::merge_tree(&local_sha, &theirs_sha)? else {
        return Ok(Integration::Conflict);
    };
    if dry_run {
        return Ok(Integration::Clean);
    }
    match merge.worktree {
        Some(path) => {
            let mut args = vec!["-C", path, "merge", "--no-edit"];
            if merge.autostash {
                args.push("--autostash");
            }
            args.push(merge.theirs);
            run(&args).inspect_err(|_| {
                let _ = run(&["-C", path, "merge", "--abort"]);
            })?;
        }
        None => {
            let merge_sha =
                super::merge::commit_tree(&tree, &[&local_sha, &theirs_sha], merge.message)?;
            super::refs::update_ref_from(
                &format!("refs/heads/{}", merge.branch),
                &merge_sha,
                &local_sha,
                merge.reflog,
            )?;
        }
    }
    Ok(Integration::Clean)
}

/// Replay `branch`'s commits after `boundary` onto `onto` with `git replay`,
/// which needs no working tree and writes no refs. Returns the new tip, or
/// `None` if replaying would conflict.
fn replay_onto(onto: &str, boundary: &str, branch: &str) -> Result<Option<String>, String> {
    let range = format!("{boundary}..refs/heads/{branch}");
    let onto_arg = format!("--onto={onto}");
    let mut output = replay(&["--ref-action=print", &onto_arg, &range])?;
    // Before Git 2.52, printing the ref updates was the only behavior and
    // `--ref-action` did not exist.
    if output.status.code() == Some(129) {
        output = replay(&[&onto_arg, &range])?;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    // Without `git replay`, git exits with 1 too, as for a conflict.
    if stderr.contains("is not a git command") {
        return Err("git replay is not available (requires Git 2.44 or newer)".to_string());
    }
    match output.status.code() {
        Some(0) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let target = format!("refs/heads/{branch}");
            stdout
                .lines()
                .filter_map(|line| line.strip_prefix("update "))
                .filter_map(|rest| rest.split_once(' '))
                .find(|(name, _)| *name == target)
                .and_then(|(_, shas)| shas.split_whitespace().next())
                .map(|sha| Some(sha.to_string()))
                .ok_or_else(|| "`git replay` reported no new tip".to_string())
        }
        Some(1) => Ok(None),
        _ => Err(format!("`git replay` failed: {stderr}")),
    }
}

fn replay(args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("git")
        .arg("replay")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))
}
