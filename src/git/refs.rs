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

pub fn stash_push(message: &str) -> Result<String, String> {
    run(&["stash", "push", "-u", "-m", message])?;
    let stash_ref = run_output(&["stash", "list", "--format=%gd", "-1"])
        .map(|s| s.trim().to_string())
        .map_err(|_| "created stash but could not determine stash ref".to_string())?;
    if stash_ref.is_empty() {
        return Err("created stash but could not determine stash ref".to_string());
    }
    Ok(stash_ref)
}

pub fn stash_pop_in(path: &str) -> Result<(), String> {
    run(&["-C", path, "stash", "pop"])
}
