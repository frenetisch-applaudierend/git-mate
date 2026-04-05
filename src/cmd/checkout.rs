#[derive(clap::Args)]
pub struct CheckoutArgs {
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
    pub branch: String,
    #[arg(short = 'm', long, conflicts_with = "linked_worktree", help = "Force the branch into the main worktree")]
    pub main_worktree: bool,
    #[arg(short = 'w', long, conflicts_with = "main_worktree", help = "Force the branch into a linked worktree")]
    pub linked_worktree: bool,
}

pub fn run(args: CheckoutArgs) -> Result<(), String> {
    match crate::git::resolve_operation_target(args.main_worktree, args.linked_worktree)? {
        crate::git::OperationTarget::LinkedWorktree => checkout_worktree(&args.branch),
        crate::git::OperationTarget::MainWorktree => checkout_in_place(&args.branch),
    }
}

fn checkout_in_place(branch: &str) -> Result<(), String> {
    if let Some(wt_path) = crate::git::find_worktree_for_branch(branch)? {
        let canonical = std::fs::canonicalize(&wt_path)
            .unwrap_or_else(|_| wt_path.clone());
        crate::output::info(&format!("Branch '{}' is already checked out at {}", branch, canonical.display()));
        crate::shell_protocol::emit_cd(&canonical);
        return Ok(());
    }
    let main_wt = crate::git::find_main_worktree()?;
    if !crate::git::is_main_worktree()? {
        let main_str = main_wt.to_str().ok_or("main worktree path is not valid UTF-8")?;
        crate::git::checkout_in(main_str, branch)?;
        crate::shell_protocol::emit_cd(&main_wt);
        crate::output::success(&format!("Switched to branch '{branch}'"));
        return Ok(());
    }
    crate::git::checkout(branch)?;
    crate::output::success(&format!("Switched to branch '{branch}'"));
    Ok(())
}

fn checkout_worktree(branch: &str) -> Result<(), String> {
    if let Some(wt_path) = crate::git::find_worktree_for_branch(branch)? {
        let canonical = std::fs::canonicalize(&wt_path)
            .unwrap_or_else(|_| wt_path.clone());
        crate::shell_protocol::emit_cd(&canonical);
        crate::output::info(&format!("Branch '{}' is already checked out at {}", branch, wt_path.display()));
        return Ok(());
    }
    let wt_path = crate::git::worktree_path(branch)?;

    if wt_path.is_dir() {
        if wt_path.join(".git").exists() {
            let canonical = std::fs::canonicalize(&wt_path)
                .unwrap_or_else(|_| wt_path.clone());
            crate::shell_protocol::emit_cd(&canonical);
            crate::output::info(&format!("worktree already exists at {}", wt_path.display()));
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

    crate::git::ensure_branch_allowed_in_linked_worktree(branch)?;
    let canonical = crate::git::add_worktree(&wt_path, &[branch])?;
    let main_wt = crate::git::find_main_worktree()?;
    crate::fs::copy_ignored_files(&main_wt, &canonical)?;
    crate::output::success(&format!("Checked out '{branch}' at {}", canonical.display()));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(())
}
