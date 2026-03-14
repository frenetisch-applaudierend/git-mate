pub fn find_worktree_for_branch(branch: &str) -> Result<Option<std::path::PathBuf>, String> {
    let path = list_worktrees()?
        .into_iter()
        .skip(1)
        .find(|wt| wt.branch.as_deref() == Some(branch))
        .map(|wt| wt.path);
    Ok(path)
}

pub fn add_worktree(wt_path: &std::path::Path, extra_args: &[&str]) -> Result<std::path::PathBuf, String> {
    if let Some(parent) = wt_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directories {}: {e}", parent.display()))?;
    }
    let wt_path_str = wt_path.to_str().ok_or("worktree path is not valid UTF-8")?;
    let mut args = vec!["worktree", "add", wt_path_str];
    args.extend_from_slice(extra_args);
    run(&args)?;
    Ok(std::fs::canonicalize(wt_path).unwrap_or_else(|_| wt_path.to_path_buf()))
}

pub fn find_main_worktree() -> Result<std::path::PathBuf, String> {
    list_worktrees()?
        .into_iter()
        .next()
        .map(|wt| wt.path)
        .ok_or_else(|| "could not determine main worktree path".to_string())
}

pub fn read_worktree_root() -> Result<std::path::PathBuf, String> {
    let value = config::read_string("mate.worktreeRoot").ok_or(
        "mate.worktreeRoot is not configured; set it with: git config mate.worktreeRoot <path>"
            .to_string(),
    )?;
    Ok(expand_tilde(&value))
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return std::path::PathBuf::from(home).join(rest);
    }
    std::path::PathBuf::from(path)
}

pub fn current_worktree_root() -> Result<std::path::PathBuf, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err("`git rev-parse --show-toplevel` failed".to_string());
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(std::path::PathBuf::from(path))
}

pub fn is_main_worktree() -> Result<bool, String> {
    let current = current_worktree_root()?;
    let main = find_main_worktree()?;
    Ok(current == main)
}

static VERBOSE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn set_verbose(v: bool) {
    let _ = VERBOSE.set(v);
}

fn is_verbose() -> bool {
    *VERBOSE.get().unwrap_or(&false)
}

pub fn checkout(branch: &str) -> Result<(), String> {
    run(&["checkout", branch])
}

pub fn checkout_new(branch: &str, from: &str) -> Result<(), String> {
    run(&["checkout", "-b", branch, from])
}

pub fn fetch(remote: &str) -> Result<(), String> {
    run(&["fetch", remote])
}

pub fn fetch_all() -> Result<(), String> {
    run(&["fetch", "--all", "--prune"])
}

pub fn pull(extra_args: &[&str]) -> Result<(), String> {
    let mut args = vec!["pull"];
    args.extend_from_slice(extra_args);
    run(&args)
}

pub fn checkout_in(path: &str, branch: &str) -> Result<(), String> {
    run(&["-C", path, "checkout", branch])
}

pub fn remove_worktree(path: &std::path::Path) -> Result<(), String> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    run(&["worktree", "remove", path_str])
}

pub fn delete_branch_in(path: &str, branch: &str) -> Result<(), String> {
    run(&["-C", path, "branch", "-d", branch])
}

fn run(args: &[&str]) -> Result<(), String> {
    if is_verbose() {
        let status = std::process::Command::new("git")
            .args(args)
            .status()
            .map_err(|e| format!("failed to run git: {e}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("`git {}` failed", args.join(" ")))
        };
    }
    let output = std::process::Command::new("git")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Err(format!("`git {}` failed", args.join(" ")))
        } else {
            Err(format!("`git {}` failed: {stderr}", args.join(" ")))
        }
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
        return Err(if stderr.is_empty() {
            "`git worktree list` failed".to_string()
        } else {
            format!("`git worktree list` failed: {stderr}")
        });
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

pub fn worktree_path(branch: &str) -> Result<std::path::PathBuf, String> {
    let main_wt = find_main_worktree()?;
    let root = read_worktree_root()?;
    let project_name = main_wt
        .file_name()
        .ok_or("main worktree path has no directory name")?
        .to_str()
        .ok_or("main worktree directory name is not valid UTF-8")?;
    Ok(root.join(project_name).join(branch))
}

pub fn detect_default_branch(remote: bool) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout);
        let trimmed = raw.trim();
        let prefix = "refs/remotes/origin/";
        if let Some(branch) = trimmed.strip_prefix(prefix) {
            return Ok(if remote {
                format!("origin/{branch}")
            } else {
                branch.to_string()
            });
        }
    }

    for candidate in ["main", "master"] {
        let ok = std::process::Command::new("git")
            .args(["rev-parse", "--verify", candidate])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Ok(candidate.to_string());
        }
    }

    Err("could not detect default branch; use --from to specify one".to_string())
}

pub fn called_from_wrapper() -> bool {
    std::env::var("GIT_MATE_SHELL").is_ok()
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
