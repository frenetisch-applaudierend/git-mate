#[derive(clap::Args)]
pub struct FinishArgs {
    /// Branch to finish (defaults to current branch)
    pub branch: Option<String>,
    #[arg(long)]
    pub delete_branch: bool,
}

pub fn run(args: FinishArgs) -> Result<(), String> {
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let main_wt_path = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?
        .to_string();
    let cwd = std::env::current_dir()
        .map_err(|e| format!("could not determine current directory: {e}"))?;

    let current_wt = worktrees
        .iter()
        .filter(|wt| cwd.starts_with(&wt.path))
        .max_by_key(|wt| wt.path.components().count());

    let target_branch = match args.branch {
        Some(ref b) => b.clone(),
        None => current_wt
            .and_then(|wt| wt.branch.clone())
            .ok_or("could not determine current branch (detached HEAD?)")?,
    };

    let target_wt = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(&target_branch));

    match target_wt {
        Some(wt) if wt.path == main_wt.path => {
            let default = detect_default_branch()?;
            if target_branch == default {
                return Err(format!(
                    "nothing to finish: {target_branch:?} is the default branch"
                ));
            }
            crate::git::run(&["-C", &main_wt_path, "checkout", &default])?;
        }
        Some(wt) => {
            let wt_path_str = wt
                .path
                .to_str()
                .ok_or("worktree path is not valid UTF-8")?;
            let in_this_wt = cwd.starts_with(&wt.path);
            crate::git::run(&["worktree", "remove", wt_path_str])?;
            if in_this_wt && crate::git::called_from_wrapper() {
                println!("_MATE_CD:{}", main_wt.path.display());
            }
        }
        None => {
            if !args.delete_branch {
                return Err(format!(
                    "branch {target_branch:?} is not checked out anywhere; use --delete-branch to delete it directly"
                ));
            }
        }
    }

    if args.delete_branch {
        crate::git::run(&["-C", &main_wt_path, "branch", "-d", &target_branch])?;
    }

    Ok(())
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
            return Ok(branch.to_string());
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

    Err("could not detect default branch".to_string())
}
