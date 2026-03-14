#[derive(clap::Args)]
pub struct FinishArgs {
    /// Branch to finish (defaults to current branch)
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
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
            let default = crate::git::detect_default_branch(false)?;
            if target_branch == default {
                return Err(format!(
                    "nothing to finish: {target_branch:?} is the default branch"
                ));
            }
            crate::git::checkout_in(&main_wt_path, &default)?;
            crate::output::success(&format!("Finished '{target_branch}', back on '{default}'"));
        }
        Some(wt) => {
            let in_this_wt = cwd.starts_with(&wt.path);
            crate::git::remove_worktree(&wt.path)?;
            crate::output::success(&format!("Removed worktree for '{target_branch}'"));
            if in_this_wt {
                let canonical = std::fs::canonicalize(&main_wt.path)
                    .map_err(|e| format!("could not canonicalize path: {e}"))?;
                crate::output::emit_cd(&canonical);
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
        crate::git::delete_branch_in(&main_wt_path, &target_branch)?;
        crate::output::success(&format!("Deleted branch '{target_branch}'"));
    }

    Ok(())
}
