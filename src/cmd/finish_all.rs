use super::finish::{
    FORCE, FinishData, FinishOptions, ensure_clean_linked_worktree, finish_branch,
    switch_main_worktree,
};
use crate::output::CommandError;

/// What `finish --all` did, reported in `--json` mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FinishAllData {
    /// Each worktree that was finished, shaped like `finish`'s result.
    finished: Vec<FinishData>,
    /// Each worktree that was left alone, and why.
    skipped: Vec<Skipped>,
    main_worktree: std::path::PathBuf,
    /// Nothing was changed: this is what would have been done.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    dry_run: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Skipped {
    /// Absent for a worktree with a detached HEAD.
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    /// "main" or "linked".
    worktree: &'static str,
    path: std::path::PathBuf,
    /// The error code `finish` would have failed with.
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_with: Option<serde_json::Value>,
}

/// Finish the branch of every linked worktree, then switch the main worktree
/// to the default branch, finishing the branch it was on. Each worktree goes
/// through the same checks as `finish`; one that fails them is skipped and
/// reported, and the others are still finished.
pub fn run(force: bool, dry_run: bool) -> Result<(), CommandError> {
    let options = FinishOptions {
        force,
        navigate: true,
        dry_run,
    };
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let default = crate::git::detect_default_branch(false)?;
    let cwd = std::env::current_dir()
        .map_err(|e| format!("could not determine current directory: {e}"))?;

    let mut data = FinishAllData {
        finished: Vec::new(),
        skipped: Vec::new(),
        main_worktree: main_wt.path.clone(),
        dry_run,
    };
    let mut record =
        |wt: &crate::git::WorktreeEntry, kind, result: Result<_, CommandError>| match result {
            Ok(Some(finished)) => data.finished.push(finished),
            Ok(None) => {}
            Err(e) => {
                let skipped = if dry_run { "Would skip" } else { "Skipped" };
                crate::output::info(&format!("{skipped}: {}", e.message()));
                data.skipped.push(Skipped {
                    branch: wt.branch.clone(),
                    worktree: kind,
                    path: wt.path.clone(),
                    code: e.code(),
                    message: e.message().to_string(),
                    retry_with: e.data().and_then(|d| d.get("retryWith")).cloned(),
                });
            }
        };

    for wt in worktrees.iter().skip(1) {
        let result = ensure_no_operation(wt, force).and_then(|()| match &wt.branch {
            Some(branch) => finish_branch(branch, options),
            None => remove_detached(wt, options, &cwd, &main_wt.path),
        });
        record(wt, "linked", result.map(Some));
    }

    let result = ensure_no_operation(main_wt, force).and_then(|()| match &main_wt.branch {
        Some(branch) if *branch == default => Ok(None),
        Some(branch) => finish_branch(branch, options).map(Some),
        None => switch_detached_main(main_wt, &default, options).map(Some),
    });
    record(main_wt, "main", result);

    if data.finished.is_empty() && data.skipped.is_empty() {
        crate::output::success("Nothing to finish");
    }
    if crate::output::json_mode() {
        crate::output::emit_json_success("finish", data);
    }
    Ok(())
}

/// Refuse, without `force`, to touch a worktree in the middle of a rebase,
/// merge or the like: removing it, or switching away, would abandon it.
fn ensure_no_operation(wt: &crate::git::WorktreeEntry, force: bool) -> Result<(), CommandError> {
    let Some(path) = wt.path.to_str() else {
        return Err("worktree path is not valid UTF-8".into());
    };
    match crate::git::operation_in_progress(path) {
        Some(operation) if !force && wt.path.exists() => Err(CommandError::with_data(
            format!(
                "a {operation} is in progress in {}; finish or abort it, or use --force",
                crate::output::display_path(&wt.path)
            ),
            "operation_in_progress",
            serde_json::json!({ "path": wt.path, "operation": operation, "retryWith": ["--force"] }),
        )),
        _ => Ok(()),
    }
}

/// Refuse, without `force`, to leave a detached HEAD whose commits nothing
/// else refers to.
fn ensure_no_orphaned_commits(
    wt: &crate::git::WorktreeEntry,
    force: bool,
) -> Result<(), CommandError> {
    let path = wt.path.to_str().ok_or("worktree path is not valid UTF-8")?;
    if !force && crate::git::has_unreferenced_commits(path)? {
        return Err(CommandError::with_data(
            format!(
                "the detached HEAD in {} has commits on no branch; use --force to discard them",
                crate::output::display_path(&wt.path)
            ),
            "unpushed_commits",
            serde_json::json!({ "path": wt.path, "retryWith": ["--force"] }),
        ));
    }
    Ok(())
}

/// Remove a linked worktree with a detached HEAD: there is no branch to delete.
fn remove_detached(
    wt: &crate::git::WorktreeEntry,
    FinishOptions { force, dry_run, .. }: FinishOptions,
    cwd: &std::path::Path,
    main_path: &std::path::Path,
) -> Result<FinishData, CommandError> {
    if wt.path.exists() {
        ensure_no_orphaned_commits(wt, force)?;
        if !force {
            ensure_clean_linked_worktree(wt, "detached HEAD", &FORCE)?;
        }
        if !dry_run {
            crate::git::remove_worktree(&wt.path, force)?;
        }
    } else if !dry_run {
        crate::git::prune_worktrees()?;
    }
    if !dry_run && let Ok(root) = crate::git::read_worktree_root() {
        crate::fs::remove_empty_parent_dirs(&wt.path, &root);
    }
    crate::output::success(&format!(
        "{} detached worktree in {}",
        if dry_run { "Would remove" } else { "Removed" },
        crate::output::display_path(&wt.path)
    ));
    if cwd.starts_with(&wt.path) && !dry_run {
        let canonical = std::fs::canonicalize(main_path)
            .map_err(|e| format!("could not canonicalize path: {e}"))?;
        crate::shell_protocol::emit_cd(&canonical);
    }
    Ok(FinishData {
        branch: None,
        worktree: Some("linked"),
        path: Some(wt.path.clone()),
        switched_to: None,
        main_worktree: main_path.to_path_buf(),
        already_finished: false,
        dry_run,
    })
}

/// Switch the main worktree from a detached HEAD to the default branch.
fn switch_detached_main(
    wt: &crate::git::WorktreeEntry,
    default: &str,
    options: FinishOptions,
) -> Result<FinishData, CommandError> {
    let FinishOptions { force, dry_run, .. } = options;
    ensure_no_orphaned_commits(wt, force)?;
    let path = wt.path.to_str().ok_or("worktree path is not valid UTF-8")?;
    switch_main_worktree(path, default, options)?;
    crate::output::success(&format!(
        "{} the main worktree to {}",
        if dry_run { "Would switch" } else { "Switched" },
        console::style(default).cyan()
    ));
    Ok(FinishData {
        branch: None,
        worktree: Some("main"),
        path: Some(wt.path.clone()),
        switched_to: Some(default.to_string()),
        main_worktree: wt.path.clone(),
        already_finished: false,
        dry_run,
    })
}
