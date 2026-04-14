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

    repo.git(&[
        "worktree",
        "add",
        wt_path.to_str().unwrap(),
        "-b",
        "linked-branch",
        "main",
    ]);
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
    assert!(
        proto.contains("CD:"),
        "protocol file should contain CD:, got: {proto:?}"
    );

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/checkout");
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
    assert_eq!(branch_name, "feature/checkout");
}

#[test]
fn checkout_linked_flag_without_branch_uses_current_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    repo.git(&["checkout", "-b", "feature/current"]);

    common::git_mate()
        .args(["checkout", "-w"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "main");
    assert!(expected_worktree_path(&repo, &wt_root, "feature/current").exists());
}

#[test]
fn checkout_main_flag_without_branch_uses_current_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/current");

    common::git_mate()
        .args(["checkout", "-m"])
        .current_dir(&wt_path)
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "feature/current");
    assert!(!wt_path.exists(), "linked worktree should be removed");
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
    let wt_path = wt_root
        .path()
        .join(repo_name)
        .join("feature/default-checkout");
    assert!(
        wt_path.exists(),
        "worktree directory should exist at {wt_path:?}"
    );
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
    assert!(
        !wt_path.exists(),
        "linked worktree should not be created at {wt_path:?}"
    );
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
        .stderr(predicate::str::contains(
            "does not appear to be a git worktree",
        ));
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
fn checkout_without_branch_and_without_target_flag_fails() {
    let repo = common::RepoWithoutRemote::new();

    common::git_mate()
        .arg("checkout")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("branch name is required"));
}

#[test]
fn checkout_target_without_branch_fails_on_detached_head() {
    let repo = common::RepoWithoutRemote::new();
    let sha = repo.head_commit();
    repo.git(&["checkout", "--detach", &sha]);

    common::git_mate()
        .args(["checkout", "-w"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("detached HEAD"));
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
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);

    // Commit a .gitignore that ignores .env.local and node_modules/
    std::fs::write(
        repo.path().join(".gitignore"),
        ".env.local\nnode_modules/\n",
    )
    .unwrap();
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
    assert!(
        wt_path.join(".env.local").exists(),
        ".env.local should be copied"
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env.local")).unwrap(),
        "SECRET=test"
    );

    // Blacklisted directory was NOT copied
    assert!(
        !wt_path.join("node_modules").exists(),
        "node_modules should not be copied"
    );
}

#[test]
fn checkout_worktree_does_not_overwrite_existing_files() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);

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
    repo.git(&[
        "worktree",
        "add",
        wt_path.to_str().unwrap(),
        "feature/shell-cd",
    ]);

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
    assert!(
        !wt_path.exists(),
        "linked worktree should not be created at {wt_path:?}"
    );
    assert_eq!(repo.current_branch(), "feature/current");
}

#[test]
fn checkout_linked_flag_moves_branch_from_main_to_linked_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    repo.git(&["checkout", "-b", "feature/move"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "feature/move", "-w"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "checkout should succeed");

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/move");
    assert!(
        proto.contains(&format!("CD:{}", wt_path.display())),
        "protocol file should contain CD: pointing to the new worktree, got: {proto:?}"
    );

    assert_eq!(repo.current_branch(), "main");
    assert!(wt_path.exists(), "worktree directory should exist at {wt_path:?}");

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let branch_name = String::from_utf8(branch.stdout).unwrap().trim().to_string();
    assert_eq!(branch_name, "feature/move");
}

#[test]
fn checkout_main_flag_moves_branch_from_linked_to_main_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "feature/linked", "-m"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "checkout should succeed");

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", repo.path().display())),
        "protocol file should contain CD: pointing to the main worktree, got: {proto:?}"
    );
    assert_eq!(repo.current_branch(), "feature/linked");
    assert!(!wt_path.exists(), "linked worktree should be removed");
}

#[test]
fn checkout_main_flag_replaces_clean_non_default_branch_in_main() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    repo.git(&["checkout", "-b", "feature/main-side"]);
    let wt_path =
        create_linked_worktree_from(&repo, &wt_root, "feature/linked", "feature/main-side");

    common::git_mate()
        .args(["checkout", "feature/linked", "-m"])
        .current_dir(&wt_path)
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "feature/linked");
    assert!(!wt_path.exists(), "linked worktree should be removed");
    assert!(repo.branch_exists("feature/main-side"));
}

#[test]
fn checkout_linked_flag_fails_on_dirty_main_without_stash() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["checkout", "feature/dirty", "-w"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("retry with --stash"));

    assert_eq!(repo.current_branch(), "feature/dirty");
}

#[test]
fn checkout_linked_flag_stashes_dirty_main_and_reapplies_in_linked_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["checkout", "feature/dirty", "-w", "--stash"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "main");
    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/dirty");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn checkout_main_flag_stashes_dirty_linked_branch_and_reapplies_in_main() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");
    std::fs::write(wt_path.join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["checkout", "feature/linked", "-m", "--stash"])
        .current_dir(&wt_path)
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "feature/linked");
    assert!(!wt_path.exists(), "linked worktree should be removed");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn checkout_main_flag_still_fails_if_main_worktree_is_dirty() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["checkout", "feature/linked", "-m", "--stash"])
        .current_dir(&wt_path)
        .assert()
        .failure()
        .stderr(predicate::str::contains("main worktree has uncommitted changes"));
}

#[test]
fn checkout_default_mode_prefers_existing_main_worktree_over_migrating() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);
    repo.git(&["checkout", "-b", "feature/current"]);
    repo.git(&["branch", "feature/other"]);
    let aux_wt = wt_root.path().join("aux");
    repo.git(&["worktree", "add", aux_wt.to_str().unwrap(), "feature/other"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "feature/current"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&aux_wt)
        .output()
        .unwrap();
    assert!(output.status.success());

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", repo.path().display())),
        "protocol file should point back to the main worktree, got: {proto:?}"
    );
    assert!(
        !expected_worktree_path(&repo, &wt_root, "feature/current").exists(),
        "plain checkout should navigate to the existing main worktree instead of migrating the branch"
    );
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
