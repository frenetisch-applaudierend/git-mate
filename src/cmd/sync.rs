#[derive(clap::Args)]
pub struct SyncArgs {
    #[arg(long)]
    pub rebase: bool,
    #[arg(long)]
    pub ff_only: bool,
}

pub fn run(args: SyncArgs) -> Result<(), String> {
    // 1. git fetch --all --prune
    crate::git::fetch_all()?;

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
    let mut extra_flags = vec![];
    if args.rebase {
        extra_flags.push("--rebase");
    }
    if args.ff_only {
        extra_flags.push("--ff-only");
    }
    crate::git::pull(&extra_flags)?;
    crate::output::success("Synced.");
    Ok(())
}
