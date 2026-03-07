#[derive(clap::Args)]
pub struct NewArgs {
    pub branch: String,
    #[arg(long)]
    pub from: Option<String>,
    #[arg(short = 'w', long)]
    pub worktree: bool,
    #[arg(long)]
    pub no_fetch: bool,
}

pub fn run(args: NewArgs) -> Result<(), String> {
    let from_ref = match args.from {
        Some(r) => r,
        None => detect_default_branch()?,
    };
    fetch_if_needed(args.no_fetch)?;
    if args.worktree {
        create_worktree(&args.branch, &from_ref)
    } else {
        run_git(&["checkout", "-b", &args.branch, &from_ref])
    }
}

fn fetch_if_needed(no_fetch: bool) -> Result<(), String> {
    if no_fetch {
        return Ok(());
    }
    if read_git_config("mate.fetch")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "false" | "no" | "off" | "0"))
        .unwrap_or(false)
    {
        return Ok(());
    }
    let remotes = std::process::Command::new("git")
        .args(["remote"])
        .output()
        .map(|o| {
            o.status
                .success()
                .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
        })
        .ok()
        .flatten()
        .unwrap_or_default();
    if !remotes.lines().any(|r| r.trim() == "origin") {
        return Ok(());
    }
    run_git(&["fetch", "origin"])
}

fn create_worktree(branch: &str, from_ref: &str) -> Result<(), String> {
    let valid = std::process::Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !valid {
        return Err(format!("invalid branch name: {branch:?}"));
    }
    let main_wt = find_main_worktree()?;
    let root = read_worktree_root()?;
    let project_name = main_wt
        .file_name()
        .ok_or("main worktree path has no directory name")?
        .to_str()
        .ok_or("main worktree directory name is not valid UTF-8")?;
    let wt_path = root.join(project_name).join(branch);
    if let Some(parent) = wt_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directories {}: {e}", parent.display()))?;
    }
    let wt_path_str = wt_path
        .to_str()
        .ok_or("worktree path is not valid UTF-8")?;
    run_git(&["worktree", "add", wt_path_str, "-b", branch, from_ref])
}

fn find_main_worktree() -> Result<std::path::PathBuf, String> {
    let output = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err("`git worktree list` failed".to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            return Ok(std::path::PathBuf::from(path));
        }
    }
    Err("could not determine main worktree path".to_string())
}

fn read_worktree_root() -> Result<std::path::PathBuf, String> {
    let value = read_git_config("mate.worktreeRoot").ok_or(
        "mate.worktreeRoot is not configured; set it with: git config mate.worktreeRoot <path>"
            .to_string(),
    )?;
    Ok(expand_tilde(&value))
}

fn read_git_config(key: &str) -> Option<String> {
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

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

fn detect_default_branch() -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout);
        let trimmed = raw.trim();
        let prefix = "refs/remotes/origin/";
        if let Some(branch) = trimmed.strip_prefix(prefix) {
            return Ok(format!("origin/{branch}"));
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

fn run_git(args: &[&str]) -> Result<(), String> {
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
