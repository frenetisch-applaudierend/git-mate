//! Pushing branches after `git mate update`.

use super::update_report::UpdateOutcome;

/// When to push, and whether that may rewrite the remote branch.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushMode {
    /// Only fast-forward pushes (or creating the remote branch).
    Ff,
    /// Like `ff`, but ask before each force-push.
    Ask,
    /// Force-push when needed, with a lease on what the last fetch saw.
    Force,
}

/// Push the branch of an updated (or already up-to-date) `outcome` and
/// record how that went. Skipped or failed branches are not pushed.
pub fn push(outcome: &mut UpdateOutcome, mode: PushMode, dry_run: bool) {
    if !matches!(outcome.action, "rebased" | "merged" | "up-to-date") {
        return;
    }
    match try_push(outcome, mode, dry_run) {
        Ok(result) => outcome.push = Some(result),
        Err(e) => {
            outcome.push = Some("failed");
            outcome.push_reason = Some(e);
        }
    }
}

fn try_push(
    outcome: &UpdateOutcome,
    mode: PushMode,
    dry_run: bool,
) -> Result<&'static str, String> {
    let branch = outcome.branch.as_str();
    let Some(target) = crate::git::push_target(branch) else {
        return Ok("no-remote");
    };
    let tip = crate::git::resolve_ref(&format!("refs/heads/{branch}"))?;
    let remote = crate::git::resolve_ref(&target.tracking_ref()).ok();

    // Under --dry-run the branch hasn't moved: judge by where it would be.
    let moves = dry_run && outcome.action != "up-to-date";
    if !moves && remote.as_deref() == Some(tip.as_str()) {
        return Ok("up-to-date");
    }
    let fast_forward = match &remote {
        None => true,
        Some(remote) if !moves => crate::git::is_ancestor(remote, &tip)?,
        Some(remote) => {
            let base = outcome.base.as_deref().unwrap_or("HEAD");
            let base_tip = crate::git::resolve_ref(&format!("{base}^{{commit}}"))?;
            // A rebase rewrites the branch's own commits: only what the base
            // already has survives. A merge keeps them.
            crate::git::is_ancestor(remote, &base_tip)?
                || (outcome.action == "merged" && crate::git::is_ancestor(remote, &tip)?)
        }
    };

    if fast_forward {
        if !dry_run {
            crate::git::push_branch(branch, &target, None)?;
        }
        return Ok("pushed");
    }

    let force = match mode {
        PushMode::Ff => false,
        PushMode::Force => true,
        PushMode::Ask if dry_run => false,
        PushMode::Ask => {
            let question = format!(
                "Force-push {branch}? This rewrites {}/{}.",
                target.remote, target.remote_branch
            );
            match super::confirm::confirm(&question, false) {
                Some(true) => true,
                Some(false) => return Ok("declined"),
                None => false,
            }
        }
    };
    if !force {
        return Ok("needs-force-push");
    }
    if !dry_run {
        crate::git::push_branch(branch, &target, Some(remote.as_deref()))?;
    }
    Ok("forced")
}
