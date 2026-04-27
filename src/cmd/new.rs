#[derive(clap::Args)]
pub struct NewArgs {
    pub branch: String,
    #[arg(long, help = "Branch or ref to create from (default: repo default branch)", add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
    pub from: Option<String>,
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
        conflicts_with = "ignore",
        help = "Carry local changes from a dirty checked-out starting point into the new branch"
    )]
    pub stash: bool,
    #[arg(
        long,
        conflicts_with = "stash",
        help = "Start fresh from the committed state and leave source-only local changes behind"
    )]
    pub ignore: bool,
    #[arg(long, help = "Skip fetching from origin before branching")]
    pub no_fetch: bool,
}

pub fn run(args: NewArgs) -> Result<(), String> {
    let from_ref = match args.from {
        Some(r) => r,
        None => crate::git::detect_default_branch(true)?,
    };
    fetch_if_needed(args.no_fetch)?;
    let target = crate::git::resolve_operation_target(args.main_worktree, args.linked_worktree)?;
    let worktrees = crate::git::list_worktrees()?;
    let source_worktree = crate::git::local_branch_for_ref(&from_ref)
        .as_deref()
        .and_then(|branch| crate::git::worktree_for_branch(branch, &worktrees));

    match target {
        crate::git::OperationTarget::LinkedWorktree => create_worktree(
            &args.branch,
            &from_ref,
            source_worktree,
            args.stash,
            args.ignore,
        ),
        crate::git::OperationTarget::MainWorktree => create_main_worktree_branch(
            &args.branch,
            &from_ref,
            &worktrees,
            source_worktree,
            args.stash,
            args.ignore,
        ),
    }
}

fn set_push_tracking(branch: &str) {
    let remotes = std::process::Command::new("git")
        .args(["remote"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    if !remotes.lines().any(|r| r.trim() == "origin") {
        return;
    }
    let _ = std::process::Command::new("git")
        .args(["config", &format!("branch.{branch}.remote"), "origin"])
        .status();
    let _ = std::process::Command::new("git")
        .args([
            "config",
            &format!("branch.{branch}.merge"),
            &format!("refs/heads/{branch}"),
        ])
        .status();
}

fn fetch_if_needed(no_fetch: bool) -> Result<(), String> {
    if no_fetch {
        return Ok(());
    }
    if let Some(val) = crate::git::config::read_string("mate.fetch")
        && matches!(val.to_lowercase().as_str(), "false" | "no" | "off" | "0")
    {
        return Ok(());
    }
    let remotes = std::process::Command::new("git")
        .args(["remote"])
        .output()
        .map(|o| {
            o.status
                .success()
                .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
        })
        .ok()
        .flatten()
        .unwrap_or_default();
    if !remotes.lines().any(|r| r.trim() == "origin") {
        return Ok(());
    }
    crate::git::fetch("origin")
}

struct SourceTransfer<'a> {
    worktree: &'a crate::git::WorktreeEntry,
    stash: Option<crate::cmd::worktree_changes::PreparedStash>,
    copy_local_config: bool,
}

fn create_worktree(
    branch: &str,
    from_ref: &str,
    source_worktree: Option<&crate::git::WorktreeEntry>,
    allow_stash: bool,
    allow_ignore: bool,
) -> Result<(), String> {
    let valid = std::process::Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !valid {
        return Err(format!("invalid branch name: {branch:?}"));
    }
    crate::git::ensure_branch_allowed_in_linked_worktree(branch)?;
    let source_transfer =
        prepare_source_transfer(source_worktree, from_ref, branch, allow_stash, allow_ignore)?;
    let wt_path = crate::git::worktree_path(branch)?;
    let canonical = match crate::git::add_worktree(&wt_path, &["-b", branch, from_ref]) {
        Ok(path) => path,
        Err(e) => {
            return Err(restore_source_transfer(
                source_transfer.as_ref(),
                format!("failed to create worktree for '{branch}': {e}"),
            ));
        }
    };
    set_push_tracking(branch);
    copy_source_local_config(source_transfer.as_ref(), &canonical)?;
    if let Some(stash) = source_transfer.and_then(|transfer| transfer.stash) {
        return crate::cmd::worktree_changes::finish_with_stash_reapply(
            stash,
            &canonical,
            &format!("Created worktree for '{branch}' at {}", canonical.display()),
            &canonical,
        );
    }
    crate::output::success(&format!(
        "Created worktree for '{branch}' at {}",
        canonical.display()
    ));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(())
}

fn create_main_worktree_branch(
    branch: &str,
    from_ref: &str,
    worktrees: &[crate::git::WorktreeEntry],
    source_worktree: Option<&crate::git::WorktreeEntry>,
    allow_stash: bool,
    allow_ignore: bool,
) -> Result<(), String> {
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    let source_is_destination = source_worktree
        .map(|worktree| worktree.path == main_wt.path)
        .unwrap_or(false);
    let main_dirty = !crate::git::is_worktree_clean(main_wt_str)?;

    let source_has_local_changes = match source_worktree {
        Some(worktree) if !source_is_destination => worktree_has_local_changes(&worktree.path)?,
        _ => false,
    };

    if main_dirty && source_has_local_changes {
        return Err(format!(
            "cannot create '{branch}' in the main worktree because both the destination and starting point '{from_ref}' have local changes; clean one worktree first"
        ));
    }

    let destination_stash = if main_dirty && !source_is_destination {
        if allow_ignore {
            return Err(
                "cannot use --ignore when the destination main worktree has uncommitted changes; use --stash or clean the worktree first"
                    .to_string(),
            );
        }
        if !allow_stash {
            return Err(format!(
                "cannot create '{branch}' in the main worktree because it has uncommitted changes; retry with --stash"
            ));
        }
        Some(crate::cmd::worktree_changes::stash_changes(
            &main_wt.path,
            branch,
            "main worktree",
            &format!("git-mate new {branch}"),
        )?)
    } else {
        None
    };

    let source_transfer = if source_is_destination {
        None
    } else {
        prepare_source_transfer(source_worktree, from_ref, branch, allow_stash, allow_ignore)?
    };

    if let Err(e) = crate::git::checkout_new_in(main_wt_str, branch, from_ref) {
        let failure = format!("failed to create branch '{branch}' in the main worktree: {e}");
        let failure = restore_source_transfer(source_transfer.as_ref(), failure);
        return Err(crate::cmd::worktree_changes::restore_source_stash(
            destination_stash.as_ref(),
            &main_wt.path,
            failure,
        ));
    }

    set_push_tracking(branch);
    copy_source_local_config(source_transfer.as_ref(), &main_wt.path)?;

    if let Some(stash) = source_transfer.and_then(|transfer| transfer.stash) {
        return crate::cmd::worktree_changes::finish_with_stash_reapply(
            stash,
            &main_wt.path,
            &format!("Created and switched to branch '{branch}'"),
            &main_wt.path,
        );
    }
    if let Some(stash) = destination_stash {
        return crate::cmd::worktree_changes::finish_with_stash_reapply(
            stash,
            &main_wt.path,
            &format!("Created and switched to branch '{branch}'"),
            &main_wt.path,
        );
    }

    crate::output::success(&format!("Created and switched to branch '{branch}'"));
    if !crate::git::is_main_worktree()? {
        crate::shell_protocol::emit_cd(&main_wt.path);
    }
    Ok(())
}

fn prepare_source_transfer<'a>(
    source_worktree: Option<&'a crate::git::WorktreeEntry>,
    from_ref: &str,
    branch: &str,
    allow_stash: bool,
    allow_ignore: bool,
) -> Result<Option<SourceTransfer<'a>>, String> {
    let Some(source_worktree) = source_worktree else {
        return Ok(None);
    };
    let source_path = &source_worktree.path;
    let source_path_str = source_path
        .to_str()
        .ok_or("source worktree path is not valid UTF-8")?;
    let has_git_changes = !crate::git::is_worktree_clean(source_path_str)?;
    let copy_local_config = crate::fs::has_local_config_files(source_path)?;
    if !has_git_changes && !copy_local_config {
        return Ok(None);
    }
    if allow_ignore {
        return Ok(None);
    }
    if !allow_stash {
        return Err(format!(
            "starting point '{from_ref}' is checked out at {} with local changes; retry with --stash to continue the work or --ignore to start fresh",
            source_worktree.path.display()
        ));
    }
    let stash = if has_git_changes {
        Some(crate::cmd::worktree_changes::stash_changes(
            source_path,
            branch,
            &format!("starting point '{}'", source_worktree.path.display()),
            &format!("git-mate new {branch}"),
        )?)
    } else {
        None
    };

    Ok(Some(SourceTransfer {
        worktree: source_worktree,
        stash,
        copy_local_config,
    }))
}

fn copy_source_local_config(
    source_transfer: Option<&SourceTransfer<'_>>,
    destination: &std::path::Path,
) -> Result<(), String> {
    if let Some(source_transfer) = source_transfer
        && source_transfer.copy_local_config
    {
        crate::fs::copy_ignored_files(&source_transfer.worktree.path, destination)?;
    }
    Ok(())
}

fn restore_source_transfer(
    source_transfer: Option<&SourceTransfer<'_>>,
    failure_message: String,
) -> String {
    if let Some(source_transfer) = source_transfer {
        return crate::cmd::worktree_changes::restore_source_stash(
            source_transfer.stash.as_ref(),
            &source_transfer.worktree.path,
            failure_message,
        );
    }
    failure_message
}

fn worktree_has_local_changes(path: &std::path::Path) -> Result<bool, String> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    Ok(!crate::git::is_worktree_clean(path_str)? || crate::fs::has_local_config_files(path)?)
}
