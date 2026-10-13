mod common;

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

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args(["new", "feature/login", "-w", "--from", "main"])
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

    // Derive expected path: <wt_root>/<repo-dir-name>/feature/login
    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/login");
    assert!(
        wt_path.exists(),
        "worktree directory should exist at {wt_path:?}"
    );

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
fn worktree_mode_sets_push_tracking_to_branch_name() {
    let setup = common::RepoWithRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    setup.local_git(&["config", "mate.worktreeRoot", wt_root_str]);

    common::git_mate()
        .args(["new", "feature/login", "-w"])
        .current_dir(setup.local_path())
        .assert()
        .success();

    let repo_name = setup.local_path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feature/login");
    let remote = Command::new("git")
        .args(["config", "--get", "branch.feature/login.remote"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    let merge = Command::new("git")
        .args(["config", "--get", "branch.feature/login.merge"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    assert!(
        remote.status.success(),
        "expected branch remote to be configured"
    );
    assert!(
        merge.status.success(),
        "expected branch merge ref to be configured"
    );
    assert_eq!(String::from_utf8(remote.stdout).unwrap().trim(), "origin");
    assert_eq!(
        String::from_utf8(merge.stdout).unwrap().trim(),
        "refs/heads/feature/login"
    );
}

#[test]
fn default_worktree_mode_from_config_creates_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);

    let output = common::git_mate()
        .args(["new", "feature/default-worktree", "--from", "main"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root
        .path()
        .join(repo_name)
        .join("feature/default-worktree");
    assert!(
        wt_path.exists(),
        "worktree directory should exist at {wt_path:?}"
    );
}

#[test]
fn main_worktree_flag_overrides_configured_worktree_default() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    let wt_root_str = wt_root.path().to_str().unwrap();

    repo.git(&["config", "mate.worktreeRoot", wt_root_str]);
    repo.git(&["config", "mate.defaultBranchMode", "linked"]);

    common::git_mate()
        .args(["new", "feature/override-main", "-m", "--from", "main"])
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
    assert_ne!(
        setup.branch_tip("feat/x"),
        old_head,
        "branch should be rooted at remote's new commit"
    );
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

    assert_eq!(
        setup.branch_tip("feat/x"),
        old_head,
        "branch should be rooted at old local HEAD (fetch skipped)"
    );
}

#[test]
fn fetch_config_false_skips_fetch() {
    let setup = common::RepoWithRemote::new();
    let old_head = setup.local_head_commit();

    setup.push_commit_to_remote("remote update");
    setup.local_git(&["config", "mate.new.fetch", "false"]);

    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();

    assert_eq!(
        setup.branch_tip("feat/x"),
        old_head,
        "branch should be rooted at old local HEAD (config fetch=false)"
    );
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
    setup.local_git(&["config", "mate.new.fetch", value]);
    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_eq!(
        setup.branch_tip("feat/x"),
        old_head,
        "mate.new.fetch={value} should skip fetch"
    );
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
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);

    common::git_mate()
        .args(["new", "../evil", "--linked-worktree", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid branch name"));
}

#[test]
fn worktree_mode_missing_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["new", "feat/x", "--linked-worktree", "--from", "main"])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.worktreeRoot"));
}

#[test]
fn invalid_default_branch_mode_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.defaultBranchMode", "banana"]);

    common::git_mate()
        .args(["new", "feat/x", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.defaultBranchMode"));
}

#[test]
fn worktree_mode_copies_ignored_files() {
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

    // Create ignored files in the main worktree
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();
    std::fs::create_dir(repo.path().join("node_modules")).unwrap();
    std::fs::write(repo.path().join("node_modules").join("pkg.json"), "{}").unwrap();

    common::git_mate()
        .args(["new", "feat/x", "--linked-worktree", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();

    let repo_name = repo.path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("feat/x");

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
fn worktree_mode_rejects_default_branch() {
    let setup = common::RepoWithRemote::new();
    let wt_root = TempDir::new().unwrap();

    setup.local_git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);
    setup.local_git(&["config", "mate.defaultBranchMode", "linked"]);
    setup.local_git(&["checkout", "-b", "feature/current"]);
    setup.local_git(&["branch", "-D", "main"]);

    common::git_mate()
        .args(["new", "main"])
        .current_dir(setup.local_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("default branch"))
        .stderr(predicate::str::contains("main worktree"));

    let repo_name = setup.local_path().file_name().unwrap().to_str().unwrap();
    let wt_path = wt_root.path().join(repo_name).join("main");
    assert!(
        !wt_path.exists(),
        "linked worktree should not be created at {wt_path:?}"
    );
    assert_eq!(setup.local_current_branch(), "feature/current");
    assert!(!setup.local_branch_exists("main"));
}

#[test]
fn worktree_mode_does_not_copy_local_config_from_non_worktree_ref() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    repo.git(&[
        "config",
        "mate.worktreeRoot",
        wt_root.path().to_str().unwrap(),
    ]);

    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "add gitignore"]);
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();

    let from_ref = repo.head_commit();

    common::git_mate()
        .args([
            "new",
            "feat/from-sha",
            "--linked-worktree",
            "--from",
            &from_ref,
        ])
        .current_dir(repo.path())
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/from-sha");
    assert!(
        !wt_path.join(".env.local").exists(),
        "local config should stay behind when the starting point is not checked out in a worktree"
    );
}

#[test]
fn worktree_mode_requires_decision_for_dirty_starting_branch_in_other_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    let linked_wt = create_linked_worktree(&repo, &wt_root, "feature/elsewhere");

    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["new", "feat/dirty", "--linked-worktree", "--from", "main"])
        .current_dir(&linked_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--take-changes"))
        .stderr(predicate::str::contains("--start-fresh"));
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn worktree_mode_start_fresh_leaves_changes_but_copies_config() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", ".gitignore", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);

    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();

    common::git_mate()
        .args([
            "new",
            "feat/fresh",
            "--linked-worktree",
            "--from",
            "main",
            "--start-fresh",
        ])
        .current_dir(repo.path())
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/fresh");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env.local")).unwrap(),
        "SECRET=test",
        "local config is copied even with --start-fresh"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n",
        "changes stay in the source worktree"
    );
}

#[test]
fn worktree_mode_takes_changes_from_current_branch_by_default() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", ".gitignore", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);

    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();

    common::git_mate()
        .args([
            "new",
            "feat/continued",
            "--linked-worktree",
            "--from",
            "main",
        ])
        .current_dir(repo.path())
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/continued");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "base\n",
        "changes should move out of the source worktree"
    );
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env.local")).unwrap(),
        "SECRET=test"
    );
}

#[test]
fn main_worktree_mode_allows_in_place_dirty_starting_branch() {
    let repo = common::RepoWithoutRemote::new();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["new", "feat/in-place", "--main-worktree", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();

    assert_eq!(repo.current_branch(), "feat/in-place");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn main_worktree_mode_rejects_start_fresh_when_destination_is_dirty() {
    let repo = common::RepoWithoutRemote::new();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["branch", "branch-b"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args([
            "new",
            "feat/no-ignore",
            "--main-worktree",
            "--from",
            "branch-b",
            "--start-fresh",
        ])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot use --start-fresh"));
}

#[test]
fn main_worktree_flag_creates_branch_in_main_from_linked_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    let linked_wt = create_linked_worktree(&repo, &wt_root, "feature/linked");

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args([
            "new",
            "feature/main-dest",
            "--main-worktree",
            "--from",
            "main",
        ])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(&linked_wt)
        .output()
        .unwrap();
    assert!(output.status.success());

    let proto = std::fs::read_to_string(&proto_path).unwrap();
    assert!(
        proto.contains(&format!("CD:{}", repo.path().display())),
        "protocol file should navigate back to the main worktree, got: {proto:?}"
    );
    assert_eq!(repo.current_branch(), "feature/main-dest");

    let linked_branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&linked_wt)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(linked_branch.stdout).unwrap().trim(),
        "feature/linked"
    );
}

#[test]
fn main_worktree_mode_take_changes_moves_changes_from_dirty_linked_starting_branch() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", ".gitignore", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);

    let linked_wt = create_linked_worktree(&repo, &wt_root, "feature/source");
    std::fs::write(linked_wt.join("tracked.txt"), "changed\n").unwrap();
    std::fs::write(linked_wt.join(".env.local"), "SECRET=test").unwrap();

    let (_proto_guard, proto_path) = common::proto_file();
    let output = common::git_mate()
        .args([
            "new",
            "feature/from-linked",
            "--main-worktree",
            "--from",
            "feature/source",
            "--take-changes",
        ])
        .env("GIT_MATE_PROTO", &proto_path)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    assert_eq!(repo.current_branch(), "feature/from-linked");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".env.local")).unwrap(),
        "SECRET=test"
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
    let wt_path = expected_worktree_path(repo, wt_root, branch);
    repo.git(&[
        "worktree",
        "add",
        "-b",
        branch,
        wt_path.to_str().unwrap(),
        "main",
    ]);
    wt_path
}

#[test]
fn main_worktree_mode_requires_take_changes_for_dirty_unrelated_branch() {
    let repo = common::RepoWithoutRemote::new();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["checkout", "-b", "feat/old"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["new", "feat/x", "--main-worktree", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--take-changes"));
    assert_eq!(repo.current_branch(), "feat/old");
}

#[test]
fn main_worktree_mode_rejects_start_fresh_in_place() {
    let repo = common::RepoWithoutRemote::new();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args(["new", "feat/x", "--main-worktree", "--start-fresh"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--start-fresh"));
    assert_eq!(repo.current_branch(), "main");
}

#[test]
fn worktree_mode_take_changes_from_other_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    let linked_wt = create_linked_worktree(&repo, &wt_root, "feature/elsewhere");
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();

    common::git_mate()
        .args([
            "new",
            "feat/taken",
            "-w",
            "--from",
            "main",
            "--take-changes",
        ])
        .current_dir(&linked_wt)
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/taken");
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
fn worktree_mode_copies_config_from_other_worktree_without_flag() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "add gitignore"]);
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();
    let linked_wt = create_linked_worktree(&repo, &wt_root, "feature/elsewhere");

    common::git_mate()
        .args(["new", "feat/cfg", "-w", "--from", "main"])
        .current_dir(&linked_wt)
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/cfg");
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env.local")).unwrap(),
        "SECRET=test"
    );
}

#[test]
fn from_dot_uses_current_branch_and_takes_changes() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    let linked_wt = create_linked_worktree(&repo, &wt_root, "feature/current");
    std::fs::write(linked_wt.join("tracked.txt"), "work\n").unwrap();
    std::fs::write(linked_wt.join("committed.txt"), "x\n").unwrap();
    common::git(&linked_wt, &["add", "committed.txt"]);
    common::git(&linked_wt, &["commit", "-m", "on current"]);

    common::git_mate()
        .args(["new", "feat/child", "-w", "--from", "."])
        .current_dir(&linked_wt)
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/child");
    assert!(wt_path.join("committed.txt").exists());
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "work\n"
    );
}

#[test]
fn from_dot_on_detached_head_uses_commit() {
    let repo = common::RepoWithoutRemote::new();
    let head = repo.head_commit();
    repo.git(&["checkout", "--detach"]);

    common::git_mate()
        .args(["new", "feat/detached", "-m", "--from", "."])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/detached");
    assert_eq!(repo.head_commit(), head);
}

fn parse_single_json(stdout: &[u8]) -> serde_json::Value {
    let text = String::from_utf8(stdout.to_vec()).unwrap();
    assert_eq!(
        text.lines().count(),
        1,
        "stdout must be exactly one JSON line, got: {text:?}"
    );
    serde_json::from_str(&text).unwrap()
}

#[test]
fn json_linked_worktree_success() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", ".gitignore", "tracked.txt"]);
    repo.git(&["commit", "-m", "add files"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();

    for verbose in [false, true] {
        let branch = if verbose {
            "feat/json-verbose"
        } else {
            "feat/json"
        };
        let mut args = vec!["--json", "new", branch, "-w", "--from", "main"];
        if verbose {
            args.push("--verbose");
            std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
        }
        let output = common::git_mate()
            .args(&args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        let json = parse_single_json(&output.stdout);
        let wt_path =
            std::fs::canonicalize(expected_worktree_path(&repo, &wt_root, branch)).unwrap();
        let main_path = std::fs::canonicalize(repo.path()).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["command"], "new");
        assert_eq!(json["data"]["branch"], branch);
        assert_eq!(json["data"]["from"], "main");
        assert_eq!(json["data"]["worktree"], "linked");
        assert_eq!(json["data"]["path"], wt_path.to_str().unwrap());
        assert_eq!(json["data"]["fetched"], false);
        assert_eq!(json["data"]["upstream"], serde_json::Value::Null);
        assert_eq!(json["data"]["carriedFrom"], main_path.to_str().unwrap());
        assert_eq!(json["data"]["copiedConfigFiles"], 1);
    }
}

#[test]
fn json_main_worktree_with_remote() {
    let setup = common::RepoWithRemote::new();
    let output = common::git_mate()
        .args(["--json", "new", "feat/json", "-m"])
        .current_dir(setup.local_path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let json = parse_single_json(&output.stdout);
    assert_eq!(json["data"]["worktree"], "main");
    assert_eq!(json["data"]["fetched"], true);
    assert_eq!(json["data"]["upstream"], "origin/feat/json");
    assert_eq!(json["data"]["carriedFrom"], serde_json::Value::Null);
    assert_eq!(json["data"]["copiedConfigFiles"], 0);
}

#[test]
fn json_error_envelope() {
    let repo = common::RepoWithoutRemote::new();
    let output = common::git_mate()
        .args(["--json", "new", "bad..name", "-w", "--from", "main"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json = parse_single_json(&output.stdout);
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "error");
    assert!(json.get("data").is_none());
}

#[test]
fn json_stash_reapply_failure_reports_created_branch() {
    let repo = common::RepoWithoutRemote::new();
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    repo.git(&["checkout", "-b", "feat/old"]);
    std::fs::write(repo.path().join("tracked.txt"), "old\n").unwrap();
    repo.git(&["commit", "-am", "old change"]);
    std::fs::write(repo.path().join("tracked.txt"), "conflicting\n").unwrap();

    let output = common::git_mate()
        .args([
            "--json",
            "new",
            "feat/x",
            "-m",
            "--from",
            "main",
            "--take-changes",
        ])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json = parse_single_json(&output.stdout);
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "stash_reapply_failed");
    assert_eq!(json["data"]["branch"], "feat/x");
    assert_eq!(json["data"]["carriedFrom"], serde_json::Value::Null);
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn from_config_current_uses_current_branch() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["checkout", "-b", "feature/base"]);
    repo.git(&["commit", "--allow-empty", "-m", "on base"]);
    let base = repo.head_commit();
    repo.git(&["config", "mate.new.from", "current"]);

    common::git_mate()
        .args(["new", "feat/child", "-m"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/child");
    assert_eq!(repo.head_commit(), base);
}

#[test]
fn from_config_ref_is_used_and_flag_overrides_it() {
    let repo = common::RepoWithoutRemote::new();
    let main = repo.head_commit();
    repo.git(&["checkout", "-b", "develop"]);
    repo.git(&["commit", "--allow-empty", "-m", "on develop"]);
    let develop = repo.head_commit();
    repo.git(&["checkout", "main"]);
    repo.git(&["config", "mate.new.from", "develop"]);

    common::git_mate()
        .args(["new", "feat/from-config", "-m"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.head_commit(), develop);

    repo.git(&["checkout", "main"]);
    common::git_mate()
        .args(["new", "feat/from-flag", "-m", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.head_commit(), main);
}

#[test]
fn from_config_default_uses_default_branch() {
    let repo = common::RepoWithoutRemote::new();
    let main = repo.head_commit();
    repo.git(&["checkout", "-b", "feature/base"]);
    repo.git(&["commit", "--allow-empty", "-m", "on base"]);
    repo.git(&["config", "mate.new.from", "default"]);

    common::git_mate()
        .args(["new", "feat/child", "-m"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.head_commit(), main);
}

#[test]
fn changes_config_start_fresh_leaves_changes() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
    repo.git(&["config", "mate.new.changes", "startFresh"]);

    common::git_mate()
        .args(["new", "feat/fresh", "-w", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/fresh");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "base\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn changes_config_take_moves_changes_from_other_worktree() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    let linked_wt = create_linked_worktree(&repo, &wt_root, "feature/elsewhere");
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
    repo.git(&["config", "mate.new.changes", "take"]);

    common::git_mate()
        .args(["new", "feat/taken", "-w", "--from", "main"])
        .current_dir(&linked_wt)
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/taken");
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
fn changes_flag_overrides_config() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-m", "add tracked file"]);
    std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
    repo.git(&["config", "mate.new.changes", "startFresh"]);

    common::git_mate()
        .args([
            "new",
            "feat/taken",
            "-w",
            "--from",
            "main",
            "--take-changes",
        ])
        .current_dir(repo.path())
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/taken");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn invalid_changes_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.new.changes", "banana"]);

    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.new.changes"));
    assert!(!repo.branch_exists("feat/x"));
}

fn setup_local_config_file(repo: &common::RepoWithoutRemote) {
    std::fs::write(repo.path().join(".gitignore"), ".env.local\n").unwrap();
    repo.git(&["add", ".gitignore"]);
    repo.git(&["commit", "-m", "add gitignore"]);
    std::fs::write(repo.path().join(".env.local"), "SECRET=test").unwrap();
}

#[test]
fn no_copy_config_flag_skips_local_config() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    setup_local_config_file(&repo);

    common::git_mate()
        .args([
            "new",
            "feat/cfg",
            "-w",
            "--from",
            "main",
            "--no-copy-config",
        ])
        .current_dir(repo.path())
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/cfg");
    assert!(!wt_path.join(".env.local").exists());
}

#[test]
fn copy_config_false_skips_local_config_and_flag_overrides_it() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);
    setup_local_config_file(&repo);
    repo.git(&["config", "mate.new.copyConfig", "false"]);

    common::git_mate()
        .args(["new", "feat/no-cfg", "-w", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();
    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/no-cfg");
    assert!(!wt_path.join(".env.local").exists());

    common::git_mate()
        .args(["new", "feat/cfg", "-w", "--from", "main", "--copy-config"])
        .current_dir(repo.path())
        .assert()
        .success();
    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/cfg");
    assert_eq!(
        std::fs::read_to_string(wt_path.join(".env.local")).unwrap(),
        "SECRET=test"
    );
}

#[test]
fn invalid_copy_config_fails() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.new.copyConfig", "banana"]);

    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.new.copyConfig"));
}

fn new_reports_fetched(setup: &common::RepoWithRemote, args: &[&str]) -> bool {
    let output = common::git_mate()
        .args(["--json", "new"])
        .args(args)
        .current_dir(setup.local_path())
        .output()
        .unwrap();
    assert!(output.status.success());
    parse_single_json(&output.stdout)["data"]["fetched"]
        .as_bool()
        .unwrap()
}

#[test]
fn from_dot_skips_fetch() {
    let setup = common::RepoWithRemote::new();
    let tracking_before = setup.branch_tip("origin/main");
    setup.push_commit_to_remote("remote update");

    assert!(!new_reports_fetched(
        &setup,
        &["feat/x", "-m", "--from", "."]
    ));
    assert_eq!(
        setup.branch_tip("origin/main"),
        tracking_before,
        "origin/main should not have been updated"
    );
}

#[test]
fn from_local_branch_skips_fetch() {
    let setup = common::RepoWithRemote::new();
    assert!(!new_reports_fetched(
        &setup,
        &["feat/x", "-m", "--from", "main"]
    ));
}

#[test]
fn from_remote_ref_fetches() {
    let setup = common::RepoWithRemote::new();
    setup.push_commit_to_remote("remote update");
    assert!(new_reports_fetched(
        &setup,
        &["feat/x", "-m", "--from", "origin/main"]
    ));
    assert_ne!(setup.branch_tip("feat/x"), setup.branch_tip("main"));
}

#[test]
fn invalid_fetch_config_fails() {
    let setup = common::RepoWithRemote::new();
    setup.local_git(&["config", "mate.new.fetch", "banana"]);

    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("mate.new.fetch"));
}

#[test]
fn success_message_names_starting_point() {
    let setup = common::RepoWithRemote::new();
    common::git_mate()
        .args(["new", "feat/x", "-m"])
        .current_dir(setup.local_path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Created and switched to branch feat/x from origin/main",
        ));
}

#[test]
fn success_message_puts_worktree_path_on_its_own_line() {
    let repo = common::RepoWithoutRemote::new();
    let wt_root = TempDir::new().unwrap();
    configure_worktree_root(&repo, &wt_root);

    let output = common::git_mate()
        .args(["new", "feat/x", "-w", "--from", "main"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let wt_path = std::fs::canonicalize(expected_worktree_path(&repo, &wt_root, "feat/x")).unwrap();
    assert!(
        stderr.contains(&format!(
            "Created worktree for feat/x from main\n  in {}\n",
            wt_path.display()
        )),
        "unexpected output: {stderr}"
    );
}
