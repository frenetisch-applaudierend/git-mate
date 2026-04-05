#[derive(clap::Args)]
pub struct SyncArgs {
    #[arg(long, help = "Pull with --rebase")]
    pub rebase: bool,
    #[arg(long, help = "Pull with --ff-only")]
    pub ff_only: bool,
}

pub fn run(args: SyncArgs) -> Result<(), String> {
    // 1. Snapshot every local branch's tip and its upstream tip before fetching,
    //    so we can tell whether a branch had unique commits even after the upstream
    //    is pruned away.
    let branches_before = snapshot_branch_upstreams();

    // 2. Fetch everything and prune stale remote-tracking refs.
    let tracking_before: std::collections::HashSet<String> =
        crate::git::list_remote_tracking_refs()?.into_iter().collect();

    crate::git::fetch_all()?;

    let tracking_after: std::collections::HashSet<String> =
        crate::git::list_remote_tracking_refs()?.into_iter().collect();

    let pruned: std::collections::HashSet<&String> =
        tracking_before.difference(&tracking_after).collect();

    // 3. Gather context we need throughout.
    let current_branch = crate::git::current_branch().ok();
    let worktrees = crate::git::list_worktrees()?;
    let main_wt = worktrees.first().ok_or("no worktrees found")?;
    let main_wt_path = main_wt
        .path
        .to_str()
        .ok_or("main worktree path is not valid UTF-8")?
        .to_string();
    let current_wt = {
        let cwd = std::env::current_dir()
            .map_err(|e| format!("could not determine current directory: {e}"))?;
        worktrees
            .iter()
            .filter(|wt| cwd.starts_with(&wt.path))
            .max_by_key(|wt| wt.path.components().count())
            .map(|wt| wt.path.clone())
    };

    // 4. Process all local branches that have an upstream.
    let branches = crate::git::list_local_branches_with_upstream()?;

    let mut current_branch_pruned = false;

    for (branch, upstream) in &branches {
        let Some(upstream) = upstream else { continue };

        let is_current = current_branch.as_deref() == Some(branch.as_str());

        if pruned.contains(upstream) {
            let had_unique = branches_before
                .iter()
                .find(|(b, _, _)| b == branch)
                .map(|(_, local_sha, upstream_sha)| {
                    // The branch had unique commits if its tip was NOT an ancestor of
                    // (i.e., was ahead of) the upstream tip at snapshot time.
                    match (local_sha, upstream_sha) {
                        (Some(l), Some(u)) => {
                            !crate::git::is_ancestor(l, u).unwrap_or(false)
                        }
                        // No upstream SHA recorded means we couldn't check → be safe.
                        _ => true,
                    }
                })
                .unwrap_or(true);

            handle_pruned_branch(
                branch,
                had_unique,
                is_current,
                &current_wt,
                &worktrees,
                &main_wt_path,
            )?;
            if is_current {
                current_branch_pruned = true;
            }
        } else if !is_current {
            // Remote still exists — try to fast-forward.
            fast_forward_branch(branch, upstream)?;
        }
        // Current branch with live upstream is handled by pull below.
    }

    // 5. Pull the current branch (if it still has an upstream).
    if current_branch_pruned {
        return Ok(());
    }

    let has_upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_upstream {
        crate::output::info("No upstream configured for current branch, skipping pull.");
        return Ok(());
    }

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

/// Returns (branch, local_sha, upstream_sha) for every local branch that has an upstream.
/// Called before fetching, so upstream SHAs reflect the state on the remote right now.
fn snapshot_branch_upstreams() -> Vec<(String, Option<String>, Option<String>)> {
    let Ok(branches) = crate::git::list_local_branches_with_upstream() else {
        return vec![];
    };
    branches
        .into_iter()
        .map(|(branch, upstream)| {
            let local_sha = crate::git::resolve_ref(&branch).ok();
            let upstream_sha = upstream
                .as_deref()
                .and_then(|u| crate::git::resolve_ref(u).ok());
            (branch, local_sha, upstream_sha)
        })
        .collect()
}

fn fast_forward_branch(branch: &str, upstream: &str) -> Result<(), String> {
    let local_sha = match crate::git::resolve_ref(branch) {
        Ok(sha) => sha,
        Err(_) => return Ok(()),
    };
    let remote_sha = match crate::git::resolve_ref(upstream) {
        Ok(sha) => sha,
        Err(_) => return Ok(()),
    };
    if local_sha == remote_sha {
        return Ok(());
    }
    match crate::git::is_ancestor(&local_sha, &remote_sha)? {
        true => {
            crate::git::update_ref(&format!("refs/heads/{branch}"), &remote_sha)?;
            crate::output::info(&format!("{branch}: fast-forwarded"));
        }
        false => {
            crate::output::info(&format!("{branch}: cannot fast-forward (diverged), skipping"));
        }
    }
    Ok(())
}

fn handle_pruned_branch(
    branch: &str,
    had_unique_commits: bool,
    is_current: bool,
    current_wt: &Option<std::path::PathBuf>,
    worktrees: &[crate::git::WorktreeEntry],
    main_wt_path: &str,
) -> Result<(), String> {
    if had_unique_commits {
        crate::output::info(&format!(
            "{branch}: remote deleted but has unpushed commits, skipping"
        ));
        return Ok(());
    }

    // Find which worktree (if any) has this branch checked out.
    let checked_out_wt = worktrees
        .iter()
        .find(|wt| wt.branch.as_deref() == Some(branch))
        .map(|wt| &wt.path);

    // If checked out, the working tree must be clean.
    if let Some(wt_path) = checked_out_wt {
        let wt_str = wt_path.to_str().ok_or("worktree path is not valid UTF-8")?;
        if !crate::git::is_worktree_clean(wt_str)? {
            crate::output::info(&format!(
                "{branch}: remote deleted but working tree is dirty, skipping"
            ));
            return Ok(());
        }
    }

    // For the current branch, ask before acting.
    if is_current {
        if !prompt_yes_no(&format!(
            "Remote for '{branch}' was deleted. Delete local branch?"
        )) {
            crate::output::info(&format!("{branch}: kept"));
            return Ok(());
        }
    }

    // Perform the finish-like action for checked-out branches.
    if let Some(wt_path) = checked_out_wt {
        let main_wt = std::path::Path::new(main_wt_path);
        if wt_path == main_wt {
            // Branch is in the main worktree — switch to default branch first.
            let default = crate::git::detect_default_branch(false)?;
            crate::git::checkout_in(main_wt_path, &default)?;
        } else {
            // Branch is in a linked worktree — remove it.
            crate::git::remove_worktree(wt_path, false)?;
            if let Ok(root) = crate::git::read_worktree_root() {
                crate::fs::remove_empty_parent_dirs(wt_path, &root);
            }
            // If the user was inside that worktree, navigate them to main.
            if current_wt.as_deref() == Some(wt_path.as_path()) {
                let canonical = std::fs::canonicalize(main_wt)
                    .map_err(|e| format!("could not canonicalize path: {e}"))?;
                crate::shell_protocol::emit_cd(&canonical);
            }
        }
    }

    // Delete the local branch ref.
    crate::git::delete_branch_force_in(main_wt_path, branch)?;
    crate::output::info(&format!("{branch}: deleted (remote was deleted)"));
    Ok(())
}

fn prompt_yes_no(question: &str) -> bool {
    use std::io::Write as _;
    eprint!("{} [y/N] ", question);
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}
