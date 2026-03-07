pub fn run(args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new("git")
        .args(args)
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`git {}` failed", args.join(" ")))
    }
}

pub struct WorktreeEntry {
    pub path: std::path::PathBuf,
    pub branch: Option<String>, // short name e.g. "feature/login", None if detached
}

pub fn list_worktrees() -> Result<Vec<WorktreeEntry>, String> {
    let output = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            return Err("`git worktree list` failed".to_string());
        } else {
            return Err(format!("`git worktree list` failed: {stderr}"));
        }
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path: Option<std::path::PathBuf> = None;
    let mut current_branch: Option<String> = None;

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(p) = current_path.take() {
                worktrees.push(WorktreeEntry { path: p, branch: current_branch.take() });
            }
            current_path = Some(std::path::PathBuf::from(path));
            current_branch = None;
        } else if let Some(refs) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(refs.to_string());
        }
    }
    if let Some(p) = current_path {
        worktrees.push(WorktreeEntry { path: p, branch: current_branch });
    }
    Ok(worktrees)
}

pub mod config {
    pub fn read_string(key: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["config", "--get", key])
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    pub fn read_bool(key: &str) -> Option<bool> {
        let value = read_string(key)?;
        match value.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        }
    }
}
