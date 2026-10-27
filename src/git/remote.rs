use super::run::{run, run_output};

pub fn list_remote_tracking_refs() -> Result<Vec<String>, String> {
    let output = run_output(&["for-each-ref", "--format=%(refname:short)", "refs/remotes/"])?;
    Ok(output
        .lines()
        .filter(|s| !s.ends_with("/HEAD"))
        .map(|s| s.to_string())
        .collect())
}

pub fn list_remotes() -> Result<Vec<String>, String> {
    Ok(run_output(&["remote"])?
        .lines()
        .map(|s| s.to_string())
        .collect())
}

pub fn fetch(remote: &str) -> Result<(), String> {
    run(&["fetch", remote])
}

pub fn fetch_all() -> Result<(), String> {
    run(&["fetch", "--all", "--prune"])
}
