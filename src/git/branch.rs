use super::run::{run, run_output};

pub fn checkout(branch: &str) -> Result<(), String> {
    run(&["checkout", branch])
}

pub fn checkout_new_in(path: &str, branch: &str, from: &str) -> Result<(), String> {
    run(&["-C", path, "checkout", "-b", branch, from])
}

pub fn checkout_in(path: &str, branch: &str) -> Result<(), String> {
    run(&["-C", path, "checkout", branch])
}

/// Check out `branch`, discarding local changes that would block the switch.
pub fn checkout_force_in(path: &str, branch: &str) -> Result<(), String> {
    run(&["-C", path, "checkout", "--force", branch])
}

pub fn merge_ff_only_in(path: &str, refspec: &str) -> Result<(), String> {
    run(&["-C", path, "merge", "--ff-only", refspec])
}

pub fn current_branch() -> Result<String, String> {
    run_output(&["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string())
}

pub fn list_local_branches_with_upstream() -> Result<Vec<(String, Option<String>)>, String> {
    let output = run_output(&[
        "for-each-ref",
        "--format=%(refname:short)\t%(upstream:short)",
        "refs/heads/",
    ])?;
    Ok(output
        .lines()
        .map(|line| {
            let mut parts = line.splitn(2, '\t');
            let branch = parts.next().unwrap_or("").to_string();
            let upstream = parts
                .next()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            (branch, upstream)
        })
        .collect())
}

/// A local branch with its tip, and its upstream's tip if that ref exists.
pub struct BranchTip {
    pub branch: String,
    pub sha: String,
    /// Short name, e.g. `origin/feature`.
    pub upstream: Option<String>,
    /// Full name, e.g. `refs/remotes/origin/feature`.
    pub upstream_ref: Option<String>,
    /// The upstream's tip when the list was read. For an upstream that is a
    /// local branch this goes stale as soon as that branch moves; see
    /// [`BranchTip::current_upstream_sha`].
    pub upstream_sha: Option<String>,
}

impl BranchTip {
    /// The upstream's tip now. Remote-tracking refs do not move while branches
    /// are synced, so the listed SHA is still right; a local upstream may have
    /// just been fast-forwarded, so it is read again.
    pub fn current_upstream_sha(&self) -> Option<String> {
        match self.upstream_ref.as_deref() {
            Some(u) if u.starts_with("refs/heads/") => super::refs::resolve_ref(u).ok(),
            _ => self.upstream_sha.clone(),
        }
    }
}

/// Every local branch with its tip and its upstream's, read with a single
/// `for-each-ref` rather than resolving each ref on its own.
pub fn list_branch_tips() -> Result<Vec<BranchTip>, String> {
    let output = run_output(&[
        "for-each-ref",
        "--format=%(refname)%09%(objectname)%09%(upstream)%09%(upstream:short)",
        "refs/heads/",
        "refs/remotes/",
    ])?;
    let refs: Vec<Vec<&str>> = output.lines().map(|l| l.split('\t').collect()).collect();
    let tips: std::collections::HashMap<&str, &str> = refs
        .iter()
        .map(|r| (r[0], r.get(1).copied().unwrap_or("")))
        .collect();
    Ok(refs
        .iter()
        .filter_map(|r| {
            let branch = r[0].strip_prefix("refs/heads/")?;
            let upstream_ref = r.get(2).copied().filter(|u| !u.is_empty());
            Some(BranchTip {
                branch: branch.to_string(),
                sha: r.get(1)?.to_string(),
                upstream: r.get(3).filter(|u| !u.is_empty()).map(|u| u.to_string()),
                upstream_ref: upstream_ref.map(str::to_string),
                // An upstream tracked outside refs/remotes/ (a custom refspec)
                // is rare enough to resolve on its own.
                upstream_sha: upstream_ref.and_then(|u| match tips.get(u) {
                    Some(sha) => Some(sha.to_string()),
                    None if u.starts_with("refs/heads/") || u.starts_with("refs/remotes/") => None,
                    None => super::refs::resolve_ref(u).ok(),
                }),
            })
        })
        .collect())
}

pub fn detect_default_branch(remote: bool) -> Result<String, String> {
    if let Ok(raw) = run_output(&["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        let prefix = "refs/remotes/origin/";
        if let Some(branch) = raw.trim().strip_prefix(prefix) {
            return Ok(if remote {
                format!("origin/{branch}")
            } else {
                branch.to_string()
            });
        }
    }

    for candidate in ["main", "master"] {
        if run_output(&["rev-parse", "--verify", candidate]).is_ok() {
            return Ok(candidate.to_string());
        }
    }

    Err("could not detect default branch; use --from to specify one".to_string())
}

pub fn local_branch_for_ref(refname: &str) -> Option<String> {
    if let Some(branch) = refname.strip_prefix("refs/heads/") {
        return run_output(&["rev-parse", "--verify", &format!("refs/heads/{branch}")])
            .ok()
            .map(|_| branch.to_string());
    }

    run_output(&["rev-parse", "--verify", &format!("refs/heads/{refname}")])
        .ok()
        .map(|_| refname.to_string())
}

pub fn is_valid_branch_name(name: &str) -> bool {
    // `check-ref-format --branch` echoes the name on stdout; run_output
    // captures it so it never reaches our stdout.
    run_output(&["check-ref-format", "--branch", name]).is_ok()
}

pub fn ensure_branch_allowed_in_linked_worktree(
    branch: &str,
) -> Result<(), crate::output::CommandError> {
    let default_branch = detect_default_branch(false)?;
    if branch == default_branch {
        return Err(crate::output::CommandError::branch(
            format!(
                "cannot use a linked worktree for the default branch '{default_branch}'; keep it in the main worktree"
            ),
            "default_branch",
            branch,
        ));
    }
    Ok(())
}

pub fn delete_branch_force_in(git_dir: &str, branch: &str) -> Result<(), String> {
    run(&["-C", git_dir, "branch", "-D", branch])
}

pub fn stash_push_in(path: &str, message: &str) -> Result<(), String> {
    run(&[
        "-C",
        path,
        "stash",
        "push",
        "--include-untracked",
        "-m",
        message,
    ])
}

pub fn stash_pop_in(path: &str, stash_ref: &str) -> Result<(), String> {
    run(&["-C", path, "stash", "pop", stash_ref])
}

pub fn has_unpushed_commits(git_dir: &str, branch: &str) -> Result<bool, String> {
    // If there are no remotes, nothing can be "unpushed"
    let no_remotes = run_output(&["-C", git_dir, "remote"])
        .map(|o| o.trim().is_empty())
        .unwrap_or(true);
    if no_remotes {
        return Ok(false);
    }
    Ok(run_output(&[
        "-C",
        git_dir,
        "log",
        branch,
        "--not",
        "--remotes",
        "--oneline",
    ])
    .map(|o| !o.trim().is_empty())
    .unwrap_or(false))
}

pub fn unset_upstream(branch: &str) -> Result<(), String> {
    run(&["branch", "--unset-upstream", branch])
}
