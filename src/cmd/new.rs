#[derive(clap::Args)]
pub struct NewArgs {
    pub branch: String,
    #[arg(long, help = "Branch or ref to create from (default: repo default branch)", add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
    pub from: Option<String>,
    #[arg(short = 'w', long, help = "Create a linked worktree instead of a checkout")]
    pub worktree: bool,
    #[arg(long, help = "Skip fetching from origin before branching")]
    pub no_fetch: bool,
}

pub fn run(args: NewArgs) -> Result<(), String> {
    let from_ref = match args.from {
        Some(r) => r,
        None => crate::git::detect_default_branch(true)?,
    };
    fetch_if_needed(args.no_fetch)?;
    if args.worktree {
        create_worktree(&args.branch, &from_ref)
    } else {
        crate::git::checkout_new(&args.branch, &from_ref)?;
        crate::output::success(&format!("Created and switched to branch '{}'", args.branch));
        Ok(())
    }
}

fn fetch_if_needed(no_fetch: bool) -> Result<(), String> {
    if no_fetch {
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
    let wt_path = crate::git::worktree_path(branch)?;
    let canonical = crate::git::add_worktree(&wt_path, &["-b", branch, from_ref])?;
    crate::output::success(&format!("Created worktree for '{branch}' at {}", canonical.display()));
    crate::output::emit_cd(&canonical);
    Ok(())
}
