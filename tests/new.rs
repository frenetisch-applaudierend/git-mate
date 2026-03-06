mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn git_mate() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("git-mate"))
}

#[test]
fn explicit_from() {
    let repo = common::TestRepo::new();
    git_mate()
        .args(["new", "feat/x", "--from", "main"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn detects_main() {
    let repo = common::TestRepo::new();
    git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .success();
    assert_eq!(repo.current_branch(), "feat/x");
}

#[test]
fn detects_master() {
    let repo = common::TestRepo::new();
    repo.git(&["branch", "-m", "main", "master"]);
    git_mate()
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
    git_mate()
        .args(["new", "feat/x"])
        .current_dir(setup.local_path())
        .assert()
        .success();
    assert_eq!(setup.local_current_branch(), "feat/x");
}

#[test]
fn no_default_branch() {
    let repo = common::TestRepo::new();
    // Detach HEAD then delete main so no fallback branch exists (no remote either).
    let sha = repo.head_commit();
    repo.git(&["checkout", "--detach", &sha]);
    repo.git(&["branch", "-D", "main"]);
    git_mate()
        .args(["new", "feat/x"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--from"));
}
