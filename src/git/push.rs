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

/// Where `branch` is pushed: the same-named branch on its upstream's remote,
/// or `origin/<branch>` (to be set as the upstream) if it has no upstream.
/// `Ok(None)` without a remote to push to.
///
/// Like git's `push.default=simple`, this refuses an upstream with a
/// different name: a branch created from `origin/main` tracks `main`, and
/// pushing it there would publish its commits onto `main`.
pub fn push_target(branch: &str) -> Result<Option<PushTarget>, String> {
    let remote = super::config::read_string(&format!("branch.{branch}.remote"));
    let merge = super::config::read_string(&format!("branch.{branch}.merge"));
    if let (Some(remote), Some(merge)) = (remote, merge)
        && remote != "."
        && let Some(remote_branch) = merge.strip_prefix("refs/heads/")
    {
        if remote_branch != branch {
            return Err(format!(
                "its upstream {remote}/{remote_branch} has a different name; \
                 push it yourself with git push {remote} {branch}"
            ));
        }
        return Ok(Some(PushTarget {
            remote,
            remote_branch: remote_branch.to_string(),
            set_upstream: false,
        }));
    }
    let has_origin = super::remote::list_remotes()?.iter().any(|r| r == "origin");
    Ok(has_origin.then(|| PushTarget {
        remote: "origin".to_string(),
        remote_branch: branch.to_string(),
        set_upstream: true,
    }))
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
