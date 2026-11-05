use super::run::run;

/// Where a branch is pushed to.
pub struct PushTarget {
    pub remote: String,
    /// The branch name on the remote.
    pub remote_branch: String,
    /// The branch has no upstream yet; pushing sets it.
    pub set_upstream: bool,
}

impl PushTarget {
    /// The remote-tracking ref that mirrors the remote branch.
    pub fn tracking_ref(&self) -> String {
        format!("refs/remotes/{}/{}", self.remote, self.remote_branch)
    }
}

/// The branch's configured upstream, or `origin/<branch>` (to be set as the
/// upstream) if it has none. `None` without a remote to push to.
pub fn push_target(branch: &str) -> Option<PushTarget> {
    let remote = super::config::read_string(&format!("branch.{branch}.remote"));
    let merge = super::config::read_string(&format!("branch.{branch}.merge"));
    if let (Some(remote), Some(merge)) = (remote, merge)
        && remote != "."
        && let Some(remote_branch) = merge.strip_prefix("refs/heads/")
    {
        return Some(PushTarget {
            remote,
            remote_branch: remote_branch.to_string(),
            set_upstream: false,
        });
    }
    let has_origin = super::remote::list_remotes()
        .ok()?
        .iter()
        .any(|r| r == "origin");
    has_origin.then(|| PushTarget {
        remote: "origin".to_string(),
        remote_branch: branch.to_string(),
        set_upstream: true,
    })
}

/// Push `branch` to `target`. With `lease`, force the push, but only if the
/// remote branch is still at the given commit (`None`: doesn't exist).
pub fn push_branch(
    branch: &str,
    target: &PushTarget,
    lease: Option<Option<&str>>,
) -> Result<(), String> {
    let remote_ref = format!("refs/heads/{}", target.remote_branch);
    let mut args = vec!["push".to_string()];
    if target.set_upstream {
        args.push("--set-upstream".to_string());
    }
    if let Some(expected) = lease {
        args.push(format!(
            "--force-with-lease={remote_ref}:{}",
            expected.unwrap_or_default()
        ));
    }
    args.push(target.remote.clone());
    args.push(format!("refs/heads/{branch}:{remote_ref}"));
    run(&args.iter().map(String::as_str).collect::<Vec<_>>())
}
