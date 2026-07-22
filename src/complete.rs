use std::collections::{HashMap, HashSet};

fn git_lines(args: &[&str]) -> Vec<String> {
    let output = std::process::Command::new("git").args(args).output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    }
}

pub fn list_branches() -> Vec<String> {
    let local = git_lines(&["branch", "--format=%(refname:short)"]);
    let remote = git_lines(&["branch", "-r", "--format=%(refname:short)"]);
    merge_branch_candidates(local, remote)
}

/// Combine local branch names with remote-tracking branches for completion.
///
/// Remote-tracking refs (e.g. `origin/feature`) are offered by their DWIM
/// short name (`feature`) so that `git checkout feature` creates a local
/// tracking branch, matching git's own completion behaviour. A remote branch
/// is skipped when a local branch already shadows it, when it is a `*/HEAD`
/// pointer, or when the same short name exists on more than one remote (in
/// which case the DWIM checkout would be ambiguous).
fn merge_branch_candidates(local: Vec<String>, remote: Vec<String>) -> Vec<String> {
    let local_set: HashSet<&str> = local.iter().map(String::as_str).collect();

    // DWIM short names of every remote-tracking branch, minus `*/HEAD` pointers.
    let dwim: Vec<&str> = remote
        .iter()
        .filter(|r| !r.ends_with("/HEAD"))
        .filter_map(|r| r.split_once('/').map(|(_, name)| name))
        .collect();

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for name in &dwim {
        *counts.entry(*name).or_default() += 1;
    }

    let mut result = local.clone();
    let mut seen: HashSet<&str> = local_set.clone();
    for name in dwim {
        if counts.get(name).copied().unwrap_or(0) != 1 {
            continue; // ambiguous across multiple remotes
        }
        if seen.insert(name) {
            result.push(name.to_string());
        }
    }
    result
}

pub fn branch_completer(
    _current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    list_branches()
        .into_iter()
        .map(clap_complete::engine::CompletionCandidate::new)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::merge_branch_candidates;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn adds_remote_only_branches_by_dwim_name() {
        let out = merge_branch_candidates(v(&["main"]), v(&["origin/main", "origin/feature"]));
        assert_eq!(out, v(&["main", "feature"]));
    }

    #[test]
    fn does_not_duplicate_branches_present_locally_and_remotely() {
        let out = merge_branch_candidates(v(&["main", "feature"]), v(&["origin/feature"]));
        assert_eq!(out, v(&["main", "feature"]));
    }

    #[test]
    fn skips_remote_head_pointer() {
        let out = merge_branch_candidates(v(&["main"]), v(&["origin/HEAD", "origin/main"]));
        assert_eq!(out, v(&["main"]));
    }

    #[test]
    fn skips_names_ambiguous_across_multiple_remotes() {
        let out = merge_branch_candidates(
            v(&["main"]),
            v(&["origin/feature", "upstream/feature", "origin/solo"]),
        );
        assert_eq!(out, v(&["main", "solo"]));
    }

    #[test]
    fn preserves_slashes_in_remote_branch_names() {
        let out = merge_branch_candidates(v(&["main"]), v(&["origin/feature/nested"]));
        assert_eq!(out, v(&["main", "feature/nested"]));
    }
}
