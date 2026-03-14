pub fn list_branches() -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    }
}

pub fn branch_completer(
    _current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    list_branches()
        .into_iter()
        .map(clap_complete::engine::CompletionCandidate::new)
        .collect()
}
