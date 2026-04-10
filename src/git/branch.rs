use super::run::{run, run_output};

pub fn checkout(branch: &str) -> Result<(), String> {
    run(&["checkout", branch])
}

pub fn checkout_new(branch: &str, from: &str) -> Result<(), String> {
    run(&["checkout", "-b", branch, from])
}

pub fn checkout_in(path: &str, branch: &str) -> Result<(), String> {
    run(&["-C", path, "checkout", branch])
}

pub fn current_branch() -> Result<String, String> {
    run_output(&["rev-parse", "--abbrev-ref", "HEAD"]).map(|s| s.trim().to_string())
}

pub fn branch_exists(branch: &str) -> Result<bool, String> {
    Ok(run_output(&["rev-parse", "--verify", &format!("refs/heads/{branch}")]).is_ok())
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

pub fn ensure_branch_allowed_in_linked_worktree(branch: &str) -> Result<(), String> {
    let default_branch = detect_default_branch(false)?;
    if branch == default_branch {
        return Err(format!(
            "cannot use a linked worktree for the default branch '{default_branch}'; keep it in the main worktree"
        ));
    }
    Ok(())
}

pub fn delete_branch_force_in(git_dir: &str, branch: &str) -> Result<(), String> {
    run(&["-C", git_dir, "branch", "-D", branch])
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
