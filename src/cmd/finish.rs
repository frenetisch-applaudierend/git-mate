#[derive(clap::Args)]
pub struct FinishArgs {
    /// Branch to finish (defaults to current branch)
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(crate::complete::worktree_branch_completer))]
    pub branch: Option<String>,
    /// Discard local changes: force removal of a linked worktree with modified
    /// or untracked files, or force the switch away in the main worktree
    #[arg(long, short = 'f')]
    pub force: bool,
}

/// What `finish` did, reported in `--json` mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FinishData {
    branch: String,
    /// "main" or "linked" if the branch was checked out, else absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree: Option<&'static str>,
    /// The worktree the branch was checked out in.
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<std::path::PathBuf>,
    /// The branch the main worktree switched to in its place.
    #[serde(skip_serializing_if = "Option::is_none")]
    switched_to: Option<String>,
    /// Where to continue: the removed worktree no longer exists.
    main_worktree: std::path::PathBuf,
    /// The branch and its worktree were already gone; nothing was left to do.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    already_finished: bool,
}

pub fn run(args: FinishArgs) -> Result<(), crate::output::CommandError> {
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

    let target_branch = match (args.branch, crate::cwd::vanished()) {
        (Some(b), _) => b,
        (None, Some(gone)) => vanished_branch(gone, &worktrees)?,
        (None, None) => current_wt
            .and_then(|wt| wt.branch.clone())
            .ok_or("could not determine current branch (detached HEAD?)")?,
    };

    let target_wt = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(&target_branch));

    if target_wt.is_none() && !crate::git::commit_exists(&format!("refs/heads/{target_branch}")) {
        let Some(gone) = crate::cwd::vanished() else {
            return Err(crate::output::CommandError::branch(
                format!("branch {target_branch} not found"),
                "branch_not_found",
                &target_branch,
            ));
        };
        // Finished from another shell, which removed the worktree this one
        // was in.
        crate::output::success(&format!(
            "{} is already finished",
            console::style(&target_branch).bold()
        ));
        if crate::output::json_mode() {
            crate::output::emit_json_success(
                "finish",
                FinishData {
                    branch: target_branch,
                    worktree: Some("linked"),
                    path: Some(gone.to_path_buf()),
                    switched_to: None,
                    main_worktree: main_wt.path.clone(),
                    already_finished: true,
                },
            );
        }
        return Ok(());
    }

    if !args.force && crate::git::has_unpushed_commits(&main_wt_path, &target_branch)? {
        return Err(crate::output::CommandError::with_data(
            format!("branch '{target_branch}' has unpushed commits; use --force to delete anyway"),
            "unpushed_commits",
            serde_json::json!({ "branch": target_branch, "retryWith": ["--force"] }),
        ));
    }

    let mut data = FinishData {
        branch: target_branch.clone(),
        worktree: None,
        path: None,
        switched_to: None,
        main_worktree: main_wt.path.clone(),
        already_finished: false,
    };
    match target_wt {
        Some(wt) if wt.path == main_wt.path => {
            let default = crate::git::detect_default_branch(false)?;
            if target_branch == default {
                return Err(crate::output::CommandError::branch(
                    format!("nothing to finish: {target_branch:?} is the default branch"),
                    "default_branch",
                    &target_branch,
                ));
            }
            if args.force {
                crate::git::checkout_force_in(&main_wt_path, &default)?;
            } else {
                crate::git::checkout_in(&main_wt_path, &default)?;
            }
            crate::output::success(&format!(
                "Finished {}, back on {}",
                console::style(&target_branch).bold(),
                console::style(&default).cyan()
            ));
            data.worktree = Some("main");
            data.path = Some(wt.path.clone());
            data.switched_to = Some(default);
        }
        Some(wt) => {
            let in_this_wt = cwd.starts_with(&wt.path);
            if wt.path.exists() {
                let path = wt.path.to_str().ok_or("worktree path is not valid UTF-8")?;
                if !args.force && !crate::git::is_worktree_clean(path)? {
                    return Err(crate::output::CommandError::uncommitted_changes(
                        format!(
                            "worktree for '{target_branch}' at {} has uncommitted changes; use --force to discard them",
                            crate::output::display_path(&wt.path)
                        ),
                        &wt.path,
                        &["--force"],
                    ));
                }
                crate::git::remove_worktree(&wt.path, args.force)?;
            } else {
                // Its directory was deleted without git; drop the stale entry.
                crate::git::prune_worktrees()?;
            }
            if let Ok(root) = crate::git::read_worktree_root() {
                crate::fs::remove_empty_parent_dirs(&wt.path, &root);
            }
            crate::output::success(&format!(
                "Removed worktree for {} in {}",
                console::style(&target_branch).bold(),
                crate::output::display_path(&wt.path)
            ));
            data.worktree = Some("linked");
            data.path = Some(wt.path.clone());
            if in_this_wt {
                let canonical = std::fs::canonicalize(&main_wt.path)
                    .map_err(|e| format!("could not canonicalize path: {e}"))?;
                crate::shell_protocol::emit_cd(&canonical);
            }
        }
        None => {
            // Branch exists but is not checked out anywhere — delete it directly.
            crate::git::delete_branch_force_in(&main_wt_path, &target_branch)?;
            crate::output::success(&format!(
                "Deleted branch {}",
                console::style(&target_branch).bold()
            ));
        }
    }
    if target_wt.is_some() {
        crate::git::delete_branch_force_in(&main_wt_path, &target_branch)?;
    }

    if crate::output::json_mode() {
        crate::output::emit_json_success("finish", data);
    }
    Ok(())
}

/// The branch of the worktree the command was started in, which no longer
/// exists: a still-registered worktree at that path, or else the branch its
/// path encodes under `<mate.worktreeRoot>/<project>/`.
fn vanished_branch(
    gone: &std::path::Path,
    worktrees: &[crate::git::WorktreeEntry],
) -> Result<String, String> {
    if let Some(branch) = worktrees
        .iter()
        .skip(1)
        .filter(|wt| gone.starts_with(&wt.path))
        .max_by_key(|wt| wt.path.components().count())
        .and_then(|wt| wt.branch.clone())
    {
        return Ok(branch);
    }
    let unknown = || {
        format!(
            "could not tell which branch {} belonged to; name the branch to finish",
            crate::output::display_path(gone)
        )
    };
    let placeholder = crate::git::worktree_path("_").map_err(|_| unknown())?;
    let container = placeholder.parent().ok_or_else(unknown)?;
    let container = std::fs::canonicalize(container).unwrap_or_else(|_| container.to_path_buf());
    gone.strip_prefix(&container)
        .ok()
        .and_then(|rel| rel.to_str())
        .filter(|rel| !rel.is_empty())
        .map(str::to_string)
        .ok_or_else(unknown)
}
