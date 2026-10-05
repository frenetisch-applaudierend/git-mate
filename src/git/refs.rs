use super::run::{run, run_output};

pub fn resolve_ref(refname: &str) -> Result<String, String> {
    run_output(&["rev-parse", refname]).map(|s| s.trim().to_string())
}

pub fn is_ancestor(ancestor: &str, descendant: &str) -> Result<bool, String> {
    // exit 0 = is ancestor, exit 1 = is not ancestor; both are valid outcomes
    let output = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    Ok(output.status.success())
}

pub fn update_ref(refname: &str, new_sha: &str) -> Result<(), String> {
    run(&["update-ref", refname, new_sha])
}

/// Move `refname` to `new_sha`, but only if it still points at `old_sha`,
/// recording `reason` in the reflog.
pub fn update_ref_from(
    refname: &str,
    new_sha: &str,
    old_sha: &str,
    reason: &str,
) -> Result<(), String> {
    run(&["update-ref", "-m", reason, refname, new_sha, old_sha])
}
