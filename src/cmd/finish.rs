#[derive(clap::Args)]
pub struct FinishArgs {
    /// Branch to finish (defaults to current branch)
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(crate::complete::worktree_branch_completer))]
    pub branch: Option<String>,
    /// Finish every linked worktree's branch and switch the main worktree to
    /// the default branch, skipping whatever can't be finished
    #[arg(long, conflicts_with = "branch")]
    pub all: bool,
    /// Discard local changes: force removal of a linked worktree with modified
    /// or untracked files, or force the switch away in the main worktree
    #[arg(long, short = 'f')]
    pub force: bool,
    /// Preview what would be finished without changing anything
    #[arg(long)]
    pub dry_run: bool,
}

/// How to finish a branch.
#[derive(Clone, Copy)]
pub(crate) struct FinishOptions {
    /// Discard unpushed commits and local changes.
    pub force: bool,
    /// Send the shell to the main worktree if it was in the removed one.
    pub navigate: bool,
    /// Run the checks, but change nothing.
    pub dry_run: bool,
}

/// What `finish` did, reported in `--json` mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FinishData {
    /// Absent for a worktree with a detached HEAD (only under `--all`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) branch: Option<String>,
    /// "main" or "linked" if the branch was checked out, else absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) worktree: Option<&'static str>,
    /// The worktree the branch was checked out in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) path: Option<std::path::PathBuf>,
    /// The branch the main worktree switched to in its place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) switched_to: Option<String>,
    /// Where to continue: the removed worktree no longer exists.
    pub(super) main_worktree: std::path::PathBuf,
    /// The branch and its worktree were already gone; nothing was left to do.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) already_finished: bool,
    /// Nothing was changed: this is what would have been done.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) dry_run: bool,
}

pub fn run(args: FinishArgs) -> Result<(), crate::output::CommandError> {
    if args.all {
        return super::finish_all::run(args.force, args.dry_run);
    }
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
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
                    branch: Some(target_branch),
                    worktree: Some("linked"),
                    path: Some(gone.to_path_buf()),
                    switched_to: None,
                    main_worktree: main_wt.path.clone(),
                    already_finished: true,
                    dry_run: args.dry_run,
                },
            );
        }
        return Ok(());
    }

    let options = FinishOptions {
        force: args.force,
        navigate: true,
        dry_run: args.dry_run,
    };
    let data = finish_branch(&target_branch, options)?;
    if crate::output::json_mode() {
        crate::output::emit_json_success("finish", data);
    }
    Ok(())
}

/// How a caller lets the user get past a refusal to finish: the flag to retry
/// with and what to tell them about it, per refusal.
pub(crate) struct Remedy {
    pub flag: &'static str,
    pub unpushed_hint: &'static str,
    pub uncommitted_hint: &'static str,
}

pub(super) const FORCE: Remedy = Remedy {
    flag: "--force",
    unpushed_hint: "use --force to delete anyway",
    uncommitted_hint: "use --force to discard them",
};

/// Fail if finishing `branch` without `--force` would lose work: commits that
/// exist on no remote, or uncommitted changes in its linked worktree.
pub(crate) fn ensure_finishable(
    branch: &str,
    remedy: &Remedy,
) -> Result<(), crate::output::CommandError> {
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let main_wt_path = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    ensure_no_unpushed_commits(main_wt_path, branch, remedy)?;
    if let Some(wt) = worktrees
        .iter()
        .skip(1)
        .find(|wt| wt.branch.as_deref() == Some(branch))
    {
        ensure_clean_linked_worktree(wt, branch, remedy)?;
    }
    Ok(())
}

fn ensure_no_unpushed_commits(
    main_wt_path: &str,
    branch: &str,
    remedy: &Remedy,
) -> Result<(), crate::output::CommandError> {
    if crate::git::has_unpushed_commits(main_wt_path, branch)? {
        return Err(crate::output::CommandError::with_data(
            format!(
                "branch '{branch}' has unpushed commits; {}",
                remedy.unpushed_hint
            ),
            "unpushed_commits",
            serde_json::json!({ "branch": branch, "retryWith": [remedy.flag] }),
        ));
    }
    Ok(())
}

pub(super) fn ensure_clean_linked_worktree(
    wt: &crate::git::WorktreeEntry,
    branch: &str,
    remedy: &Remedy,
) -> Result<(), crate::output::CommandError> {
    if !wt.path.exists() {
        return Ok(());
    }
    let path = wt.path.to_str().ok_or("worktree path is not valid UTF-8")?;
    if !crate::git::is_worktree_clean(path)? {
        return Err(crate::output::CommandError::uncommitted_changes(
            format!(
                "worktree for '{branch}' at {} has uncommitted changes; {}",
                crate::output::display_path(&wt.path),
                remedy.uncommitted_hint
            ),
            &wt.path,
            &[remedy.flag],
        ));
    }
    Ok(())
}

/// Finish an existing `branch`: remove its linked worktree or switch the main
/// worktree away from it, then delete it.
pub(crate) fn finish_branch(
    target_branch: &str,
    options: FinishOptions,
) -> Result<FinishData, crate::output::CommandError> {
    let FinishOptions {
        force,
        navigate,
        dry_run,
    } = options;
    let would = if dry_run { "Would finish" } else { "Finished" };
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let main_wt_path = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?
        .to_string();
    let cwd = std::env::current_dir()
        .map_err(|e| format!("could not determine current directory: {e}"))?;
    let target_wt = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(target_branch));
    let target_branch = target_branch.to_string();

    if !force {
        ensure_no_unpushed_commits(&main_wt_path, &target_branch, &FORCE)?;
    }

    let mut data = FinishData {
        branch: Some(target_branch.clone()),
        worktree: None,
        path: None,
        switched_to: None,
        main_worktree: main_wt.path.clone(),
        already_finished: false,
        dry_run,
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
            switch_main_worktree(&main_wt_path, &default, options)?;
            crate::output::success(&format!(
                "{would} {}, back on {}",
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
                if !force {
                    ensure_clean_linked_worktree(wt, &target_branch, &FORCE)?;
                }
                if !dry_run {
                    crate::git::remove_worktree(&wt.path, force)?;
                }
            } else if !dry_run {
                // Its directory was deleted without git; drop the stale entry.
                crate::git::prune_worktrees()?;
            }
            if !dry_run && let Ok(root) = crate::git::read_worktree_root() {
                crate::fs::remove_empty_parent_dirs(&wt.path, &root);
            }
            crate::output::success(&format!(
                "{} worktree for {} in {}",
                if dry_run { "Would remove" } else { "Removed" },
                console::style(&target_branch).bold(),
                crate::output::display_path(&wt.path)
            ));
            data.worktree = Some("linked");
            data.path = Some(wt.path.clone());
            if navigate && in_this_wt && !dry_run {
                let canonical = std::fs::canonicalize(&main_wt.path)
                    .map_err(|e| format!("could not canonicalize path: {e}"))?;
                crate::shell_protocol::emit_cd(&canonical);
            }
        }
        None => {
            // Branch exists but is not checked out anywhere — delete it directly.
            if !dry_run {
                crate::git::delete_branch_force_in(&main_wt_path, &target_branch)?;
            }
            crate::output::success(&format!(
                "{} branch {}",
                if dry_run { "Would delete" } else { "Deleted" },
                console::style(&target_branch).bold()
            ));
        }
    }
    if target_wt.is_some() && !dry_run {
        crate::git::delete_branch_force_in(&main_wt_path, &target_branch)?;
    }
    Ok(data)
}

/// Check out `default` in the main worktree at `path`, unless `dry_run`.
pub(super) fn switch_main_worktree(
    path: &str,
    default: &str,
    FinishOptions { force, dry_run, .. }: FinishOptions,
) -> Result<(), String> {
    match (dry_run, force) {
        (true, _) => Ok(()),
        (false, true) => crate::git::checkout_force_in(path, default),
        (false, false) => crate::git::checkout_in(path, default),
    }
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
