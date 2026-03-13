mod common;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

fn git_mate() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("git-mate"))
}

#[test]
fn default_wrapper_name_is_gm() {
    let repo = common::RepoWithoutRemote::new();
    let output = git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("function gm()"),
        "expected 'function gm()' in output, got: {stdout:?}"
    );
}

#[test]
fn config_overrides_wrapper_name() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.wrapperName", "g"]);
    let output = git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("function g()"),
        "expected 'function g()' in output, got: {stdout:?}"
    );
}

#[test]
fn invalid_wrapper_name_is_rejected() {
    let repo = common::RepoWithoutRemote::new();
    repo.git(&["config", "mate.wrapperName", "bad name!"]);
    git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid wrapper name"));
}
