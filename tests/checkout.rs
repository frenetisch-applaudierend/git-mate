mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn checkout_in_place_from_main_worktree() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "existing-branch"]);

    common::git_mate()
        .args(["checkout", "existing-branch"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "existing-branch");
}

#[test]
fn checkout_in_place_from_linked_worktree_cds_to_main_and_switches() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "-b", "linked-branch", "main"]);
    repo.git(&["branch", "other-branch"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "other-branch"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(output.status.success());

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    let main_path = repo.path().to_str().unwrap();
    assert!(
        proto.contains("CD:") && proto.contains(main_path),
        "protocol file should contain CD: pointing to main worktree, got: {proto:?}"
    );

    assert_eq!(repo.current_branch(), "other-branch");
}

#[test]
fn checkout_in_place_from_linked_worktree_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_a = wt_root.path().join("a");
    let wt_b = wt_root.path().join("b");

    repo.git(&["branch", "branch-a"]);
    repo.git(&["branch", "branch-b"]);
    repo.git(&["worktree", "add", wt_a.to_str().unwrap(), "branch-a"]);
    repo.git(&["worktree", "add", wt_b.to_str().unwrap(), "branch-b"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "branch-b"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&wt_a)
        .output()
        .unwrap();
    assert!(output.status.success());

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains("CD:") && proto.contains(wt_b.to_str().unwrap()),
        "protocol file should contain CD: pointing to branch-b worktree, got: {proto:?}"
    );
}

#[test]
fn checkout_worktree_creates_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "feature/checkout"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "feature/checkout", "-w"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(proto.contains("CD:"), "protocol file should contain CD:, got: {proto:?}");

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/checkout");
    assert!(wt_path.exists(), "worktree directory should exist at {wt_path:?}");

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let branch_name = String::from_utf8(branch.stdout).unwrap().trim().to_string();
    assert_eq!(branch_name, "feature/checkout");
}

#[test]
fn checkout_uses_configured_worktree_default() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);
    repo.git(&["branch", "feature/default-checkout"]);

    common::git_mate()
        .args(["checkout", "feature/default-checkout"])
        .current_dir(repo.path())
        .assert()
        .success();

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/default-checkout");
    assert!(wt_path.exists(), "worktree directory should exist at {wt_path:?}");
}

#[test]
fn main_worktree_flag_overrides_configured_checkout_worktree_default() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);
    repo.git(&["branch", "feature/override-main"]);

    common::git_mate()
        .args(["checkout", "feature/override-main", "-m"])
        .current_dir(repo.path())
        .assert()
        .success();

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/override-main");
    assert!(!wt_path.exists(), "linked worktree should not be created at {wt_path:?}");
    assert_eq!(repo.current_branch(), "feature/override-main");
}

#[test]
fn checkout_worktree_noop_if_path_exists() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "existing"]);

    // First call creates the worktree
    common::git_mate()
        .args(["checkout", "existing", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .success();

    // Second call should be a no-op
    common::git_mate()
        .args(["checkout", "existing", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("already checked out"));
}

#[test]
fn checkout_worktree_fails_if_directory_exists_but_is_not_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "existing"]);

    // Create a plain directory at the worktree path (no .git file)
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("existing");
    std::fs::create_dir_all(&wt_path).unwrap();

    common::git_mate()
        .args(["checkout", "existing", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not appear to be a git worktree"));
}

#[test]
fn checkout_worktree_fails_if_file_exists_at_path() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["branch", "existing"]);

    // Create a plain file at the worktree path
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("existing");
    std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();
    std::fs::write(&wt_path, "not a directory").unwrap();

    common::git_mate()
        .args(["checkout", "existing", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a directory"));
}

#[test]
fn checkout_worktree_missing_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "some-branch"]);

    common::git_mate()
        .args(["checkout", "some-branch", "--linked-worktree"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.worktreeRoot"));
}

#[test]
fn checkout_in_place_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["branch", "feature/x"]);
    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "feature/x"]);

    common::git_mate()
        .args(["checkout", "feature/x"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(wt_path.to_str().unwrap()));
}

#[test]
fn checkout_worktree_copies_ignored_files() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&["config", "mate.worktreeRoot", wt_root.path().to_str().unwrap()]);

    // Commit a .gitignore that ignores .env.local and node_modules/
    std::fs::write(repo.path().join(".gitignore"), ".env.local\nnode_modules/\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "add gitignore"]);

    // Create the branch to check out
    repo.git(&["branch", "feature/copy-test"]);

    // Create ignored files in the main worktree
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();
    std::fs::create_dir(repo.path().join("node_modules")).unwrap();
    std::fs::write(repo.path().join("node_modules").join("pkg.json"), "{}").unwrap();

    common::git_mate()
        .args(["checkout", "feature/copy-test", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .success();

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/copy-test");

    // Ignored file was copied
    assert!(wt_path.join(".env.local").exists(), ".env.local should be copied");
    assert_eq!(std::fs::read_to_string(wt_path.join(".env.local")).unwrap(), "SECRET=test");

    // Blacklisted directory was NOT copied
    assert!(!wt_path.join("node_modules").exists(), "node_modules should not be copied");
}

#[test]
fn checkout_worktree_does_not_overwrite_existing_files() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&["config", "mate.worktreeRoot", wt_root.path().to_str().unwrap()]);

    // Commit .env.local on a branch (before it was gitignored)
    std::fs::write(repo.path().join(".env.local"), "COMMITTED=value").unwrap();
    repo.git(&["add", ".env.local"]);
    repo.git(&["commit", "-m", "add env"]);

    // Now gitignore it and switch back to main
    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "gitignore env"]);
    repo.git(&["branch", "feature/no-overwrite", "HEAD~1"]);

    // Put a different .env.local in main worktree
    std::fs::write(repo.path().join(".env.local"), "LOCAL=override").unwrap();

    common::git_mate()
        .args(["checkout", "feature/no-overwrite", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .success();

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/no-overwrite");

    // The committed version should be preserved, not overwritten by the main worktree copy
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env.local")).unwrap(),
        "COMMITTED=value"
    );
}

#[test]
fn checkout_worktree_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["branch", "feature/x"]);
    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "feature/x"]);

    common::git_mate()
        .args(["checkout", "feature/x", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(wt_path.to_str().unwrap()));
}

#[test]
fn checkout_worktree_existing_worktree_emits_cd_for_shell_wrapper() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["branch", "feature/shell-cd"]);
    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "feature/shell-cd"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "feature/shell-cd", "--linked-worktree"])
        .env("GIT_MATE_PROTO", &proto_path)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains("CD:") && proto.contains(wt_path.to_str().unwrap()),
        "protocol file should contain CD: pointing to feature/shell-cd worktree, got: {proto:?}"
    );
}

#[test]
fn checkout_worktree_rejects_default_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);
    repo.git(&["checkout", "-b", "feature/current"]);

    common::git_mate()
        .args(["checkout", "main"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("default branch"))
        .stderr(predicate::str::contains("main worktree"));

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("main");
    assert!(!wt_path.exists(), "linked worktree should not be created at {wt_path:?}");
    assert_eq!(repo.current_branch(), "feature/current");
}
