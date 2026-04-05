#![allow(dead_code)]

use std::process::Command;
use tempfile::TempDir;

pub fn git_mate() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("git-mate"))
}

/// Create a temporary protocol file and return a guard (keeps the file alive) and its path.
/// Pass the path as `GIT_MATE_PROTO` to activate shell integration.
pub fn proto_file() -> (tempfile::NamedTempFile, std::path::PathBuf) {
    let f = tempfile::NamedTempFile::new().unwrap();
    let path = f.path().to_path_buf();
    (f, path)
}

pub struct RepoWithoutRemote {
    pub dir: TempDir,
}

impl RepoWithoutRemote {
    /// Init a repo with one empty commit on `main`.
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        // Use symbolic-ref instead of `init -b` for broader git compatibility.
        git_silent(p, &["init"]);
        git_silent(p, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git_silent(p, &["config", "user.email", "test@test.com"]);
        git_silent(p, &["config", "user.name", "Test"]);
        git_silent(p, &["commit", "--allow-empty", "-m", "init"]);
        Self { dir }
    }

    pub fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    pub fn git(&self, args: &[&str]) {
        git(self.path(), args);
    }

    pub fn current_branch(&self) -> String {
        rev_parse_output(self.path(), &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    pub fn head_commit(&self) -> String {
        rev_parse_output(self.path(), &["rev-parse", "HEAD"])
    }

    pub fn branch_exists(&self, branch: &str) -> bool {
        Command::new("git")
            .args(["rev-parse", "--verify", branch])
            .current_dir(self.path())
            .output()
            .unwrap()
            .status
            .success()
    }
}

/// A bare remote repo paired with a local clone — no network needed.
pub struct RepoWithRemote {
    bare: TempDir,
    pub local: TempDir,
}

impl RepoWithRemote {
    pub fn new() -> Self {
        Self::with_default_branch("main")
    }

    /// Create a bare remote whose HEAD points to `branch`, then clone it locally.
    pub fn with_default_branch(branch: &str) -> Self {
        let bare = TempDir::new().unwrap();

        // Init bare and point HEAD at the desired branch.
        git_silent(bare.path(), &["init", "--bare"]);
        git_silent(
            bare.path(),
            &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
        );

        // Scratch clone: init, set HEAD, commit, push to bare.
        let scratch = TempDir::new().unwrap();
        let bare_url = bare.path().to_str().unwrap().to_string();
        git_silent(scratch.path(), &["init"]);
        git_silent(
            scratch.path(),
            &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
        );
        git_silent(scratch.path(), &["config", "user.email", "test@test.com"]);
        git_silent(scratch.path(), &["config", "user.name", "Test"]);
        git_silent(scratch.path(), &["remote", "add", "origin", &bare_url]);
        git_silent(scratch.path(), &["commit", "--allow-empty", "-m", "init"]);
        git_silent(scratch.path(), &["push", "-u", "origin", branch]);

        // Clone bare into local.
        let local = TempDir::new().unwrap();
        git_silent(local.path(), &["clone", &bare_url, "."]);
        git_silent(local.path(), &["config", "user.email", "test@test.com"]);
        git_silent(local.path(), &["config", "user.name", "Test"]);

        RepoWithRemote { bare, local }
    }

    pub fn local_path(&self) -> &std::path::Path {
        self.local.path()
    }

    pub fn bare_path(&self) -> &std::path::Path {
        self.bare.path()
    }

    pub fn local_current_branch(&self) -> String {
        rev_parse_output(self.local_path(), &["rev-parse", "--abbrev-ref", "HEAD"])
    }

    pub fn local_head_commit(&self) -> String {
        rev_parse_output(self.local_path(), &["rev-parse", "HEAD"])
    }

    pub fn branch_tip(&self, branch: &str) -> String {
        rev_parse_output(self.local_path(), &["rev-parse", branch])
    }

    pub fn local_git(&self, args: &[&str]) {
        git(self.local_path(), args);
    }

    pub fn remote_tracking_exists(&self, tracking_ref: &str) -> bool {
        Command::new("git")
            .args(["rev-parse", "--verify", tracking_ref])
            .current_dir(self.local_path())
            .output()
            .unwrap()
            .status
            .success()
    }

    /// Push a new empty commit onto the remote's current branch.
    pub fn push_commit_to_remote(&self, message: &str) {
        let scratch = TempDir::new().unwrap();
        let bare_url = self.bare_path().to_str().unwrap().to_string();
        git_silent(scratch.path(), &["clone", &bare_url, "."]);
        git_silent(scratch.path(), &["config", "user.email", "test@test.com"]);
        git_silent(scratch.path(), &["config", "user.name", "Test"]);
        git_silent(scratch.path(), &["commit", "--allow-empty", "-m", message]);
        git_silent(scratch.path(), &["push"]);
    }

    /// Push a new empty commit onto a specific remote branch.
    pub fn push_commit_to_remote_branch(&self, branch: &str, message: &str) {
        let scratch = TempDir::new().unwrap();
        let bare_url = self.bare_path().to_str().unwrap().to_string();
        git_silent(scratch.path(), &["clone", &bare_url, "."]);
        git_silent(scratch.path(), &["config", "user.email", "test@test.com"]);
        git_silent(scratch.path(), &["config", "user.name", "Test"]);
        git_silent(scratch.path(), &["checkout", branch]);
        git_silent(
            scratch.path(),
            &["commit", "--allow-empty", "-m", message],
        );
        git_silent(scratch.path(), &["push"]);
    }

    /// Create a new branch on the remote via a scratch clone.
    pub fn push_branch_to_remote(&self, branch: &str) {
        let scratch = TempDir::new().unwrap();
        let bare_url = self.bare_path().to_str().unwrap().to_string();
        git_silent(scratch.path(), &["clone", &bare_url, "."]);
        git_silent(scratch.path(), &["config", "user.email", "test@test.com"]);
        git_silent(scratch.path(), &["config", "user.name", "Test"]);
        git_silent(scratch.path(), &["checkout", "-b", branch]);
        git_silent(
            scratch.path(),
            &["commit", "--allow-empty", "-m", &format!("add {branch}")],
        );
        git_silent(scratch.path(), &["push", "origin", branch]);
    }

    /// Delete a branch directly from the bare repo.
    pub fn delete_remote_branch(&self, branch: &str) {
        git_silent(self.bare_path(), &["branch", "-D", branch]);
    }

    /// Fetch from remote so the tracking ref exists locally.
    pub fn local_fetch(&self) {
        git_silent(self.local_path(), &["fetch", "--prune"]);
    }

    /// Create a local branch tracking origin/<branch> (must already be fetched).
    /// Leaves the repo on its original branch.
    pub fn create_local_tracking_branch(&self, branch: &str) {
        let current = self.local_current_branch();
        git_silent(
            self.local_path(),
            &["checkout", "-b", branch, &format!("origin/{branch}")],
        );
        git_silent(self.local_path(), &["checkout", &current]);
    }

    pub fn local_branch_exists(&self, branch: &str) -> bool {
        Command::new("git")
            .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
            .current_dir(self.local_path())
            .output()
            .unwrap()
            .status
            .success()
    }

    /// Make a local commit on <branch> that has NOT been pushed.
    pub fn make_local_commit_on(&self, branch: &str, message: &str) {
        let current = self.local_current_branch();
        git_silent(self.local_path(), &["checkout", branch]);
        git_silent(
            self.local_path(),
            &["commit", "--allow-empty", "-m", message],
        );
        git_silent(self.local_path(), &["checkout", &current]);
    }
}

pub fn git(dir: &std::path::Path, args: &[&str]) {
    let s = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(s.success(), "git {:?} failed in {:?}", args, dir);
}

fn git_silent(dir: &std::path::Path, args: &[&str]) {
    let s = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(s.success(), "git {:?} failed in {:?}", args, dir);
}

fn rev_parse_output(dir: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}
