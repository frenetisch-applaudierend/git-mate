use super::run::{run, run_output};

pub struct WorktreeEntry {
    pub path: std::path::PathBuf,
    pub branch: Option<String>, // short name e.g. "feature/login", None if detached
}

pub fn list_worktrees() -> Result<Vec<WorktreeEntry>, String> {
    let stdout = run_output(&["worktree", "list", "--porcelain"])?;
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

pub fn find_main_worktree() -> Result<std::path::PathBuf, String> {
    list_worktrees()?
        .into_iter()
        .next()
        .map(|wt| wt.path)
        .ok_or_else(|| "could not determine main worktree path".to_string())
}

pub fn current_worktree_root() -> Result<std::path::PathBuf, String> {
    run_output(&["rev-parse", "--show-toplevel"]).map(|s| std::path::PathBuf::from(s.trim()))
}

pub fn is_main_worktree() -> Result<bool, String> {
    let current = current_worktree_root()?;
    let main = find_main_worktree()?;
    Ok(current == main)
}

pub fn read_worktree_root() -> Result<std::path::PathBuf, String> {
    let value = super::config::read_string("mate.worktreeRoot").ok_or(
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

pub fn remove_worktree(path: &std::path::Path, force: bool) -> Result<(), String> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    if force {
        run(&["worktree", "remove", "--force", path_str])
    } else {
        run(&["worktree", "remove", path_str])
    }
}

pub fn is_worktree_clean(path: &str) -> Result<bool, String> {
    run_output(&["-C", path, "status", "--porcelain"]).map(|o| o.is_empty())
}

pub fn worktree_for_branch<'a>(
    branch: &str,
    worktrees: &'a [WorktreeEntry],
) -> Option<&'a WorktreeEntry> {
    worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationTarget {
    MainWorktree,
    LinkedWorktree,
}

impl OperationTarget {
    fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "main" => Some(Self::MainWorktree),
            "linked" => Some(Self::LinkedWorktree),
            _ => None,
        }
    }
}

pub fn default_operation_target() -> Result<OperationTarget, String> {
    match super::config::read_string("mate.defaultBranchMode") {
        Some(value) => OperationTarget::from_config_value(&value).ok_or_else(|| {
            format!(
                "invalid value for mate.defaultBranchMode: {value:?}; expected 'main' or 'linked'"
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
