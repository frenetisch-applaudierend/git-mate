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
        help = "Stash changes in the source worktree before relocating, then reapply them in the destination"
    )]
    pub stash: bool,
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
        }
    }
}

pub fn run(args: CheckoutArgs) -> Result<(), String> {
    let branch = resolve_branch(&args)?;
    let data = if args.main_worktree || args.linked_worktree {
        let target = resolve_checkout_target(&args, &branch)?;
        match target {
            crate::git::OperationTarget::LinkedWorktree => {
                ensure_checkout_in_linked_worktree(&branch, args.stash)
            }
            crate::git::OperationTarget::MainWorktree => {
                ensure_checkout_in_main_worktree(&branch, args.stash)
            }
        }
    } else {
        default_checkout(&branch)
    }?;
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

fn default_checkout(branch: &str) -> Result<CheckoutData, String> {
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
            stash: false,
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
    allow_stash: bool,
) -> Result<CheckoutData, String> {
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;

    match crate::git::worktree_for_branch(branch, &worktrees) {
        Some(worktree) if worktree.path == main_wt.path => {
            navigate_to_worktree(branch, &main_wt.path, "main")
        }
        Some(worktree) => move_branch_from_linked_to_main(main_wt, worktree, branch, allow_stash),
        None => checkout_main_worktree(branch),
    }
}

fn ensure_checkout_in_linked_worktree(
    branch: &str,
    allow_stash: bool,
) -> Result<CheckoutData, String> {
    crate::git::ensure_branch_allowed_in_linked_worktree(branch)?;

    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;

    match crate::git::worktree_for_branch(branch, &worktrees) {
        Some(worktree) if worktree.path != main_wt.path => {
            navigate_to_worktree(branch, &worktree.path, "linked")
        }
        Some(_) => move_branch_from_main_to_linked(main_wt, branch, allow_stash),
        None => checkout_linked_worktree(branch),
    }
}

fn navigate_to_worktree(
    branch: &str,
    wt_path: &std::path::Path,
    kind: &'static str,
) -> Result<CheckoutData, String> {
    let canonical = std::fs::canonicalize(wt_path).unwrap_or_else(|_| wt_path.to_path_buf());
    crate::output::info(&format!(
        "{} is already checked out in {}",
        console::style(branch).bold(),
        crate::output::display_path(&canonical)
    ));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(CheckoutData::new(branch, kind, &canonical, "navigated"))
}

fn checkout_main_worktree(branch: &str) -> Result<CheckoutData, String> {
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

fn checkout_linked_worktree(branch: &str) -> Result<CheckoutData, String> {
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
    crate::output::success(&checked_out_message(branch, &canonical));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(CheckoutData::new(branch, "linked", &canonical, "created"))
}

fn move_branch_from_main_to_linked(
    main_wt: &crate::git::WorktreeEntry,
    branch: &str,
    allow_stash: bool,
) -> Result<CheckoutData, String> {
    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    let stash = stash_source_changes(
        &main_wt.path,
        branch,
        allow_stash,
        "main worktree",
        &format!(
            "cannot relocate '{branch}' out of the main worktree while it has uncommitted changes"
        ),
    )?;

    let default_branch = crate::git::detect_default_branch(false)?;
    let wt_path = crate::git::worktree_path(branch)?;
    ensure_worktree_path_available(&wt_path)?;

    if let Err(e) = crate::git::checkout_in(main_wt_str, &default_branch) {
        return Err(crate::cmd::worktree_changes::restore_source_stash(
            stash.as_ref(),
            &main_wt.path,
            format!("failed to switch main worktree to '{default_branch}': {e}"),
        ));
    }

    let canonical = match crate::git::add_worktree(&wt_path, &[branch]) {
        Ok(path) => path,
        Err(e) => {
            if let Err(restore_err) = crate::git::checkout_in(main_wt_str, branch) {
                return Err(format!(
                    "failed to create linked worktree for '{branch}': {e}; also failed to switch main worktree back to '{branch}': {restore_err}"
                ));
            }
            if stash.is_some() {
                return Err(crate::cmd::worktree_changes::restore_source_stash(
                    stash.as_ref(),
                    &main_wt.path,
                    format!("failed to create linked worktree for '{branch}': {e}"),
                ));
            }
            return Err(format!(
                "failed to create linked worktree for '{branch}': {e}"
            ));
        }
    };

    crate::fs::copy_ignored_files(&main_wt.path, &canonical)?;
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
    allow_stash: bool,
) -> Result<CheckoutData, String> {
    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    if !crate::git::is_worktree_clean(main_wt_str)? {
        return Err(format!(
            "cannot check out '{branch}' in the main worktree: main worktree has uncommitted changes"
        ));
    }

    let stash = stash_source_changes(
        &linked_wt.path,
        branch,
        allow_stash,
        "linked worktree",
        &format!(
            "cannot relocate '{branch}' out of {} while it has uncommitted changes",
            linked_wt.path.display()
        ),
    )?;

    if let Err(e) = crate::git::remove_worktree(&linked_wt.path, false) {
        return Err(crate::cmd::worktree_changes::restore_source_stash(
            stash.as_ref(),
            &linked_wt.path,
            e,
        ));
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
                ));
            }
        };
        if let Some(stash) = stash.as_ref() {
            return Err(crate::cmd::worktree_changes::restore_source_stash(
                Some(stash),
                &restored_path,
                format!(
                    "removed linked worktree for '{branch}' but failed to check it out in the main worktree: {e}"
                ),
            ));
        }
        return Err(format!(
            "removed linked worktree for '{branch}' but failed to check it out in the main worktree: {e}"
        ));
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

fn stash_source_changes(
    path: &std::path::Path,
    branch: &str,
    allow_stash: bool,
    label: &str,
    failure_message: &str,
) -> Result<Option<crate::cmd::worktree_changes::PreparedStash>, String> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    if crate::git::is_worktree_clean(path_str)? {
        return Ok(None);
    }
    if !allow_stash {
        return Err(format!("{failure_message}; retry with --stash"));
    }

    crate::cmd::worktree_changes::stash_changes(
        path,
        branch,
        label,
        &format!("git-mate checkout {branch}"),
    )
    .map(Some)
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
