#[derive(clap::Args)]
pub struct NewArgs {
    pub branch: String,
    #[arg(long, add = clap_complete::engine::ArgValueCompleter::new(crate::complete::branch_completer))]
    pub from: Option<String>,
    #[arg(short = 'w', long)]
    pub worktree: bool,
    #[arg(long)]
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
        crate::git::run(&["checkout", "-b", &args.branch, &from_ref])
    }
}

fn fetch_if_needed(no_fetch: bool) -> Result<(), String> {
    if no_fetch {
        return Ok(());
    }
    if crate::git::config::read_bool("mate.fetch") == Some(false) {
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
    crate::git::run(&["fetch", "origin"])
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
    if let Some(parent) = wt_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directories {}: {e}", parent.display()))?;
    }
    let wt_path_str = wt_path
        .to_str()
        .ok_or("worktree path is not valid UTF-8")?;
    crate::git::run(&["worktree", "add", wt_path_str, "-b", branch, from_ref])?;
    let canonical = std::fs::canonicalize(&wt_path)
        .unwrap_or_else(|_| wt_path.clone());
    crate::output::emit_cd(&canonical);
    Ok(())
}
