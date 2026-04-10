#[derive(clap::Args)]
pub struct MoveArgs {
    /// Branch to move (defaults to current branch)
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
    pub branch: Option<String>,
}

pub fn run(args: MoveArgs) -> Result<(), String> {
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let current_root = crate::git::current_worktree_root()?;
    let current_wt = worktrees
        .iter()
        .find(|wt| wt.path == current_root)
        .ok_or_else(|| {
            format!(
                "could not determine current worktree for {}",
                current_root.display()
            )
        })?;
    let branch = match args.branch {
        Some(branch) => branch,
        None => current_wt
            .branch
            .clone()
            .ok_or("cannot move detached HEAD; check out a branch first")?,
    };

    let default_branch = crate::git::detect_default_branch(false)?;
    if branch == default_branch {
        return Err(format!("cannot move the default branch '{default_branch}'"));
    }

    let target_wt = resolve_target_worktree(&branch, &worktrees)?;
    let target_wt_str = target_wt
        .path
        .to_str()
        .ok_or("worktree path is not valid UTF-8")?;
    if !crate::git::is_worktree_clean(target_wt_str)? {
        return Err(format!(
            "cannot move '{branch}': worktree at {} has uncommitted changes",
            target_wt.path.display()
        ));
    }

    if target_wt.path == main_wt.path {
        move_from_main_to_linked(
            main_wt,
            &branch,
            &default_branch,
            current_wt.path == main_wt.path,
        )
    } else {
        move_from_linked_to_main(
            main_wt,
            target_wt,
            &branch,
            &default_branch,
            current_wt.path == target_wt.path,
        )
    }
}

fn resolve_target_worktree<'a>(
    branch: &str,
    worktrees: &'a [crate::git::WorktreeEntry],
) -> Result<&'a crate::git::WorktreeEntry, String> {
    if let Some(worktree) = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
    {
        return Ok(worktree);
    }

    if crate::git::branch_exists(branch)? {
        return Err(format!(
            "branch '{branch}' exists but is not checked out in any worktree"
        ));
    }

    Err(format!("branch '{branch}' does not exist"))
}

fn move_from_main_to_linked(
    main_wt: &crate::git::WorktreeEntry,
    branch: &str,
    default_branch: &str,
    emit_cd: bool,
) -> Result<(), String> {
    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    let wt_path = crate::git::worktree_path(branch)?;
    ensure_worktree_path_available(&wt_path)?;

    crate::git::checkout_in(main_wt_str, default_branch)?;
    let canonical = match crate::git::add_worktree(&wt_path, &[branch]) {
        Ok(path) => path,
        Err(e) => {
            return rollback_move_from_main_failure(main_wt_str, branch, e);
        }
    };

    crate::fs::copy_ignored_files(&main_wt.path, &canonical)?;
    crate::output::success(&format!("Moved '{branch}' to {}", canonical.display()));
    if emit_cd {
        crate::shell_protocol::emit_cd(&canonical);
    }
    Ok(())
}

fn move_from_linked_to_main(
    main_wt: &crate::git::WorktreeEntry,
    linked_wt: &crate::git::WorktreeEntry,
    branch: &str,
    default_branch: &str,
    emit_cd: bool,
) -> Result<(), String> {
    let main_branch = main_wt
        .branch
        .as_deref()
        .ok_or("cannot move into the main worktree while it is in detached HEAD")?;
    if main_branch != default_branch {
        return Err(format!(
            "cannot move '{branch}' into the main worktree while main is on '{main_branch}'; switch main back to '{default_branch}' first"
        ));
    }

    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    if !crate::git::is_worktree_clean(main_wt_str)? {
        return Err(format!(
            "cannot move '{branch}' into the main worktree: main worktree has uncommitted changes"
        ));
    }

    let expected_path = crate::git::worktree_path(branch)?;
    if normalize_path(&linked_wt.path) != normalize_path(&expected_path) {
        return Err(format!(
            "branch '{branch}' is checked out at {}, expected {}; refusing to move from an unexpected linked worktree path",
            linked_wt.path.display(),
            expected_path.display()
        ));
    }

    crate::git::remove_worktree(&linked_wt.path, false)?;
    if let Ok(root) = crate::git::read_worktree_root() {
        crate::fs::remove_empty_parent_dirs(&linked_wt.path, &root);
    }

    if let Err(e) = crate::git::checkout_in(main_wt_str, branch) {
        return rollback_move_to_main_failure(main_wt, linked_wt, branch, emit_cd, e);
    }

    crate::output::success(&format!("Moved '{branch}' to {}", main_wt.path.display()));
    if emit_cd {
        crate::shell_protocol::emit_cd(&main_wt.path);
    }
    Ok(())
}

fn ensure_worktree_path_available(wt_path: &std::path::Path) -> Result<(), String> {
    if wt_path.is_dir() {
        return Err(format!(
            "cannot create worktree at {}: directory already exists",
            wt_path.display()
        ));
    }
    if wt_path.exists() {
        return Err(format!(
            "cannot create worktree at {}: path already exists and is not a directory",
            wt_path.display()
        ));
    }
    Ok(())
}

fn rollback_move_from_main_failure(
    main_wt: &str,
    branch: &str,
    cause: String,
) -> Result<(), String> {
    let mut details = vec![format!("failed to create worktree for '{branch}': {cause}")];

    match crate::git::checkout_in(main_wt, branch) {
        Ok(()) => details.push(format!("switched main worktree back to '{branch}'")),
        Err(e) => {
            details.push(format!(
                "also failed to switch main worktree back to '{branch}': {e}"
            ));
            return Err(details.join("; "));
        }
    }

    Err(details.join("; "))
}

fn rollback_move_to_main_failure(
    main_wt: &crate::git::WorktreeEntry,
    linked_wt: &crate::git::WorktreeEntry,
    branch: &str,
    emit_cd: bool,
    cause: String,
) -> Result<(), String> {
    let mut details = vec![format!(
        "removed linked worktree for '{branch}' but failed to check it out in the main worktree: {cause}"
    )];

    match crate::git::add_worktree(&linked_wt.path, &[branch]) {
        Ok(path) => details.push(format!("restored linked worktree at {}", path.display())),
        Err(e) => {
            details.push(format!(
                "also failed to restore linked worktree at {}: {e}",
                linked_wt.path.display()
            ));
            if emit_cd {
                crate::shell_protocol::emit_cd(&main_wt.path);
            }
        }
    }

    Err(details.join("; "))
}

fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
