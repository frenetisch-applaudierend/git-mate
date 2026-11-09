#[derive(clap::Args)]
pub struct CheckoutArgs {
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
    pub branch: Option<String>,
    #[arg(
        short = 'm',
        long,
        conflicts_with = "linked_worktree",
        help = "Force the branch into the main worktree"
    )]
    pub main_worktree: bool,
    #[arg(
        short = 'w',
        long,
        conflicts_with = "main_worktree",
        help = "Force the branch into a linked worktree"
    )]
    pub linked_worktree: bool,
    #[arg(
        long,
        conflicts_with = "start_fresh",
        help = "Bring uncommitted changes along when relocating the branch, even when it is not checked out in the current worktree"
    )]
    pub take_changes: bool,
    #[arg(
        long,
        conflicts_with = "take_changes",
        help = "Leave uncommitted changes in the main worktree when relocating the branch out of it (local config files are still copied)"
    )]
    pub start_fresh: bool,
}

/// What to do with uncommitted changes in the worktree a branch is relocated
/// out of.
#[derive(Clone, Copy, PartialEq)]
enum Changes {
    /// Take them when relocating out of the current worktree; otherwise
    /// require an explicit choice.
    Default,
    Take,
    StartFresh,
}

impl Changes {
    fn from_flags(take_changes: bool, start_fresh: bool) -> Self {
        if take_changes {
            Self::Take
        } else if start_fresh {
            Self::StartFresh
        } else {
            Self::Default
        }
    }
}

/// What `checkout` did, reported in `--json` mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckoutData {
    branch: String,
    /// "main" or "linked": where the branch is checked out now.
    worktree: &'static str,
    path: std::path::PathBuf,
    /// "navigated" (already checked out there), "switched" (checked out in
    /// the main worktree), "created" (new linked worktree) or "moved"
    /// (relocated between the main and a linked worktree).
    action: &'static str,
    /// Uncommitted changes were stashed and reapplied at the destination.
    stashed: bool,
    /// The remotes were fetched because the branch was not known locally.
    fetched: bool,
}

impl CheckoutData {
    fn new(
        branch: &str,
        worktree: &'static str,
        path: &std::path::Path,
        action: &'static str,
    ) -> Self {
        Self {
            branch: branch.to_string(),
            worktree,
            path: path.to_path_buf(),
            action,
            stashed: false,
            fetched: false,
        }
    }
}

pub fn run(args: CheckoutArgs) -> Result<(), crate::output::CommandError> {
    let branch = resolve_branch(&args)?;
    let fetched = fetch_if_unknown(&branch)?;
    prune_missing_worktree(&branch)?;
    let changes = Changes::from_flags(args.take_changes, args.start_fresh);
    let mut data = if args.main_worktree || args.linked_worktree {
        let target = resolve_checkout_target(&args, &branch)?;
        match target {
            crate::git::OperationTarget::LinkedWorktree => {
                ensure_checkout_in_linked_worktree(&branch, changes)
            }
            crate::git::OperationTarget::MainWorktree => {
                ensure_checkout_in_main_worktree(&branch, changes)
            }
        }
    } else {
        default_checkout(&branch)
    }?;
    data.fetched = fetched;
    if crate::output::json_mode() {
        crate::output::emit_json_success("checkout", data);
    }
    Ok(())
}

fn resolve_checkout_target(
    args: &CheckoutArgs,
    branch: &str,
) -> Result<crate::git::OperationTarget, String> {
    if !args.main_worktree
        && !args.linked_worktree
        && branch == crate::git::detect_default_branch(false)?
    {
        return Ok(crate::git::OperationTarget::MainWorktree);
    }

    crate::git::resolve_operation_target(args.main_worktree, args.linked_worktree)
}

fn resolve_branch(args: &CheckoutArgs) -> Result<String, String> {
    if let Some(branch) = args.branch.clone() {
        return Ok(branch);
    }
    if !(args.main_worktree || args.linked_worktree) {
        return Err("branch name is required unless -m or -w is specified".to_string());
    }

    let branch = crate::git::current_branch()?;
    if branch == "HEAD" {
        return Err(
            "cannot infer branch in detached HEAD; specify a branch name explicitly".to_string(),
        );
    }
    Ok(branch)
}

/// Fetch all remotes when `branch` is neither a local ref nor a
/// remote-tracking branch yet, so a branch someone just pushed can be checked
/// out without a separate fetch. Returns whether a fetch happened, and fails
/// when the branch cannot be found even after fetching.
fn fetch_if_unknown(branch: &str) -> Result<bool, crate::output::CommandError> {
    let remotes = crate::git::list_remotes()?;
    if is_known(branch, &remotes) {
        return Ok(false);
    }
    if remotes.is_empty() {
        return Err(crate::output::CommandError::branch(
            format!("branch {branch} not found"),
            "branch_not_found",
            branch,
        ));
    }
    crate::output::info(&format!(
        "{} not found locally, fetching",
        console::style(branch).bold()
    ));
    crate::git::fetch_all()?;
    if !is_known(branch, &remotes) {
        return Err(crate::output::CommandError::branch(
            format!(
                "branch {branch} not found, neither locally nor on {}",
                remotes.join(", ")
            ),
            "branch_not_found",
            branch,
        ));
    }
    Ok(true)
}

/// Whether `branch` names a local ref or a branch on one of the `remotes`.
fn is_known(branch: &str, remotes: &[String]) -> bool {
    crate::git::commit_exists(branch)
        || remotes
            .iter()
            .any(|remote| crate::git::commit_exists(&format!("refs/remotes/{remote}/{branch}")))
}

/// Drop the worktree entry of `branch` when its directory was deleted without
/// git, so the branch is treated as not checked out anywhere instead of
/// navigating into a directory that no longer exists.
fn prune_missing_worktree(branch: &str) -> Result<(), String> {
    let worktrees = crate::git::list_worktrees()?;
    let Some(worktree) = crate::git::worktree_for_branch(branch, &worktrees) else {
        return Ok(());
    };
    if worktree.path.exists() {
        return Ok(());
    }
    crate::output::info(&format!(
        "Worktree for {} in {} no longer exists, removing it",
        console::style(branch).bold(),
        crate::output::display_path(&worktree.path)
    ));
    crate::git::prune_worktrees()
}

fn default_checkout(branch: &str) -> Result<CheckoutData, crate::output::CommandError> {
    let worktrees = crate::git::list_worktrees()?;
    if let Some(worktree) = crate::git::worktree_for_branch(branch, &worktrees) {
        let kind = if worktrees
            .first()
            .is_some_and(|main| main.path == worktree.path)
        {
            "main"
        } else {
            "linked"
        };
        return navigate_to_worktree(branch, &worktree.path, kind);
    }

    let target = resolve_checkout_target(
        &CheckoutArgs {
            branch: Some(branch.to_string()),
            main_worktree: false,
            linked_worktree: false,
            take_changes: false,
            start_fresh: false,
        },
        branch,
    )?;

    match target {
        crate::git::OperationTarget::MainWorktree => checkout_main_worktree(branch),
        crate::git::OperationTarget::LinkedWorktree => checkout_linked_worktree(branch),
    }
}

fn ensure_checkout_in_main_worktree(
    branch: &str,
    changes: Changes,
) -> Result<CheckoutData, crate::output::CommandError> {
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;

    match crate::git::worktree_for_branch(branch, &worktrees) {
        Some(worktree) if worktree.path == main_wt.path => {
            navigate_to_worktree(branch, &main_wt.path, "main")
        }
        Some(worktree) => move_branch_from_linked_to_main(main_wt, worktree, branch, changes),
        None => checkout_main_worktree(branch),
    }
}

fn ensure_checkout_in_linked_worktree(
    branch: &str,
    changes: Changes,
) -> Result<CheckoutData, crate::output::CommandError> {
    crate::git::ensure_branch_allowed_in_linked_worktree(branch)?;

    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;

    match crate::git::worktree_for_branch(branch, &worktrees) {
        Some(worktree) if worktree.path != main_wt.path => {
            navigate_to_worktree(branch, &worktree.path, "linked")
        }
        Some(_) => move_branch_from_main_to_linked(main_wt, branch, changes),
        None => checkout_linked_worktree(branch),
    }
}

fn navigate_to_worktree(
    branch: &str,
    wt_path: &std::path::Path,
    kind: &'static str,
) -> Result<CheckoutData, crate::output::CommandError> {
    let canonical = std::fs::canonicalize(wt_path).unwrap_or_else(|_| wt_path.to_path_buf());
    crate::output::info(&format!(
        "{} is already checked out in {}",
        console::style(branch).bold(),
        crate::output::display_path(&canonical)
    ));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(CheckoutData::new(branch, kind, &canonical, "navigated"))
}

fn checkout_main_worktree(branch: &str) -> Result<CheckoutData, crate::output::CommandError> {
    let main_wt = crate::git::find_main_worktree()?;
    if !crate::git::is_main_worktree()? {
        let main_str = main_wt
            .to_str()
            .ok_or("main worktree path is not valid UTF-8")?;
        crate::git::checkout_in(main_str, branch)?;
        crate::shell_protocol::emit_cd(&main_wt);
        crate::output::success(&switched_message(branch));
        return Ok(CheckoutData::new(branch, "main", &main_wt, "switched"));
    }
    crate::git::checkout(branch)?;
    crate::output::success(&switched_message(branch));
    Ok(CheckoutData::new(branch, "main", &main_wt, "switched"))
}

fn checkout_linked_worktree(branch: &str) -> Result<CheckoutData, crate::output::CommandError> {
    let wt_path = crate::git::worktree_path(branch)?;

    if wt_path.is_dir() {
        if wt_path.join(".git").exists() {
            let canonical = std::fs::canonicalize(&wt_path).unwrap_or_else(|_| wt_path.clone());
            crate::shell_protocol::emit_cd(&canonical);
            crate::output::info(&format!(
                "Worktree for {} already exists in {}",
                console::style(branch).bold(),
                crate::output::display_path(&wt_path)
            ));
            return Ok(CheckoutData::new(branch, "linked", &canonical, "navigated"));
        } else {
            return Err(format!(
                "cannot create worktree at {}: directory already exists but does not appear to be a git worktree",
                wt_path.display()
            ).into());
        }
    } else if wt_path.exists() {
        return Err(format!(
            "cannot create worktree at {}: path already exists and is not a directory",
            wt_path.display()
        )
        .into());
    }

    crate::git::ensure_branch_allowed_in_linked_worktree(branch)?;
    let canonical = crate::git::add_worktree(&wt_path, &[branch])?;
    let main_wt = crate::git::find_main_worktree()?;
    crate::fs::copy_ignored_files(&main_wt, &canonical)?;
    crate::output::success(&checked_out_message(branch, &canonical));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(CheckoutData::new(branch, "linked", &canonical, "created"))
}

fn move_branch_from_main_to_linked(
    main_wt: &crate::git::WorktreeEntry,
    branch: &str,
    changes: Changes,
) -> Result<CheckoutData, crate::output::CommandError> {
    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    let stash = stash_source_changes(
        &main_wt.path,
        branch,
        changes,
        "main worktree",
        &format!(
            "cannot relocate '{branch}' out of the main worktree while it has uncommitted changes; retry with --take-changes to bring them along or --start-fresh to leave them in the main worktree"
        ),
        &["--take-changes", "--start-fresh"],
    )?;
    let left_changes = stash.is_none() && !crate::git::is_worktree_clean(main_wt_str)?;

    let default_branch = crate::git::detect_default_branch(false)?;
    let wt_path = crate::git::worktree_path(branch)?;
    ensure_worktree_path_available(&wt_path)?;

    if let Err(e) = crate::git::checkout_in(main_wt_str, &default_branch) {
        return Err(crate::cmd::worktree_changes::restore_source_stash(
            stash.as_ref(),
            &main_wt.path,
            format!("failed to switch main worktree to '{default_branch}': {e}"),
        )
        .into());
    }

    let canonical = match crate::git::add_worktree(&wt_path, &[branch]) {
        Ok(path) => path,
        Err(e) => {
            if let Err(restore_err) = crate::git::checkout_in(main_wt_str, branch) {
                return Err(format!(
                    "failed to create linked worktree for '{branch}': {e}; also failed to switch main worktree back to '{branch}': {restore_err}"
                ).into());
            }
            if stash.is_some() {
                return Err(crate::cmd::worktree_changes::restore_source_stash(
                    stash.as_ref(),
                    &main_wt.path,
                    format!("failed to create linked worktree for '{branch}': {e}"),
                )
                .into());
            }
            return Err(format!("failed to create linked worktree for '{branch}': {e}").into());
        }
    };

    crate::fs::copy_ignored_files(&main_wt.path, &canonical)?;
    if left_changes {
        crate::output::info(&format!(
            "Left uncommitted changes in the main worktree, now on {}",
            console::style(&default_branch).cyan()
        ));
    }
    let mut data = CheckoutData::new(branch, "linked", &canonical, "moved");
    if let Some(stash) = stash {
        crate::cmd::worktree_changes::finish_with_stash_reapply(
            stash,
            &canonical,
            &checked_out_message(branch, &canonical),
            &canonical,
        )?;
        data.stashed = true;
        return Ok(data);
    }

    crate::output::success(&checked_out_message(branch, &canonical));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(data)
}

fn move_branch_from_linked_to_main(
    main_wt: &crate::git::WorktreeEntry,
    linked_wt: &crate::git::WorktreeEntry,
    branch: &str,
    changes: Changes,
) -> Result<CheckoutData, crate::output::CommandError> {
    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    if !crate::git::is_worktree_clean(main_wt_str)? {
        return Err(crate::output::CommandError::uncommitted_changes(
            format!(
                "cannot check out '{branch}' in the main worktree: main worktree has uncommitted changes"
            ),
            &main_wt.path,
            &[],
        ));
    }

    let linked_wt_str = linked_wt
        .path
        .to_str()
        .ok_or("linked worktree path is not valid UTF-8")?;
    if changes == Changes::StartFresh && !crate::git::is_worktree_clean(linked_wt_str)? {
        return Err(crate::output::CommandError::uncommitted_changes(
            format!(
                "cannot use --start-fresh when relocating '{branch}' out of {}: the linked worktree is removed, so its uncommitted changes cannot stay there; use --take-changes or commit them first",
                linked_wt.path.display()
            ),
            &linked_wt.path,
            &["--take-changes"],
        ));
    }

    let stash = stash_source_changes(
        &linked_wt.path,
        branch,
        changes,
        "linked worktree",
        &format!(
            "cannot relocate '{branch}' out of {} while it has uncommitted changes; retry with --take-changes to bring them along",
            linked_wt.path.display()
        ),
        &["--take-changes"],
    )?;

    // Ignored local config files would be deleted with the linked worktree.
    if let Err(e) = crate::fs::copy_ignored_files(&linked_wt.path, &main_wt.path) {
        return Err(crate::cmd::worktree_changes::restore_source_stash(
            stash.as_ref(),
            &linked_wt.path,
            e,
        )
        .into());
    }

    if let Err(e) = crate::git::remove_worktree(&linked_wt.path, false) {
        return Err(crate::cmd::worktree_changes::restore_source_stash(
            stash.as_ref(),
            &linked_wt.path,
            e,
        )
        .into());
    }
    if let Ok(root) = crate::git::read_worktree_root() {
        crate::fs::remove_empty_parent_dirs(&linked_wt.path, &root);
    }

    if let Err(e) = crate::git::checkout_in(main_wt_str, branch) {
        let restored_path = match crate::git::add_worktree(&linked_wt.path, &[branch]) {
            Ok(path) => path,
            Err(restore_err) => {
                return Err(format!(
                    "removed linked worktree for '{branch}' but failed to check it out in the main worktree: {e}; also failed to restore linked worktree at {}: {restore_err}",
                    linked_wt.path.display()
                ).into());
            }
        };
        if let Some(stash) = stash.as_ref() {
            return Err(crate::cmd::worktree_changes::restore_source_stash(
                Some(stash),
                &restored_path,
                format!(
                    "removed linked worktree for '{branch}' but failed to check it out in the main worktree: {e}"
                ),
            ).into());
        }
        return Err(format!(
            "removed linked worktree for '{branch}' but failed to check it out in the main worktree: {e}"
        ).into());
    }

    let mut data = CheckoutData::new(branch, "main", &main_wt.path, "moved");
    if let Some(stash) = stash {
        crate::cmd::worktree_changes::finish_with_stash_reapply(
            stash,
            &main_wt.path,
            &switched_message(branch),
            &main_wt.path,
        )?;
        data.stashed = true;
        return Ok(data);
    }

    crate::output::success(&switched_message(branch));
    crate::shell_protocol::emit_cd(&main_wt.path);
    Ok(data)
}

fn ensure_worktree_path_available(
    wt_path: &std::path::Path,
) -> Result<(), crate::output::CommandError> {
    if wt_path.is_dir() {
        return Err(format!(
            "cannot create worktree at {}: directory already exists",
            wt_path.display()
        )
        .into());
    }
    if wt_path.exists() {
        return Err(format!(
            "cannot create worktree at {}: path already exists and is not a directory",
            wt_path.display()
        )
        .into());
    }
    Ok(())
}

/// Stash the uncommitted changes of the worktree a branch is relocated out
/// of, if they should come along: by default only when it is the current
/// worktree. `failure_message` is the error when an explicit choice is needed.
fn stash_source_changes(
    path: &std::path::Path,
    branch: &str,
    changes: Changes,
    label: &str,
    failure_message: &str,
    retry_with: &[&str],
) -> Result<Option<crate::cmd::worktree_changes::PreparedStash>, crate::output::CommandError> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    if crate::git::is_worktree_clean(path_str)? {
        return Ok(None);
    }
    match changes {
        Changes::Take => {}
        Changes::StartFresh => return Ok(None),
        Changes::Default if is_current_worktree(path) => {}
        Changes::Default => {
            return Err(crate::output::CommandError::uncommitted_changes(
                failure_message.to_string(),
                path,
                retry_with,
            ));
        }
    }

    crate::cmd::worktree_changes::stash_changes(
        path,
        branch,
        label,
        &format!("git-mate checkout {branch}"),
    )
    .map(Some)
    .map_err(Into::into)
}

fn is_current_worktree(path: &std::path::Path) -> bool {
    let canonical =
        |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    crate::git::current_worktree_root().is_ok_and(|root| canonical(&root) == canonical(path))
}

fn switched_message(branch: &str) -> String {
    format!("Switched to branch {}", console::style(branch).bold())
}

fn checked_out_message(branch: &str, path: &std::path::Path) -> String {
    format!(
        "Checked out {} in {}",
        console::style(branch).bold(),
        crate::output::display_path(path)
    )
}
