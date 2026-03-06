#[derive(clap::Args)]
pub struct SyncArgs {
    #[arg(long)]
    pub rebase: bool,
    #[arg(long)]
    pub ff_only: bool,
}

pub fn run(args: SyncArgs) -> Result<(), String> {
    // 1. git fetch --all --prune
    run_git(&["fetch", "--all", "--prune"])?;

    // 2. Check for upstream
    let has_upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_upstream {
        println!("No upstream configured for current branch, skipping pull.");
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
    run_git(&pull_args)
}

fn run_git(args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new("git")
        .args(args)
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`git {}` failed", args.join(" ")))
    }
}
