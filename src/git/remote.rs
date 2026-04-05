use super::run::{run, run_output};

pub fn list_remote_tracking_refs() -> Result<Vec<String>, String> {
    let output = run_output(&["for-each-ref", "--format=%(refname:short)", "refs/remotes/"])?;
    Ok(output
        .lines()
        .filter(|s| !s.ends_with("/HEAD"))
        .map(|s| s.to_string())
        .collect())
}

pub fn fetch(remote: &str) -> Result<(), String> {
    run(&["fetch", remote])
}

pub fn fetch_all() -> Result<(), String> {
    run(&["fetch", "--all", "--prune"])
}

pub fn pull(extra_args: &[&str]) -> Result<(), String> {
    let mut args = vec!["pull"];
    args.extend_from_slice(extra_args);
    run(&args)
}
