use super::run::run;

/// Where a branch is pushed to.
pub struct PushTarget {
    pub remote: String,
    /// The branch name on the remote.
    pub remote_branch: String,
    /// The branch has no upstream yet; pushing sets it.
    pub set_upstream: bool,
    /// The remote branch is the branch's configured upstream, so if its
    /// remote-tracking ref is missing, the remote branch was deleted (and
    /// pruned), not never pushed.
    pub is_upstream: bool,
}

impl PushTarget {
    /// The remote-tracking ref that mirrors the remote branch.
    pub fn tracking_ref(&self) -> String {
        format!("refs/remotes/{}/{}", self.remote, self.remote_branch)
    }
}

/// Where `branch` is pushed, as `git push` with the default
/// `push.default=simple` would push it: to the same-named branch on its push
/// remote (`branch.<name>.pushRemote`, else `remote.pushDefault`) if that
/// differs from its upstream's remote; else to its upstream; else to
/// `origin/<branch>`, to be set as the upstream. `Ok(None)` without a remote
/// to push to.
///
/// Like `simple`, this refuses an upstream with a different name: a branch
/// created from `origin/main` tracks `main`, and pushing it there would
/// publish its commits onto `main`.
pub fn push_target(branch: &str) -> Result<Option<PushTarget>, String> {
    let branch_config = |key: &str| super::config::read_string(&format!("branch.{branch}.{key}"));
    let upstream = match (branch_config("remote"), branch_config("merge")) {
        (Some(remote), Some(merge)) if remote != "." => merge
            .strip_prefix("refs/heads/")
            .map(|name| (remote, name.to_string())),
        _ => None,
    };
    let push_remote =
        branch_config("pushRemote").or_else(|| super::config::read_string("remote.pushDefault"));
    match (push_remote, upstream) {
        (Some(push_remote), upstream)
            if upstream
                .as_ref()
                .is_none_or(|(remote, _)| *remote != push_remote) =>
        {
            Ok(Some(PushTarget {
                remote: push_remote,
                remote_branch: branch.to_string(),
                set_upstream: false,
                is_upstream: false,
            }))
        }
        (_, Some((remote, remote_branch))) => {
            if remote_branch != branch {
                return Err(format!(
                    "its upstream {remote}/{remote_branch} has a different name; \
                     push it yourself with git push {remote} {branch}"
                ));
            }
            Ok(Some(PushTarget {
                remote,
                remote_branch,
                set_upstream: false,
                is_upstream: true,
            }))
        }
        _ => {
            let has_origin = super::remote::list_remotes()?.iter().any(|r| r == "origin");
            Ok(has_origin.then(|| PushTarget {
                remote: "origin".to_string(),
                remote_branch: branch.to_string(),
                set_upstream: true,
                is_upstream: false,
            }))
        }
    }
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
