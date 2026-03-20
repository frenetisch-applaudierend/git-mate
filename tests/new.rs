mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn explicit_from() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["new", "feat/x", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn detects_main() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn detects_master() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "-m", "main", "master"]);
    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn prefers_origin_head() {
    // Remote HEAD points to "develop"; local clone has no main/master branch,
    // so detect_default_branch must succeed via the origin/HEAD path.
    let setup = common::RepoWithRemote::with_default_branch("develop");
    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_eq!(setup.local_current_branch(), "feat/x");
}

#[test]
fn no_default_branch() {
    let repo = common::RepoWithoutRemote::new();
    // Detach HEAD then delete main so no fallback branch exists (no remote either).
    let sha = repo.head_commit();
    repo.git(&["checkout", "--detach", &sha]);
    repo.git(&["branch", "-D", "main"]);
    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--from"));
}

#[test]
fn worktree_mode_creates_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);

    let output = common::git_mate()
        .args(["new", "feature/login", "-w", "--from", "main"])
        .env("GIT_MATE_SHELL", "1")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("_MATE_CD:"), "stdout should contain _MATE_CD:, got: {stdout:?}");

    // Derive expected path: <wt_root>/<repo-dir-name>/feature/login
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/login");
    assert!(wt_path.exists(), "worktree directory should exist at {wt_path:?}");

    // The worktree should have feature/login checked out
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let branch_name = String::from_utf8(branch.stdout).unwrap().trim().to_string();
    assert_eq!(branch_name, "feature/login");
}

#[test]
fn fetch_updates_before_branch() {
    let setup = common::RepoWithRemote::new();
    let old_head = setup.local_head_commit();

    // Push a new commit to the remote after the local clone was made.
    setup.push_commit_to_remote("remote update");

    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();

    // The new branch tip should be the remote's latest commit, not the old local HEAD.
    assert_ne!(setup.branch_tip("feat/x"), old_head, "branch should be rooted at remote's new commit");
}

#[test]
fn no_fetch_flag_skips_fetch() {
    let setup = common::RepoWithRemote::new();
    let old_head = setup.local_head_commit();

    setup.push_commit_to_remote("remote update");

    common::git_mate()
        .args(["new", "feat/x", "--no-fetch"])
        .current_dir(setup.local_path())
        .assert()
        .success();

    assert_eq!(setup.branch_tip("feat/x"), old_head, "branch should be rooted at old local HEAD (fetch skipped)");
}

#[test]
fn fetch_config_false_skips_fetch() {
    let setup = common::RepoWithRemote::new();
    let old_head = setup.local_head_commit();

    setup.push_commit_to_remote("remote update");
    setup.local_git(&["config", "mate.fetch", "false"]);

    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();

    assert_eq!(setup.branch_tip("feat/x"), old_head, "branch should be rooted at old local HEAD (config fetch=false)");
}

#[test]
fn no_remote_skips_fetch_silently() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["new", "feat/x", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

fn assert_fetch_skipped_with_config(value: &str) {
    let setup = common::RepoWithRemote::new();
    let old_head = setup.local_head_commit();
    setup.push_commit_to_remote("remote update");
    setup.local_git(&["config", "mate.fetch", value]);
    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_eq!(setup.branch_tip("feat/x"), old_head, "mate.fetch={value} should skip fetch");
}

#[test]
fn fetch_config_no_skips_fetch() {
    assert_fetch_skipped_with_config("no");
}

#[test]
fn fetch_config_off_skips_fetch() {
    assert_fetch_skipped_with_config("off");
}

#[test]
fn fetch_config_zero_skips_fetch() {
    assert_fetch_skipped_with_config("0");
}

#[test]
fn non_origin_remote_skips_fetch_silently() {
    // A repo whose only remote is not named "origin" should not attempt
    // `git fetch origin` — it would fail. The command must succeed.
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["remote", "add", "upstream", "https://example.com/repo.git"]);

    common::git_mate()
        .args(["new", "feat/x", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn worktree_mode_invalid_branch_name_fails() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&["config", "mate.worktreeRoot", wt_root.path().to_str().unwrap()]);

    common::git_mate()
        .args(["new", "../evil", "-w", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid branch name"));
}

#[test]
fn worktree_mode_missing_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["new", "feat/x", "-w", "--from", "main"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.worktreeRoot"));
}

#[test]
fn worktree_mode_copies_ignored_files() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&["config", "mate.worktreeRoot", wt_root.path().to_str().unwrap()]);

    // Commit a .gitignore that ignores .env.local and node_modules/
    std::fs::write(repo.path().join(".gitignore"), ".env.local\nnode_modules/\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "add gitignore"]);

    // Create ignored files in the main worktree
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();
    std::fs::create_dir(repo.path().join("node_modules")).unwrap();
    std::fs::write(repo.path().join("node_modules").join("pkg.json"), "{}").unwrap();

    common::git_mate()
        .args(["new", "feat/x", "-w", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feat/x");

    // Ignored file was copied
    assert!(wt_path.join(".env.local").exists(), ".env.local should be copied");
    assert_eq!(std::fs::read_to_string(wt_path.join(".env.local")).unwrap(), "SECRET=test");

    // Blacklisted directory was NOT copied
    assert!(!wt_path.join("node_modules").exists(), "node_modules should not be copied");
}
