//! Resolve a GitHub pull request URL given to `checkout` to the branch it was
//! opened from, using only git: GitHub publishes every pull request's head
//! commit as `refs/pull/<N>/head` on the base repository, and the branch is
//! the one on the same remote pointing at that commit. Pull requests opened
//! from a fork have no such branch and are rejected.

/// A pull request URL: `https://<host>/<owner>/<repo>/pull/<number>[/...]`.
#[derive(Debug, PartialEq)]
pub struct PullRequestUrl {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

/// The pull request `checkout` was given, reported in `--json` mode.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestData {
    pub number: u64,
    pub url: String,
    pub remote: String,
}

/// The branch a pull request resolved to.
pub struct ResolvedPullRequest {
    pub branch: String,
    /// The remote was fetched because its remote-tracking branch was missing
    /// or behind the pull request.
    pub fetched: bool,
    pub data: PullRequestData,
}

/// Whether `arg` is meant as a URL rather than a branch name. Branch names
/// cannot contain `:`, so this never shadows a branch.
pub fn is_url(arg: &str) -> bool {
    arg.contains("://")
}

pub fn resolve(url: &str) -> Result<ResolvedPullRequest, crate::output::CommandError> {
    let pr = parse_pull_request_url(url).ok_or_else(|| {
        crate::output::CommandError::new(
            format!(
                "{url} is not a pull request URL; expected https://<host>/<owner>/<repo>/pull/<number>"
            ),
            "invalid_pull_request_url",
        )
    })?;
    let error_data = |remote: Option<&str>| serde_json::json!({ "number": pr.number, "url": url, "remote": remote });

    let remotes = crate::git::remote_urls()?;
    let remote = remotes
        .iter()
        .find(|(_, remote_url)| {
            parse_remote_url(remote_url).is_some_and(|(host, path)| {
                host.eq_ignore_ascii_case(&pr.host)
                    && path.eq_ignore_ascii_case(&format!("{}/{}", pr.owner, pr.repo))
            })
        })
        .map(|(name, _)| name.clone())
        .ok_or_else(|| {
            crate::output::CommandError::with_data(
                format!("no remote points to {}/{}/{}", pr.host, pr.owner, pr.repo),
                "remote_not_found",
                error_data(None),
            )
        })?;

    let pull_ref = format!("refs/pull/{}/head", pr.number);
    let refs = crate::git::ls_remote(&remote, &[&pull_ref, "refs/heads/*"])?;
    let head = refs
        .iter()
        .find(|(_, refname)| *refname == pull_ref)
        .map(|(commit, _)| commit.clone())
        .ok_or_else(|| {
            crate::output::CommandError::with_data(
                format!("pull request #{} not found on {remote}", pr.number),
                "pull_request_not_found",
                error_data(Some(&remote)),
            )
        })?;
    let branches: Vec<&str> = refs
        .iter()
        .filter(|(commit, _)| *commit == head)
        .filter_map(|(_, refname)| refname.strip_prefix("refs/heads/"))
        .collect();
    let branch = match branches.as_slice() {
        [branch] => branch.to_string(),
        [] => {
            return Err(crate::output::CommandError::with_data(
                format!(
                    "the branch of pull request #{} is not on {remote}: it was opened from a fork, or its branch has been deleted",
                    pr.number
                ),
                "pull_request_branch_not_found",
                error_data(Some(&remote)),
            ));
        }
        _ => {
            return Err(crate::output::CommandError::with_data(
                format!(
                    "cannot tell which branch pull request #{} is: {} on {remote} all point to its head",
                    pr.number,
                    branches.join(", ")
                ),
                "ambiguous_pull_request_branch",
                serde_json::json!({
                    "number": pr.number,
                    "url": url,
                    "remote": remote,
                    "branches": branches,
                }),
            ));
        }
    };

    crate::output::info(&format!(
        "Pull request #{} is {} on {remote}",
        pr.number,
        console::style(&branch).bold()
    ));
    let tracking = format!("refs/remotes/{remote}/{branch}");
    let fetched = if crate::git::resolve_ref(&tracking).ok().as_deref() == Some(head.as_str()) {
        false
    } else {
        crate::output::info(&format!("Fetching {remote}"));
        crate::git::fetch(&remote)?;
        true
    };

    Ok(ResolvedPullRequest {
        branch,
        fetched,
        data: PullRequestData {
            number: pr.number,
            url: url.to_string(),
            remote,
        },
    })
}

fn parse_pull_request_url(url: &str) -> Option<PullRequestUrl> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let rest = rest.split(['?', '#']).next()?;
    let mut segments = rest.split('/');
    let host = segments.next().filter(|h| !h.is_empty())?;
    let owner = segments.next().filter(|s| !s.is_empty())?;
    let repo = segments.next().filter(|s| !s.is_empty())?;
    if segments.next()? != "pull" {
        return None;
    }
    let number = segments.next()?.parse().ok().filter(|n| *n > 0)?;
    Some(PullRequestUrl {
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        number,
    })
}

/// Split a remote URL into host (without user or port) and repository path
/// (without leading `/`, trailing `/` or `.git`). Understands URLs with a
/// scheme (`https://`, `ssh://`, `git://`, ...) and the scp-like
/// `[user@]host:path` form.
fn parse_remote_url(url: &str) -> Option<(String, String)> {
    let (authority, path) = if let Some((_, rest)) = url.split_once("://") {
        rest.split_once('/')?
    } else {
        // scp-like syntax; a local path has no `:` before its first `/`.
        let (authority, path) = url.split_once(':')?;
        if authority.contains('/') {
            return None;
        }
        (authority, path)
    };
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    let path = path.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some((host.to_string(), path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(host: &str, owner: &str, repo: &str, number: u64) -> Option<PullRequestUrl> {
        Some(PullRequestUrl {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        })
    }

    #[test]
    fn parses_pull_request_urls() {
        let expected = pr("github.com", "acme", "widget", 123);
        for url in [
            "https://github.com/acme/widget/pull/123",
            "https://github.com/acme/widget/pull/123/",
            "https://github.com/acme/widget/pull/123/files",
            "https://github.com/acme/widget/pull/123/commits/abc123",
            "https://github.com/acme/widget/pull/123#issuecomment-1",
            "https://github.com/acme/widget/pull/123?w=1",
            "http://github.com/acme/widget/pull/123",
        ] {
            assert_eq!(parse_pull_request_url(url), expected, "{url}");
        }
        assert_eq!(
            parse_pull_request_url("https://ghe.example.com/team/app/pull/7"),
            pr("ghe.example.com", "team", "app", 7)
        );
    }

    #[test]
    fn rejects_other_urls() {
        for url in [
            "https://github.com/acme/widget",
            "https://github.com/acme/widget/issues/123",
            "https://github.com/acme/widget/pull/",
            "https://github.com/acme/widget/pull/abc",
            "https://github.com/acme/widget/pull/0",
            "https://github.com//widget/pull/1",
            "ssh://github.com/acme/widget/pull/1",
            "feature/login",
        ] {
            assert_eq!(parse_pull_request_url(url), None, "{url}");
        }
    }

    #[test]
    fn parses_remote_urls() {
        let expected = Some(("github.com".to_string(), "acme/widget".to_string()));
        for url in [
            "https://github.com/acme/widget.git",
            "https://github.com/acme/widget",
            "https://github.com/acme/widget/",
            "https://user@github.com/acme/widget.git",
            "git@github.com:acme/widget.git",
            "github.com:acme/widget",
            "ssh://git@github.com/acme/widget.git",
            "ssh://git@github.com:22/acme/widget.git",
            "git://github.com/acme/widget.git",
        ] {
            assert_eq!(parse_remote_url(url), expected, "{url}");
        }
    }

    #[test]
    fn ignores_local_remote_paths() {
        assert_eq!(parse_remote_url("/srv/git/widget.git"), None);
        assert_eq!(parse_remote_url("../widget.git"), None);
        assert_eq!(parse_remote_url("./a:b/widget.git"), None);
    }

    #[test]
    fn recognizes_urls() {
        assert!(is_url("https://github.com/acme/widget/pull/1"));
        assert!(!is_url("feature/login"));
    }
}
