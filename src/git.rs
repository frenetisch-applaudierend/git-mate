pub fn find_worktree_for_branch(branch: &str) -> Result<Option<std::path::PathBuf>, String> {
    let path = list_worktrees()?
        .into_iter()
        .skip(1)
        .find(|wt| wt.branch.as_deref() == Some(branch))
        .map(|wt| wt.path);
    Ok(path)
}

pub fn add_worktree(
    wt_path: &std::path::Path,
    extra_args: &[&str],
) -> Result<std::path::PathBuf, String> {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationTarget {
    MainWorktree,
    LinkedWorktree,
}

impl OperationTarget {
    fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "main" | "main-worktree" => Some(Self::MainWorktree),
            "worktree" | "linked" | "linked-worktree" => Some(Self::LinkedWorktree),
            _ => None,
        }
    }
}

pub fn default_operation_target() -> Result<OperationTarget, String> {
    match config::read_string("mate.defaultLocation") {
        Some(value) => OperationTarget::from_config_value(&value).ok_or_else(|| {
            format!(
                "invalid value for mate.defaultLocation: {value:?}; expected 'main' or 'worktree'"
            )
        }),
        None => Ok(OperationTarget::MainWorktree),
    }
}

pub fn resolve_operation_target(
    main_worktree: bool,
    linked_worktree: bool,
) -> Result<OperationTarget, String> {
    if main_worktree {
        return Ok(OperationTarget::MainWorktree);
    }
    if linked_worktree {
        return Ok(OperationTarget::LinkedWorktree);
    }
    default_operation_target()
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

pub fn list_remote_tracking_refs() -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short)", "refs/remotes/"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err("git for-each-ref failed".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|s| !s.ends_with("/HEAD"))
        .map(|s| s.to_string())
        .collect())
}

pub fn list_local_branches_with_upstream() -> Result<Vec<(String, Option<String>)>, String> {
    let output = std::process::Command::new("git")
        .args([
            "for-each-ref",
            "--format=%(refname:short)\t%(upstream:short)",
            "refs/heads/",
        ])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err("git for-each-ref failed".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
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

pub fn current_branch() -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err("failed to get current branch".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn stash_push(message: &str) -> Result<String, String> {
    run(&["stash", "push", "-u", "-m", message])?;

    let output = std::process::Command::new("git")
        .args(["stash", "list", "--format=%gd", "-1"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err("created stash but could not determine stash ref".to_string());
    }

    let stash_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stash_ref.is_empty() {
        return Err("created stash but could not determine stash ref".to_string());
    }

    Ok(stash_ref)
}

pub fn resolve_ref(refname: &str) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", refname])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err(format!("failed to resolve ref {refname}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn is_ancestor(ancestor: &str, descendant: &str) -> Result<bool, String> {
    let output = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    Ok(output.status.success())
}

pub fn update_ref(refname: &str, new_sha: &str) -> Result<(), String> {
    run(&["update-ref", refname, new_sha])
}

pub fn delete_branch_force_in(git_dir: &str, branch: &str) -> Result<(), String> {
    run(&["-C", git_dir, "branch", "-D", branch])
}

pub fn has_unpushed_commits(git_dir: &str, branch: &str) -> Result<bool, String> {
    // If there are no remotes, nothing can be "unpushed"
    let remotes = std::process::Command::new("git")
        .args(["-C", git_dir, "remote"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    if remotes.trim().is_empty() {
        return Ok(false);
    }
    let output = std::process::Command::new("git")
        .args(["-C", git_dir, "log", branch, "--not", "--remotes", "--oneline"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Ok(false);
    }
    Ok(!output.stdout.is_empty())
}


pub fn is_worktree_clean(path: &str) -> Result<bool, String> {
    let output = std::process::Command::new("git")
        .args(["-C", path, "status", "--porcelain"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !output.status.success() {
        return Err(format!("failed to check status in {path}"));
    }
    Ok(output.stdout.is_empty())
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

pub fn stash_pop_in(path: &str) -> Result<(), String> {
    run(&["-C", path, "stash", "pop"])
}

pub fn checkout_in(path: &str, branch: &str) -> Result<(), String> {
    run(&["-C", path, "checkout", branch])
}

pub fn remove_worktree(path: &std::path::Path, force: bool) -> Result<(), String> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    if force {
        run(&["worktree", "remove", "--force", path_str])
    } else {
        run(&["worktree", "remove", path_str])
    }
}

pub fn remove_empty_parent_dirs(path: &std::path::Path, stop_at: &std::path::Path) {
    let mut current = path.to_path_buf();
    loop {
        current = match current.parent() {
            Some(p) => p.to_path_buf(),
            None => break,
        };
        if current == stop_at || !current.starts_with(stop_at) {
            break;
        }
        if std::fs::remove_dir(&current).is_err() {
            break;
        }
    }
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
                worktrees.push(WorktreeEntry {
                    path: p,
                    branch: current_branch.take(),
                });
            }
            current_path = Some(std::path::PathBuf::from(path));
            current_branch = None;
        } else if let Some(refs) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(refs.to_string());
        }
    }
    if let Some(p) = current_path {
        worktrees.push(WorktreeEntry {
            path: p,
            branch: current_branch,
        });
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

const COPY_BLACKLIST: &[&str] = &[
    "node_modules",
    "target",
    ".gradle",
    ".m2",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    "obj",
    "bin",
    "vendor",
    ".terraform",
    "coverage",
    ".nyc_output",
    ".cache",
];

fn path_is_blacklisted(rel_path: &str) -> bool {
    rel_path
        .trim_end_matches('/')
        .split('/')
        .any(|c| COPY_BLACKLIST.contains(&c))
}

pub fn copy_ignored_files(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    let src_str = src.to_str().ok_or("source path is not valid UTF-8")?;
    let output = std::process::Command::new("git")
        .args([
            "-C",
            src_str,
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ])
        .output()
        .map_err(|e| format!("failed to list ignored files: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        crate::output::info(&format!(
            "skipping local config copy: git ls-files failed: {}",
            stderr.trim()
        ));
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut copied = 0usize;
    for rel_path in stdout.split('\0').filter(|s| !s.is_empty()) {
        if path_is_blacklisted(rel_path) {
            continue;
        }
        if rel_path.ends_with('/') {
            copied += copy_dir(src, dst, rel_path.trim_end_matches('/'));
        } else {
            copied += copy_file(src, dst, rel_path) as usize;
        }
    }
    if copied > 0 {
        crate::output::info(&format!("Copied {copied} local config file(s) to worktree"));
    }
    Ok(())
}

fn copy_file(src: &std::path::Path, dst: &std::path::Path, rel_path: &str) -> bool {
    let src_file = src.join(rel_path);
    let dst_file = dst.join(rel_path);
    if dst_file.exists() {
        return false;
    }
    if let Some(parent) = dst_file.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    if let Err(e) = std::fs::copy(&src_file, &dst_file) {
        crate::output::info(&format!("could not copy {rel_path}: {e}"));
        return false;
    }
    true
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path, rel_dir: &str) -> usize {
    let src_dir = src.join(rel_dir);
    let Ok(entries) = std::fs::read_dir(&src_dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let rel_path = format!("{rel_dir}/{}", name.to_string_lossy());
        if path_is_blacklisted(&rel_path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            count += copy_dir(src, dst, &rel_path);
        } else if file_type.is_file() {
            count += copy_file(src, dst, &rel_path) as usize;
        }
    }
    count
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
}
