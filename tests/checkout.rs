mod common;

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
fn co_completion_includes_remote_only_branches() {
    let repo = common::RepoWithRemote::new();
    repo.push_branch_to_remote("remote-feature");
    repo.local_fetch();
    repo.local_git(&["branch", "local-only"]);

    // Drive clap's dynamic completion engine directly: complete the branch
    // argument of `git-mate co ` (word index 2, empty current word).
    let output = common::git_mate()
        .args(["--", "git-mate", "co", ""])
        .env("COMPLETE", "bash")
        .env("_CLAP_COMPLETE_INDEX", "2")
        .current_dir(repo.local_path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let candidates = String::from_utf8_lossy(&output.stdout);
    let branches: Vec<&str> = candidates.lines().collect();

    assert!(
        branches.contains(&"local-only"),
        "expected local branch in completions, got: {branches:?}"
    );
    assert!(
        branches.contains(&"remote-feature"),
        "expected remote-only branch in completions, got: {branches:?}"
    );
    // The `origin/HEAD` pointer must not leak into completions.
    assert!(
        !branches
            .iter()
            .any(|b| b.ends_with("/HEAD") || *b == "origin"),
        "remote HEAD pointer should not appear, got: {branches:?}"
    );
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
fn checkout_in_place_prefers_existing_worktree_over_configured_default_location() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let external_root = TempDir::new().unwrap();
    let external_wt = external_root.path().join("branch-b");

    configure_worktree_root(&repo, &wt_root);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);
    repo.git(&["branch", "branch-a"]);
    repo.git(&["branch", "branch-b"]);
    repo.git(&["worktree", "add", external_wt.to_str().unwrap(), "branch-b"]);

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "branch-b"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains("CD:") && proto.contains(external_wt.to_str().unwrap()),
        "protocol file should contain CD: pointing to the existing non-default worktree, got: {proto:?}"
    );
    assert!(
        !expected_worktree_path(&repo, &wt_root, "branch-b").exists(),
        "plain checkout should navigate to the existing worktree instead of creating one under mate.worktreeRoot"
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
    let output = common::git_mate()
        .args(["--json", "checkout", "existing", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = common::parse_single_json(&output);
    assert_eq!(json["data"]["action"], "navigated");
    assert_eq!(json["data"]["worktree"], "linked");
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

    let output = common::git_mate()
        .args(["--json", "checkout", "existing", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["does not appear to be a git worktree"]);
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

    let output = common::git_mate()
        .args(["--json", "checkout", "existing", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["not a directory"]);
}

#[test]
fn checkout_worktree_missing_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["branch", "some-branch"]);

    let output = common::git_mate()
        .args(["--json", "checkout", "some-branch", "--linked-worktree"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["mate.worktreeRoot"]);
}

#[test]
fn checkout_without_branch_and_without_target_flag_fails() {
    let repo = common::RepoWithoutRemote::new();

    let output = common::git_mate()
        .args(["--json", "checkout"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["branch name is required"]);
}

#[test]
fn checkout_target_without_branch_fails_on_detached_head() {
    let repo = common::RepoWithoutRemote::new();
    let sha = repo.head_commit();
    repo.git(&["checkout", "--detach", &sha]);

    let output = common::git_mate()
        .args(["--json", "checkout", "-w"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["detached HEAD"]);
}

#[test]
fn checkout_in_place_navigates_to_existing_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_path = wt_root.path().join("linked");

    repo.git(&["branch", "feature/x"]);
    repo.git(&["worktree", "add", wt_path.to_str().unwrap(), "feature/x"]);

    let output = common::git_mate()
        .args(["--json", "checkout", "feature/x"])
        .current_dir(repo.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = common::parse_single_json(&output);
    assert_eq!(json["data"]["action"], "navigated");
    assert_eq!(
        json["data"]["path"],
        std::fs::canonicalize(&wt_path).unwrap().to_str().unwrap()
    );
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

    let output = common::git_mate()
        .args(["--json", "checkout", "feature/x", "--linked-worktree"])
        .current_dir(repo.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = common::parse_single_json(&output);
    assert_eq!(json["data"]["action"], "navigated");
    assert_eq!(
        json["data"]["path"],
        std::fs::canonicalize(&wt_path).unwrap().to_str().unwrap()
    );
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
fn checkout_default_branch_implies_main_worktree_in_main() {
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
        .success();

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("main");
    assert!(
        !wt_path.exists(),
        "linked worktree should not be created at {wt_path:?}"
    );
    assert_eq!(repo.current_branch(), "main");
}

#[test]
fn checkout_default_branch_implies_main_worktree_from_linked() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);
    repo.git(&["checkout", "-b", "feature/main-side"]);
    let wt_path =
        create_linked_worktree_from(&repo, &wt_root, "feature/linked", "feature/main-side");

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["checkout", "main"])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(output.status.success());

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", repo.path().display())),
        "protocol file should contain CD: pointing to the main worktree, got: {proto:?}"
    );
    assert_eq!(repo.current_branch(), "main");
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
fn checkout_linked_flag_takes_changes_from_current_main_worktree_by_default() {
    let (repo, wt_root) = repo_with_tracked_file();
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    let data = checkout_json_ok(repo.path(), &["feature/dirty", "-w"]);
    assert_eq!(data["action"], "moved");
    assert_eq!(data["stashed"], true);

    assert_eq!(repo.current_branch(), "main");
    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/dirty");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "base\n"
    );
}

#[test]
fn checkout_linked_flag_requires_a_choice_for_dirty_main_from_another_worktree() {
    let (repo, wt_root) = repo_with_tracked_file();
    let other_wt = create_linked_worktree(&repo, &wt_root, "feature/other");
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    let output = checkout_json_err(&other_wt, &["feature/dirty", "-w"]);
    common::assert_json_error(&output, &["--take-changes", "--start-fresh"]);
    let data = common::assert_json_error_code(&output, "uncommitted_changes");
    assert_eq!(
        data["retryWith"],
        serde_json::json!(["--take-changes", "--start-fresh"])
    );

    assert_eq!(repo.current_branch(), "feature/dirty");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn checkout_linked_flag_take_changes_moves_dirty_main_from_another_worktree() {
    let (repo, wt_root) = repo_with_tracked_file();
    let other_wt = create_linked_worktree(&repo, &wt_root, "feature/other");
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    let data = checkout_json_ok(&other_wt, &["feature/dirty", "-w", "--take-changes"]);
    assert_eq!(data["stashed"], true);

    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/dirty");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn checkout_linked_flag_start_fresh_leaves_changes_in_main_worktree() {
    let (repo, wt_root) = repo_with_tracked_file();
    repo.git(&["checkout", "-b", "feature/dirty"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    let data = checkout_json_ok(repo.path(), &["feature/dirty", "-w", "--start-fresh"]);
    assert_eq!(data["action"], "moved");
    assert_eq!(data["stashed"], false);

    assert_eq!(repo.current_branch(), "main");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/dirty");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "base\n"
    );
}

#[test]
fn checkout_linked_flag_does_not_require_a_choice_for_config_only_main_worktree() {
    let (repo, wt_root) = repo_with_tracked_file();
    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "add gitignore"]);
    let other_wt = create_linked_worktree(&repo, &wt_root, "feature/other");
    repo.git(&["checkout", "-b", "feature/config"]);
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();

    let data = checkout_json_ok(&other_wt, &["feature/config", "-w"]);
    assert_eq!(data["action"], "moved");
    assert_eq!(data["stashed"], false);

    let wt_path = expected_worktree_path(&repo, &wt_root, "feature/config");
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env.local")).unwrap(),
        "SECRET=test"
    );
}

#[test]
fn checkout_main_flag_takes_changes_from_current_linked_worktree_by_default() {
    let (repo, wt_root) = repo_with_tracked_file();
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");
    std::fs::write(wt_path.join("tracked.txt"), "changed\n").unwrap();

    let data = checkout_json_ok(&wt_path, &["feature/linked", "-m"]);
    assert_eq!(data["action"], "moved");
    assert_eq!(data["stashed"], true);

    assert_eq!(repo.current_branch(), "feature/linked");
    assert!(!wt_path.exists(), "linked worktree should be removed");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn checkout_main_flag_requires_take_changes_for_dirty_linked_worktree_from_main() {
    let (repo, wt_root) = repo_with_tracked_file();
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");
    std::fs::write(wt_path.join("tracked.txt"), "changed\n").unwrap();

    let output = checkout_json_err(repo.path(), &["feature/linked", "-m"]);
    common::assert_json_error(&output, &["--take-changes"]);
    let data = common::assert_json_error_code(&output, "uncommitted_changes");
    assert_eq!(data["retryWith"], serde_json::json!(["--take-changes"]));
    assert_eq!(repo.current_branch(), "main");
    assert!(wt_path.exists(), "linked worktree should be kept");

    let data = checkout_json_ok(repo.path(), &["feature/linked", "-m", "--take-changes"]);
    assert_eq!(data["stashed"], true);
    assert_eq!(repo.current_branch(), "feature/linked");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn checkout_main_flag_start_fresh_fails_for_dirty_linked_worktree() {
    let (repo, wt_root) = repo_with_tracked_file();
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");
    std::fs::write(wt_path.join("tracked.txt"), "changed\n").unwrap();

    let output = checkout_json_err(&wt_path, &["feature/linked", "-m", "--start-fresh"]);
    common::assert_json_error(&output, &["--start-fresh", "--take-changes"]);
    let data = common::assert_json_error_code(&output, "uncommitted_changes");
    assert_eq!(data["retryWith"], serde_json::json!(["--take-changes"]));

    assert_eq!(repo.current_branch(), "main");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn checkout_main_flag_copies_local_config_from_linked_worktree() {
    let (repo, wt_root) = repo_with_tracked_file();
    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "add gitignore"]);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/linked");
    std::fs::write(wt_path.join(".env.local"), "SECRET=linked").unwrap();

    checkout_json_ok(&wt_path, &["feature/linked", "-m"]);

    assert_eq!(
        std::fs::read_to_string(repo.path().join(".env.local")).unwrap(),
        "SECRET=linked"
    );
}

/// A repo with a committed `tracked.txt` ("base") on `main` and
/// `mate.worktreeRoot` configured.
fn repo_with_tracked_file() -> (common::RepoWithoutRemote, TempDir) {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    (repo, wt_root)
}

fn checkout_json_ok(dir: &std::path::Path, args: &[&str]) -> serde_json::Value {
    let output = common::git_mate()
        .args(["--json", "checkout"])
        .args(args)
        .current_dir(dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    common::parse_single_json(&output)["data"].clone()
}

fn checkout_json_err(dir: &std::path::Path, args: &[&str]) -> Vec<u8> {
    common::git_mate()
        .args(["--json", "checkout"])
        .args(args)
        .current_dir(dir)
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone()
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

    let output = common::git_mate()
        .args(["--json", "checkout", "feature/linked", "-m"])
        .current_dir(&wt_path)
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["main worktree has uncommitted changes"]);
    let data = common::assert_json_error_code(&output, "uncommitted_changes");
    assert_eq!(data["retryWith"], serde_json::json!([]));
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

#[test]
fn json_reports_checkout_result() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);
    repo.git(&["branch", "feature/main"]);
    repo.git(&["branch", "feature/linked"]);
    let checkout = |args: &[&str]| {
        let output = common::git_mate()
            .args(["--json", "checkout"])
            .args(args)
            .current_dir(repo.path())
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        common::parse_single_json(&output)["data"].clone()
    };

    let data = checkout(&["feature/main"]);
    assert_eq!(data["branch"], "feature/main");
    assert_eq!(data["worktree"], "main");
    assert_eq!(data["action"], "switched");
    assert_eq!(data["stashed"], false);

    let data = checkout(&["feature/linked", "-w"]);
    assert_eq!(data["worktree"], "linked");
    assert_eq!(data["action"], "created");
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path =
        std::fs::canonicalize(wt_root.path().join(repo_name).join("feature/linked")).unwrap();
    assert_eq!(data["path"], wt_path.to_str().unwrap());

    // feature/main is checked out in the main worktree; moving it out.
    let data = checkout(&["feature/main", "-w"]);
    assert_eq!(data["worktree"], "linked");
    assert_eq!(data["action"], "moved");
}

fn checkout_json(dir: &std::path::Path, branch: &str) -> serde_json::Value {
    let output = common::git_mate()
        .args(["--json", "checkout", branch])
        .current_dir(dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    common::parse_single_json(&output)["data"].clone()
}

#[test]
fn checkout_fetches_a_branch_not_known_locally() {
    let repo = common::RepoWithRemote::new();
    // Pushed after the clone's last fetch: no origin/late-feature yet.
    repo.push_branch_to_remote("late-feature");
    assert!(!repo.remote_tracking_exists("origin/late-feature"));

    let data = checkout_json(repo.local_path(), "late-feature");
    assert_eq!(data["fetched"], true);
    assert_eq!(data["branch"], "late-feature");
    assert_eq!(repo.local_current_branch(), "late-feature");
}

#[test]
fn checkout_does_not_fetch_a_known_branch() {
    let repo = common::RepoWithRemote::new();
    repo.push_branch_to_remote("known-feature");
    repo.local_fetch();
    let fetched_tip = repo.branch_tip("origin/known-feature");
    repo.push_commit_to_remote_branch("known-feature", "newer");

    let data = checkout_json(repo.local_path(), "known-feature");
    assert_eq!(data["fetched"], false);
    assert_eq!(repo.branch_tip("origin/known-feature"), fetched_tip);
}

#[test]
fn checkout_fails_clearly_when_the_branch_is_not_on_any_remote() {
    let repo = common::RepoWithRemote::new();
    let output = common::git_mate()
        .args(["--json", "checkout", "no-such-branch"])
        .current_dir(repo.local_path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    common::assert_json_error(&output, &["no-such-branch", "not found", "origin"]);
    assert_eq!(
        common::assert_json_error_code(&output, "branch_not_found")["branch"],
        "no-such-branch"
    );
}

#[test]
fn checkout_recreates_worktree_whose_directory_was_deleted() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/deleted");
    std::fs::remove_dir_all(&wt_path).unwrap();

    let data = checkout_json(repo.path(), "feature/deleted");
    assert_eq!(data["worktree"], "linked");
    assert_eq!(data["action"], "created");
    assert_eq!(
        data["path"],
        std::fs::canonicalize(&wt_path).unwrap().to_str().unwrap()
    );
}

#[test]
fn checkout_main_flag_switches_branch_whose_worktree_directory_was_deleted() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let wt_path = create_linked_worktree(&repo, &wt_root, "feature/deleted");
    std::fs::remove_dir_all(&wt_path).unwrap();

    let output = common::git_mate()
        .args(["--json", "checkout", "feature/deleted", "--main-worktree"])
        .current_dir(repo.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let data = common::parse_single_json(&output)["data"].clone();
    assert_eq!(data["worktree"], "main");
    assert_eq!(data["action"], "switched");
    assert_eq!(repo.current_branch(), "feature/deleted");
}

#[test]
fn json_default_branch_in_linked_worktree_is_default_branch() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-q", "-b", "feature/x"]);

    let output = common::git_mate()
        .args(["--json", "checkout", "main", "-w"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        common::assert_json_error_code(&output, "default_branch")["branch"],
        "main"
    );
    assert_eq!(repo.current_branch(), "feature/x");
}

const PR_REMOTE_URL: &str = "https://github.com/acme/widget.git";

/// A clone whose `origin` claims to be `remote_url` on GitHub while git is
/// redirected to the local bare repo via `url.<base>.insteadOf`.
fn repo_posing_as_github(remote_url: &str) -> common::RepoWithRemote {
    let repo = common::RepoWithRemote::new();
    let bare = repo.bare_path().to_str().unwrap().to_string();
    repo.local_git(&["config", &format!("url.{bare}.insteadOf"), remote_url]);
    repo.local_git(&["remote", "set-url", "origin", remote_url]);
    repo
}

/// Publish `commit` as the head of pull request `number` on the bare remote.
fn open_pull_request(repo: &common::RepoWithRemote, number: u32, commit: &str) {
    common::git(
        repo.bare_path(),
        &["update-ref", &format!("refs/pull/{number}/head"), commit],
    );
}

fn remote_tip(repo: &common::RepoWithRemote, branch: &str) -> String {
    let out = Command::new("git")
        .args(["rev-parse", &format!("refs/heads/{branch}")])
        .current_dir(repo.bare_path())
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

const PR_URL: &str = "https://github.com/acme/widget/pull/7";

#[test]
fn checkout_pull_request_url_checks_out_its_branch() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);
    repo.push_branch_to_remote("feature/login");
    open_pull_request(&repo, 7, &remote_tip(&repo, "feature/login"));

    let data = checkout_json(repo.local_path(), &format!("{PR_URL}/files"));

    assert_eq!(repo.local_current_branch(), "feature/login");
    assert_eq!(data["branch"], "feature/login");
    assert_eq!(data["fetched"], true);
    assert_eq!(data["pullRequest"]["number"], 7);
    assert_eq!(data["pullRequest"]["url"], format!("{PR_URL}/files"));
    assert_eq!(data["pullRequest"]["remote"], "origin");
}

#[test]
fn checkout_pull_request_matches_an_scp_like_remote_url() {
    let repo = repo_posing_as_github("git@github.com:acme/widget.git");
    repo.push_branch_to_remote("feature/login");
    open_pull_request(&repo, 7, &remote_tip(&repo, "feature/login"));

    let data = checkout_json(repo.local_path(), PR_URL);

    assert_eq!(data["branch"], "feature/login");
    assert_eq!(repo.local_current_branch(), "feature/login");
}

#[test]
fn checkout_pull_request_does_not_fetch_an_up_to_date_branch() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);
    repo.push_branch_to_remote("feature/login");
    open_pull_request(&repo, 7, &remote_tip(&repo, "feature/login"));
    repo.local_fetch();

    let data = checkout_json(repo.local_path(), PR_URL);

    assert_eq!(data["fetched"], false);
    assert_eq!(repo.local_current_branch(), "feature/login");
}

#[test]
fn checkout_pull_request_fetches_a_branch_behind_the_pull_request() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);
    repo.push_branch_to_remote("feature/login");
    repo.local_fetch();
    repo.push_commit_to_remote_branch("feature/login", "newer");
    let head = remote_tip(&repo, "feature/login");
    open_pull_request(&repo, 7, &head);

    let data = checkout_json(repo.local_path(), PR_URL);

    assert_eq!(data["fetched"], true);
    assert_eq!(repo.branch_tip("origin/feature/login"), head);
    assert_eq!(repo.local_head_commit(), head);
}

#[test]
fn checkout_pull_request_from_a_fork_is_rejected() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);
    // A head commit no branch on the remote points to, as for a fork.
    repo.push_branch_to_remote("fork-work");
    open_pull_request(&repo, 7, &remote_tip(&repo, "fork-work"));
    repo.delete_remote_branch("fork-work");

    let output = checkout_json_err(repo.local_path(), &[PR_URL]);

    let data = common::assert_json_error_code(&output, "pull_request_branch_not_found");
    assert_eq!(data["number"], 7);
    assert_eq!(data["remote"], "origin");
    assert_eq!(repo.local_current_branch(), "main");
}

#[test]
fn checkout_pull_request_that_does_not_exist_fails() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);

    let output = checkout_json_err(repo.local_path(), &[PR_URL]);

    let data = common::assert_json_error_code(&output, "pull_request_not_found");
    assert_eq!(data["number"], 7);
    assert_eq!(repo.local_current_branch(), "main");
}

#[test]
fn checkout_pull_request_of_another_repository_fails() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);

    let output = checkout_json_err(repo.local_path(), &["https://github.com/acme/other/pull/7"]);

    common::assert_json_error_code(&output, "remote_not_found");
}

#[test]
fn checkout_pull_request_with_several_branches_at_its_head_fails() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);
    repo.push_branch_to_remote("feature/a");
    common::git(repo.bare_path(), &["branch", "feature/b", "feature/a"]);
    open_pull_request(&repo, 7, &remote_tip(&repo, "feature/a"));

    let output = checkout_json_err(repo.local_path(), &[PR_URL]);

    let data = common::assert_json_error_code(&output, "ambiguous_pull_request_branch");
    assert_eq!(
        data["branches"],
        serde_json::json!(["feature/a", "feature/b"])
    );
    assert_eq!(repo.local_current_branch(), "main");
}

#[test]
fn checkout_rejects_a_url_that_is_not_a_pull_request() {
    let repo = repo_posing_as_github(PR_REMOTE_URL);

    let output = checkout_json_err(
        repo.local_path(),
        &["https://github.com/acme/widget/issues/7"],
    );

    common::assert_json_error_code(&output, "invalid_pull_request_url");
}
