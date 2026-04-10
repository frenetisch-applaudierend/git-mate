mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn move_current_branch_from_main_to_linked_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    repo.git(&["checkout", "-b", "feature/move"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .arg("move")
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "move should succeed");

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/move");
    assert!(
        proto.contains(&format!("CD:{}", wt_path.display())),
        "protocol file should contain CD: pointing to the new worktree, got: {proto:?}"
    );

    assert_eq!(repo.current_branch(), "main");
    assert!(
        wt_path.exists(),
        "worktree directory should exist at {wt_path:?}"
    );

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let branch_name = String::from_utf8(branch.stdout).unwrap().trim().to_string();
    assert_eq!(branch_name, "feature/move");
}

#[test]
fn move_current_branch_from_linked_to_main_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .arg("move")
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "move should succeed");

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", repo.path().display())),
        "protocol file should contain CD: pointing to the main worktree, got: {proto:?}"
    );
    assert_eq!(repo.current_branch(), "feature/linked");
    assert!(!wt_path.exists(), "linked worktree should be removed");
}

#[test]
fn move_explicit_branch_from_linked_to_main() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");

    common::git_mate()
        .args(["move", "feature/linked"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "feature/linked");
    assert!(!wt_path.exists(), "linked worktree should be removed");
}

#[test]
fn move_fails_on_default_branch() {
    let repo = common::RepoWithoutRemote::new();

    common::git_mate()
        .arg("move")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("default branch"));
}

#[test]
fn move_fails_on_detached_head() {
    let repo = common::RepoWithoutRemote::new();
    let sha = repo.head_commit();
    repo.git(&["checkout", "--detach", &sha]);

    common::git_mate()
        .arg("move")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("detached HEAD"));
}

#[test]
fn move_missing_branch_fails() {
    let repo = common::RepoWithoutRemote::new();

    common::git_mate()
        .args(["move", "feature/missing"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn move_branch_not_checked_out_anywhere_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/free"]);
    repo.git(&["checkout", "main"]);

    common::git_mate()
        .args(["move", "feature/free"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "is not checked out in any worktree",
        ));
}

#[test]
fn move_dirty_source_worktree_fails() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .arg("move")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("uncommitted changes"));

    assert_eq!(repo.current_branch(), "feature/dirty");
}

#[test]
fn move_dirty_main_worktree_fails_when_moving_from_linked() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .arg("move")
        .current_dir(&wt_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "main worktree has uncommitted changes",
        ));
}

#[test]
fn move_to_main_fails_when_main_is_on_another_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    repo.git(&["checkout", "-b", "feature/main-side"]);
    let wt_path =
        create_linked_worktree_from(&repo, &wt_root, "feature/linked", "feature/main-side");

    common::git_mate()
        .arg("move")
        .current_dir(&wt_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("switch main back to 'main' first"));
}

#[test]
fn move_fails_if_linked_worktree_path_does_not_match_configured_location() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    let other_root = TempDir::new().unwrap();
    let wt_path = other_root.path().join("feature-linked");
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "feature/linked",
        wt_path.to_str().unwrap(),
        "main",
    ]);

    common::git_mate()
        .arg("move")
        .current_dir(&wt_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected linked worktree path"));
}

#[test]
fn move_to_linked_fails_if_expected_directory_already_exists() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    repo.git(&["checkout", "-b", "feature/move"]);

    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/move");
    std::fs::create_dir_all(&wt_path).unwrap();

    common::git_mate()
        .arg("move")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("directory already exists"));
}

fn configure_worktree_root(repo: &common::RepoWithoutRemote, wt_root: &TempDir) {
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);
}

fn expected_worktree_path(
    repo: &common::RepoWithoutRemote,
    wt_root: &TempDir,
    branch: &str,
) -> std::path::PathBuf {
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    wt_root.path().join(repo_name).join(branch)
}

fn create_linked_worktree(
    repo: &common::RepoWithoutRemote,
    wt_root: &TempDir,
    branch: &str,
) -> std::path::PathBuf {
    create_linked_worktree_from(repo, wt_root, branch, "main")
}

fn create_linked_worktree_from(
    repo: &common::RepoWithoutRemote,
    wt_root: &TempDir,
    branch: &str,
    from_ref: &str,
) -> std::path::PathBuf {
    let wt_path = expected_worktree_path(repo, wt_root, branch);
    repo.git(&[
        "worktree",
        "add",
        "-b",
        branch,
        wt_path.to_str().unwrap(),
        from_ref,
    ]);
    wt_path
}
