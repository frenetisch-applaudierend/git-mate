mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;

#[test]
fn zsh_output_includes_mate_wrapper() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("function git-mate()"));
}

#[test]
fn zsh_wrapper_uses_auto_cd() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_MATE_CD:"))
        .stdout(predicate::str::contains("builtin cd"));
}

#[test]
fn zsh_output_includes_completion() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("source <(COMPLETE=zsh command git-mate)"));
}

#[test]
fn bash_output_includes_mate_wrapper() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("git-mate()"));
}

#[test]
fn bash_wrapper_uses_auto_cd() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("_MATE_CD:"))
        .stdout(predicate::str::contains("builtin cd"));
}

#[test]
fn bash_output_includes_completion() {
    let repo = common::RepoWithoutRemote::new();
    common::git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("source <(COMPLETE=bash command git-mate)"));
}
