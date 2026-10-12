#[derive(clap::Args)]
pub struct NewArgs {
    pub branch: String,
    #[arg(long, help = "Branch or ref to create from; `.` is the current HEAD (default: repo default branch)", add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
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
        conflicts_with = "start_fresh",
        help = "Bring uncommitted changes into the new branch even when they are not in the current worktree's branch"
    )]
    pub take_changes: bool,
    #[arg(
        long,
        conflicts_with = "take_changes",
        help = "Leave uncommitted changes where they are and start from the committed state (local config files are still copied)"
    )]
    pub start_fresh: bool,
    #[arg(long, help = "Skip fetching from origin before branching")]
    pub no_fetch: bool,
}

/// What to do with uncommitted changes in the starting point's worktree.
#[derive(Clone, Copy, PartialEq)]
enum Changes {
    /// Take them when starting from the current worktree's branch; otherwise
    /// require an explicit choice.
    Default,
    Take,
    StartFresh,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NewData {
    branch: String,
    from: String,
    worktree: &'static str,
    path: std::path::PathBuf,
    fetched: bool,
    upstream: Option<String>,
    carried_from: Option<std::path::PathBuf>,
    copied_config_files: usize,
}

/// The result of creating the branch, before any stashed changes are
/// reapplied.
struct Created {
    worktree: &'static str,
    path: std::path::PathBuf,
    message: String,
    upstream: Option<String>,
    stash: Option<crate::cmd::worktree_changes::PreparedStash>,
    carried_from: Option<std::path::PathBuf>,
    copied_config_files: usize,
}

pub fn run(args: NewArgs) -> Result<(), crate::output::CommandError> {
    let changes = if args.take_changes {
        Changes::Take
    } else if args.start_fresh {
        Changes::StartFresh
    } else {
        Changes::Default
    };
    let from_ref = resolve_from(args.from)?;
    let fetched = fetch_if_needed(args.no_fetch)?;
    let target = crate::git::resolve_operation_target(args.main_worktree, args.linked_worktree)?;
    let worktrees = crate::git::list_worktrees()?;
    let current_root = canonical(&crate::git::current_worktree_root()?);
    let source_worktree = crate::git::local_branch_for_ref(&from_ref)
        .as_deref()
        .and_then(|branch| crate::git::worktree_for_branch(branch, &worktrees));
    let source_is_current = source_worktree.is_some_and(|wt| canonical(&wt.path) == current_root);

    let created = match target {
        crate::git::OperationTarget::LinkedWorktree => create_worktree(
            &args.branch,
            &from_ref,
            source_worktree,
            changes,
            source_is_current,
        ),
        crate::git::OperationTarget::MainWorktree => create_main_worktree_branch(
            &args.branch,
            &from_ref,
            &worktrees,
            source_worktree,
            changes,
            source_is_current,
        ),
    }?;

    let reapplied = match created.stash {
        Some(stash) => crate::cmd::worktree_changes::reapply_stash(stash, &created.path),
        None => Ok(()),
    };
    if canonical(&created.path) != current_root {
        crate::shell_protocol::emit_cd(&created.path);
    }
    let mut data = NewData {
        branch: args.branch,
        from: from_ref,
        worktree: created.worktree,
        path: created.path,
        fetched,
        upstream: created.upstream,
        carried_from: created.carried_from,
        copied_config_files: created.copied_config_files,
    };

    if let Err(e) = reapplied {
        data.carried_from = None;
        return Err(crate::output::CommandError::with_data(
            format!("{}, but {e}", created.message),
            "stash_reapply_failed",
            data,
        ));
    }

    if let Some(from) = data
        .carried_from
        .as_ref()
        .filter(|from| **from != data.path)
    {
        crate::output::info(&format!("Took uncommitted changes from {}", from.display()));
    }
    crate::output::success(&created.message);
    if crate::output::json_mode() {
        crate::output::emit_json_success("new", data);
    }
    Ok(())
}

/// Resolve `--from`: absent means the default branch, `.` the current HEAD
/// (its branch, or the commit when detached).
fn resolve_from(from: Option<String>) -> Result<String, String> {
    match from.as_deref() {
        None => crate::git::detect_default_branch(true),
        Some(".") => match crate::git::current_branch()? {
            branch if branch == "HEAD" => crate::git::resolve_ref("HEAD"),
            branch => Ok(branch),
        },
        Some(r) => Ok(r.to_string()),
    }
}

fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Point the new branch's push target at `origin/<branch>` and return it,
/// or `None` when there is no `origin` remote.
fn set_push_tracking(branch: &str) -> Option<String> {
    if !has_origin_remote() {
        return None;
    }
    // `git config` prints nothing when setting a value.
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
    Some(format!("origin/{branch}"))
}

fn has_origin_remote() -> bool {
    std::process::Command::new("git")
        .args(["remote"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|r| r.trim() == "origin")
        })
        .unwrap_or(false)
}

/// Fetch from `origin` unless disabled; returns whether a fetch happened.
fn fetch_if_needed(no_fetch: bool) -> Result<bool, String> {
    if no_fetch {
        return Ok(false);
    }
    if let Some(val) = crate::git::config::read_string("mate.fetch")
        && matches!(val.to_lowercase().as_str(), "false" | "no" | "off" | "0")
    {
        return Ok(false);
    }
    if !has_origin_remote() {
        return Ok(false);
    }
    crate::git::fetch("origin")?;
    Ok(true)
}

struct SourceTransfer<'a> {
    worktree: &'a crate::git::WorktreeEntry,
    stash: Option<crate::cmd::worktree_changes::PreparedStash>,
    copy_local_config: bool,
}

impl SourceTransfer<'_> {
    /// Split into the stash to reapply and the worktree it came from.
    fn into_stash(
        self,
    ) -> (
        Option<crate::cmd::worktree_changes::PreparedStash>,
        Option<std::path::PathBuf>,
    ) {
        let carried_from = self.stash.as_ref().map(|_| self.worktree.path.clone());
        (self.stash, carried_from)
    }
}

fn create_worktree(
    branch: &str,
    from_ref: &str,
    source_worktree: Option<&crate::git::WorktreeEntry>,
    changes: Changes,
    source_is_current: bool,
) -> Result<Created, String> {
    if !crate::git::is_valid_branch_name(branch) {
        return Err(format!("invalid branch name: {branch:?}"));
    }
    crate::git::ensure_branch_allowed_in_linked_worktree(branch)?;
    let source_transfer = prepare_source_transfer(
        source_worktree,
        from_ref,
        branch,
        changes,
        source_is_current,
    )?;
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
    let upstream = set_push_tracking(branch);
    let copied_config_files = copy_source_local_config(source_transfer.as_ref(), &canonical)?;
    let (stash, carried_from) = source_transfer
        .map(SourceTransfer::into_stash)
        .unwrap_or((None, None));
    Ok(Created {
        worktree: "linked",
        message: format!("Created worktree for '{branch}' at {}", canonical.display()),
        path: canonical,
        upstream,
        stash,
        carried_from,
        copied_config_files,
    })
}

fn create_main_worktree_branch(
    branch: &str,
    from_ref: &str,
    worktrees: &[crate::git::WorktreeEntry],
    source_worktree: Option<&crate::git::WorktreeEntry>,
    changes: Changes,
    source_is_current: bool,
) -> Result<Created, String> {
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let main_wt_str = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?;
    let source_is_destination = source_worktree
        .map(|worktree| worktree.path == main_wt.path)
        .unwrap_or(false);
    let main_dirty = !crate::git::is_worktree_clean(main_wt_str)?;

    let source_dirty = match source_worktree {
        Some(worktree) if !source_is_destination => !is_clean(&worktree.path)?,
        _ => false,
    };

    if main_dirty && source_dirty {
        return Err(format!(
            "cannot create '{branch}' in the main worktree because both the destination and starting point '{from_ref}' have uncommitted changes; clean one worktree first"
        ));
    }

    if main_dirty && source_is_destination && changes == Changes::StartFresh {
        return Err(format!(
            "cannot use --start-fresh when creating '{branch}' in place: the main worktree's uncommitted changes would come along; commit or stash them first"
        ));
    }

    let destination_stash = if main_dirty && !source_is_destination {
        match changes {
            Changes::StartFresh => {
                return Err(
                    "cannot use --start-fresh when the main worktree has uncommitted changes; use --take-changes or clean the worktree first"
                        .to_string(),
                );
            }
            Changes::Default => {
                return Err(format!(
                    "cannot create '{branch}' in the main worktree because it has uncommitted changes; retry with --take-changes to bring them into the new branch"
                ));
            }
            Changes::Take => Some(crate::cmd::worktree_changes::stash_changes(
                &main_wt.path,
                branch,
                "main worktree",
                &format!("git-mate new {branch}"),
            )?),
        }
    } else {
        None
    };

    let source_transfer = if source_is_destination {
        None
    } else {
        prepare_source_transfer(
            source_worktree,
            from_ref,
            branch,
            changes,
            source_is_current,
        )?
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

    let upstream = set_push_tracking(branch);
    let copied_config_files = copy_source_local_config(source_transfer.as_ref(), &main_wt.path)?;

    // At most one stash exists: a dirty source and a dirty destination are
    // rejected above.
    let (stash, carried_from) = match (source_transfer, destination_stash) {
        (Some(transfer), _) if transfer.stash.is_some() => transfer.into_stash(),
        (_, Some(stash)) => (Some(stash), Some(main_wt.path.clone())),
        _ if main_dirty => (None, Some(main_wt.path.clone())),
        _ => (None, None),
    };
    Ok(Created {
        worktree: "main",
        path: main_wt.path.clone(),
        message: format!("Created and switched to branch '{branch}'"),
        upstream,
        stash,
        carried_from,
        copied_config_files,
    })
}

/// Decide what moves out of the worktree where the starting point is checked
/// out. Local config files are always copied; uncommitted changes are taken
/// by default only when that worktree is the current one.
fn prepare_source_transfer<'a>(
    source_worktree: Option<&'a crate::git::WorktreeEntry>,
    from_ref: &str,
    branch: &str,
    changes: Changes,
    source_is_current: bool,
) -> Result<Option<SourceTransfer<'a>>, String> {
    let Some(source_worktree) = source_worktree else {
        return Ok(None);
    };
    let source_path = &source_worktree.path;
    let has_git_changes = !is_clean(source_path)?;
    let copy_local_config = crate::fs::has_local_config_files(source_path)?;

    let take = match changes {
        Changes::Take => true,
        Changes::StartFresh => false,
        Changes::Default if has_git_changes && !source_is_current => {
            return Err(format!(
                "starting point '{from_ref}' is checked out at {} with uncommitted changes; retry with --take-changes to bring them into the new branch or --start-fresh to leave them there",
                source_path.display()
            ));
        }
        Changes::Default => true,
    };

    let stash = if has_git_changes && take {
        Some(crate::cmd::worktree_changes::stash_changes(
            source_path,
            branch,
            &format!("starting point '{}'", source_path.display()),
            &format!("git-mate new {branch}"),
        )?)
    } else {
        None
    };
    if stash.is_none() && !copy_local_config {
        return Ok(None);
    }

    Ok(Some(SourceTransfer {
        worktree: source_worktree,
        stash,
        copy_local_config,
    }))
}

fn copy_source_local_config(
    source_transfer: Option<&SourceTransfer<'_>>,
    destination: &std::path::Path,
) -> Result<usize, String> {
    match source_transfer {
        Some(source_transfer) if source_transfer.copy_local_config => {
            crate::fs::copy_ignored_files(&source_transfer.worktree.path, destination)
        }
        _ => Ok(0),
    }
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

fn is_clean(path: &std::path::Path) -> Result<bool, String> {
    let path_str = path.to_str().ok_or("worktree path is not valid UTF-8")?;
    crate::git::is_worktree_clean(path_str)
}
