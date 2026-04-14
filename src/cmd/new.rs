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
    #[arg(long, help = "Skip fetching from origin before branching")]
    pub no_fetch: bool,
}

pub fn run(args: NewArgs) -> Result<(), String> {
    let from_ref = match args.from {
        Some(r) => r,
        None => crate::git::detect_default_branch(true)?,
    };
    fetch_if_needed(args.no_fetch)?;
    match crate::git::resolve_operation_target(args.main_worktree, args.linked_worktree)? {
        crate::git::OperationTarget::LinkedWorktree => create_worktree(&args.branch, &from_ref),
        crate::git::OperationTarget::MainWorktree => {
            crate::git::checkout_new(&args.branch, &from_ref)?;
            set_push_tracking(&args.branch);
            crate::output::success(&format!("Created and switched to branch '{}'", args.branch));
            Ok(())
        }
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

fn create_worktree(branch: &str, from_ref: &str) -> Result<(), String> {
    let valid = std::process::Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !valid {
        return Err(format!("invalid branch name: {branch:?}"));
    }
    crate::git::ensure_branch_allowed_in_linked_worktree(branch)?;
    let wt_path = crate::git::worktree_path(branch)?;
    let canonical = crate::git::add_worktree(&wt_path, &["-b", branch, from_ref])?;
    set_push_tracking(branch);
    let main_wt = crate::git::find_main_worktree()?;
    crate::fs::copy_ignored_files(&main_wt, &canonical)?;
    crate::output::success(&format!(
        "Created worktree for '{branch}' at {}",
        canonical.display()
    ));
    crate::shell_protocol::emit_cd(&canonical);
    Ok(())
}
