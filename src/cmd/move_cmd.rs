#[derive(clap::Args)]
pub struct MoveArgs {
    #[arg(long, help = "Stash tracked and untracked changes before moving")]
    pub stash: bool,
}

pub fn run(args: MoveArgs) -> Result<(), String> {
    if !crate::git::is_main_worktree()? {
        return Err("`mate move` can only be run from the main worktree".to_string());
    }

    let main_wt = crate::git::find_main_worktree()?;
    let main_wt_str = main_wt
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    let branch = crate::git::current_branch()?;
    if branch == "HEAD" {
        return Err("cannot move detached HEAD; check out a branch first".to_string());
    }

    let default_branch = crate::git::detect_default_branch(false)?;
    if branch == default_branch {
        return Err(format!("cannot move the default branch '{default_branch}'"));
    }

    let wt_path = crate::git::worktree_path(&branch)?;
    ensure_worktree_path_available(&wt_path)?;

    let stash_ref = maybe_stash_changes(main_wt_str, args.stash)?;

    crate::git::checkout_in(main_wt_str, &default_branch)?;
    let canonical = match crate::git::add_worktree(&wt_path, &[&branch]) {
        Ok(path) => path,
        Err(e) => {
            return rollback_move_failure(main_wt_str, &branch, stash_ref.as_deref(), e);
        }
    };

    crate::fs::copy_ignored_files(&main_wt, &canonical)?;

    if let Some(stash_ref) = stash_ref.as_deref() {
        let canonical_str = canonical
            .to_str()
            .ok_or("worktree path is not valid UTF-8")?;
        if let Err(e) = crate::git::stash_pop_in(canonical_str) {
            crate::shell_protocol::emit_cd(&canonical);
            return Err(format!(
                "Moved '{branch}' to {} but failed to restore stashed changes: {e}. The stash was kept as {stash_ref}.",
                canonical.display()
            ));
        }
        crate::output::info("Restored stashed changes in new worktree");
    }

    crate::output::success(&format!("Moved '{branch}' to {}", canonical.display()));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(())
}

fn maybe_stash_changes(main_wt: &str, allow_stash: bool) -> Result<Option<String>, String> {
    if crate::git::is_worktree_clean(main_wt)? {
        return Ok(None);
    }
    if !allow_stash {
        return Err(
            "worktree has uncommitted changes; re-run `mate move --stash` to move them too"
                .to_string(),
        );
    }

    let stash_ref = crate::git::stash_push("mate move")?;
    crate::output::info(&format!("Stashed local changes as {stash_ref}"));
    Ok(Some(stash_ref))
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

fn rollback_move_failure(
    main_wt: &str,
    branch: &str,
    stash_ref: Option<&str>,
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

    if let Some(stash_ref) = stash_ref {
        match crate::git::stash_pop_in(main_wt) {
            Ok(()) => details.push("restored stashed changes in the main worktree".to_string()),
            Err(e) => details.push(format!(
                "also failed to restore stashed changes in the main worktree: {e}; the stash was kept as {stash_ref}"
            )),
        }
    }

    Err(details.join("; "))
}
