#[derive(clap::Args)]
pub struct CheckoutArgs {
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
    pub branch: String,
    #[arg(short = 'w', long)]
    pub worktree: bool,
}

pub fn run(args: CheckoutArgs) -> Result<(), String> {
    if args.worktree {
        checkout_worktree(&args.branch)
    } else {
        checkout_in_place(&args.branch)
    }
}

fn find_worktree_for_branch(branch: &str) -> Result<Option<std::path::PathBuf>, String> {
    let path = crate::git::list_worktrees()?
        .into_iter()
        .skip(1) // skip main worktree
        .find(|wt| wt.branch.as_deref() == Some(branch))
        .map(|wt| wt.path);
    Ok(path)
}

fn checkout_in_place(branch: &str) -> Result<(), String> {
    if !crate::git::is_main_worktree()? {
        return Err(
            "checkout is only supported from the main worktree; use --worktree to open this branch in a linked worktree"
                .to_string(),
        );
    }
    if let Some(wt_path) = find_worktree_for_branch(branch)? {
        println!("Branch '{}' is already checked out at {}", branch, wt_path.display());
        return Ok(());
    }
    crate::git::run(&["checkout", branch])
}

fn checkout_worktree(branch: &str) -> Result<(), String> {
    if let Some(wt_path) = find_worktree_for_branch(branch)? {
        println!("Branch '{}' is already checked out at {}", branch, wt_path.display());
        return Ok(());
    }
    let wt_path = crate::git::worktree_path(branch)?;

    if wt_path.is_dir() {
        if wt_path.join(".git").exists() {
            let canonical = std::fs::canonicalize(&wt_path)
                .unwrap_or_else(|_| wt_path.clone());
            crate::output::emit_cd(&canonical);
            println!("worktree already exists at {}", wt_path.display());
            return Ok(());
        } else {
            return Err(format!(
                "cannot create worktree at {}: directory already exists but does not appear to be a git worktree",
                wt_path.display()
            ));
        }
    } else if wt_path.exists() {
        return Err(format!(
            "cannot create worktree at {}: path already exists and is not a directory",
            wt_path.display()
        ));
    }

    if let Some(parent) = wt_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directories {}: {e}", parent.display()))?;
    }

    let wt_path_str = wt_path.to_str().ok_or("worktree path is not valid UTF-8")?;
    crate::git::run(&["worktree", "add", wt_path_str, branch])?;
    let canonical = std::fs::canonicalize(&wt_path)
        .unwrap_or_else(|_| wt_path.clone());
    crate::output::emit_cd(&canonical);
    Ok(())
}
