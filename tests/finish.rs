mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn finish_feature_branch_switches_to_main() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "main");
    assert!(!repo.branch_exists("feature/x"), "feature/x should have been deleted");
}

#[test]
fn finish_on_default_branch_fails() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("default branch"));
}

#[test]
fn finish_deletes_branch_after_switching() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    // feature/x is at the same commit as main, so safe delete succeeds
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "main");
    assert!(!repo.branch_exists("feature/x"), "feature/x should have been deleted");
}

#[test]
fn finish_unmerged_branch_without_remote_succeeds() {
    // No remote → no "unpushed" concept; finish always succeeds
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/x"]);
    repo.git(&["commit", "--allow-empty", "-m", "unmerged work"]);
    common::git_mate()
        .args(["finish"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(!repo.branch_exists("feature/x"), "feature/x should have been deleted");
}

#[test]
fn finish_unpushed_commits_requires_force() {
    let repo = common::RepoWithRemote::new();
    repo.local_git(&["checkout", "-b", "feature/x"]);
    repo.local_git(&["commit", "--allow-empty", "-m", "local only"]);

    // Without --force: should fail with a clear message
    common::git_mate()
        .args(["finish", "feature/x"])
        .current_dir(repo.local_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unpushed commits"));

    // With --force: should succeed
    common::git_mate()
        .args(["finish", "--force", "feature/x"])
        .current_dir(repo.local_path())
        .assert()
        .success();
    assert!(!repo.local_branch_exists("feature/x"), "feature/x should have been deleted");
}

#[test]
fn finish_pushed_branch_succeeds_without_force() {
    // Branch created from origin/main (same commit) — no unpushed commits
    let repo = common::RepoWithRemote::new();
    repo.local_git(&["checkout", "-b", "feature/x", "origin/main"]);

    common::git_mate()
        .args(["finish", "feature/x"])
        .current_dir(repo.local_path())
        .assert()
        .success();
    assert!(!repo.local_branch_exists("feature/x"), "feature/x should have been deleted");
}

#[test]
fn finish_force_removes_dirty_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-dirty-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/dirty", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();
    // Create an untracked file in the worktree to make it dirty
    std::fs::write(wt_canonical.join("untracked.txt"), "dirty").unwrap();

    // Without --force it should fail
    common::git_mate()
        .args(["finish", "feature/dirty"])
        .current_dir(repo.path())
        .assert()
        .failure();

    // With --force it should succeed
    common::git_mate()
        .args(["finish", "--force", "feature/dirty"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(!wt_canonical.exists(), "worktree directory should be removed");
    assert!(!repo.branch_exists("feature/dirty"), "branch should have been deleted");
}

#[test]
fn finish_linked_worktree_removes_it_and_prints_mate_cd() {
    let repo = common::RepoWithoutRemote::new();
    // Add a linked worktree for feature/x
    let wt_path = repo.path().join("feature-x-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/x", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["finish"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&wt_canonical)
        .output()
        .unwrap();
    assert!(output.status.success(), "finish should succeed");
    let proto = std::fs::read_to_string(&proto_path).unwrap();
    let main_path = std::fs::canonicalize(repo.path()).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", main_path.display())),
        "protocol file should contain CD:<main_path>, got: {proto:?}"
    );
    assert!(!wt_canonical.exists(), "worktree directory should be removed");
    assert!(!repo.branch_exists("feature/x"), "branch should have been deleted");
}

#[test]
fn finish_branch_name_from_main_worktree_removes_linked_wt() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-y-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/y", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    common::git_mate()
        .args(["finish", "feature/y"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(!wt_canonical.exists(), "worktree directory should be removed");
    assert!(!repo.branch_exists("feature/y"), "branch should have been deleted");
}

#[test]
fn finish_linked_worktree_removes_wt_and_deletes_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_path = repo.path().join("feature-del-wt");
    let wt_path_str = wt_path.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "feature/del", wt_path_str, "main"]);
    let wt_canonical = std::fs::canonicalize(&wt_path).unwrap();

    let output = common::git_mate()
        .args(["finish"])
        .current_dir(&wt_canonical)
        .output()
        .unwrap();
    assert!(output.status.success(), "finish should succeed");
    assert!(!wt_canonical.exists(), "worktree directory should be removed");
    assert!(!repo.branch_exists("feature/del"), "feature/del should have been deleted");
}

#[test]
fn finish_branch_not_checked_out_deletes_it() {
    let repo = common::RepoWithoutRemote::new();
    // Create a branch (merged, same commit as main) and stay on main
    repo.git(&["branch", "feature/z"]);
    common::git_mate()
        .args(["finish", "feature/z"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert!(!repo.branch_exists("feature/z"), "feature/z should have been deleted");
}

#[test]
fn finish_removes_empty_parent_dirs_from_slash_branch() {
    use tempfile::TempDir;

    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);

    let project_name = repo.path().file_name().unwrap().to_str().unwrap();
    let feature_dir = wt_root.path().join(project_name).join("feature");
    let wt_path = feature_dir.join("login");
    std::fs::create_dir_all(&feature_dir).unwrap();

    repo.git(&["worktree", "add", "-b", "feature/login", wt_path.to_str().unwrap(), "main"]);

    common::git_mate()
        .args(["finish", "feature/login"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert!(!wt_path.exists(), "worktree dir should be removed");
    assert!(!feature_dir.exists(), "empty 'feature' dir should be cleaned up");
    assert!(
        !wt_root.path().join(project_name).exists(),
        "empty project container dir should be cleaned up"
    );
    assert!(wt_root.path().exists(), "worktree root itself must not be deleted");
}
