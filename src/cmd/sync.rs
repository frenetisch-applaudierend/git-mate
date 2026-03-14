#[derive(clap::Args)]
pub struct SyncArgs {
    #[arg(long)]
    pub rebase: bool,
    #[arg(long)]
    pub ff_only: bool,
}

pub fn run(args: SyncArgs) -> Result<(), String> {
    // 1. git fetch --all --prune
    crate::git::run(&["fetch", "--all", "--prune"])?;

    // 2. Check for upstream
    let has_upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_upstream {
        crate::output::info("No upstream configured for current branch, skipping pull.");
        return Ok(());
    }

    // 3. git pull [--rebase | --ff-only]
    let mut pull_args = vec!["pull"];
    if args.rebase {
        pull_args.push("--rebase");
    }
    if args.ff_only {
        pull_args.push("--ff-only");
    }
    crate::git::run(&pull_args)?;
    crate::output::success("Synced.");
    Ok(())
}
