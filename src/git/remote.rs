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

/// The configured URL of every remote as `(name, url)`, exactly as written in
/// the config (before any `url.<base>.insteadOf` rewriting).
pub fn remote_urls() -> Result<Vec<(String, String)>, String> {
    // `--get-regexp` exits with 1 when nothing matches, i.e. no remotes.
    let output = run_output(&["config", "--get-regexp", r"^remote\..*\.url$"]).unwrap_or_default();
    Ok(output
        .lines()
        .filter_map(|line| {
            let (key, url) = line.split_once(' ')?;
            let name = key.strip_prefix("remote.")?.strip_suffix(".url")?;
            Some((name.to_string(), url.trim().to_string()))
        })
        .collect())
}

/// The refs on `remote` matching `patterns` as `(commit, refname)`.
pub fn ls_remote(remote: &str, patterns: &[&str]) -> Result<Vec<(String, String)>, String> {
    let mut args = vec!["ls-remote", remote];
    args.extend_from_slice(patterns);
    Ok(run_output(&args)?
        .lines()
        .filter_map(|line| {
            let (commit, refname) = line.split_once('\t')?;
            Some((commit.to_string(), refname.to_string()))
        })
        .collect())
}
