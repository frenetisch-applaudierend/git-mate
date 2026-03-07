#[derive(clap::Args)]
pub struct CheckoutArgs {
    pub branch: String,
    #[arg(short = 'w', long)]
    pub worktree: bool,
}

pub fn run(args: CheckoutArgs) -> Result<(), String> {
    if args.worktree {
        checkout_worktree(&args.branch)
    } else {
        checkout_in_place(&args.branch)
    }
}

fn checkout_in_place(branch: &str) -> Result<(), String> {
    if !crate::git::is_main_worktree()? {
        return Err(
            "checkout is only supported from the main worktree; use --worktree to open this branch in a linked worktree"
                .to_string(),
        );
    }
    crate::git::run(&["checkout", branch])
}

fn checkout_worktree(branch: &str) -> Result<(), String> {
    let main_wt = crate::git::find_main_worktree()?;
    let root = crate::git::read_worktree_root()?;
    let project_name = main_wt
        .file_name()
        .ok_or("main worktree path has no directory name")?
        .to_str()
        .ok_or("main worktree directory name is not valid UTF-8")?;
    let wt_path = root.join(project_name).join(branch);

    if wt_path.exists() && wt_path.join(".git").exists() {
        println!("worktree already exists at {}", wt_path.display());
        return Ok(());
    }

    if let Some(parent) = wt_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create directories {}: {e}", parent.display()))?;
    }

    let wt_path_str = wt_path.to_str().ok_or("worktree path is not valid UTF-8")?;
    crate::git::run(&["worktree", "add", wt_path_str, branch])
}
