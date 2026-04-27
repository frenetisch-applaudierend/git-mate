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
    setup.local_git(&["config", "mate.fetch", "false"]);

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
    setup.local_git(&["config", "mate.fetch", value]);
    common::git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_eq!(
        setup.branch_tip("feat/x"),
        old_head,
        "mate.fetch={value} should skip fetch"
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
        .args([
            "new",
            "feat/x",
            "--linked-worktree",
            "--from",
            "main",
            "--stash",
        ])
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
fn worktree_mode_requires_decision_for_dirty_starting_branch() {
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
        .args(["new", "feat/dirty", "--linked-worktree", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--stash"))
        .stderr(predicate::str::contains("--ignore"));
}

#[test]
fn worktree_mode_ignore_starts_fresh_from_dirty_starting_branch() {
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
            "--ignore",
        ])
        .current_dir(repo.path())
        .assert()
        .success();

    let wt_path = expected_worktree_path(&repo, &wt_root, "feat/fresh");
    assert_eq!(
        std::fs::read_to_string(wt_path.join("tracked.txt")).unwrap(),
        "base\n"
    );
    assert!(
        !wt_path.join(".env.local").exists(),
        "ignored local config should not be copied with --ignore"
    );
}

#[test]
fn worktree_mode_stash_carries_dirty_starting_branch_changes() {
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
            "--stash",
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
fn main_worktree_mode_rejects_ignore_when_destination_is_dirty() {
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
            "--ignore",
        ])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot use --ignore"));
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
fn main_worktree_mode_stash_moves_changes_from_dirty_linked_starting_branch() {
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
            "--stash",
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
