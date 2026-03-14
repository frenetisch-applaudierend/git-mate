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

#[test]
fn zsh_output_includes_completion_function() {
    let repo = common::RepoWithoutRemote::new();
    let output = git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("_git_mate_complete"),
        "expected '_git_mate_complete' in zsh output, got: {stdout:?}"
    );
    assert!(
        stdout.contains("compdef"),
        "expected 'compdef' in zsh output, got: {stdout:?}"
    );
}

#[test]
fn bash_output_includes_completion_function() {
    let repo = common::RepoWithoutRemote::new();
    let output = git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("_git_mate_complete"),
        "expected '_git_mate_complete' in bash output, got: {stdout:?}"
    );
    assert!(
        stdout.contains("complete -F"),
        "expected 'complete -F' in bash output, got: {stdout:?}"
    );
}

#[test]
fn completion_registered_for_binary_and_wrapper() {
    let repo = common::RepoWithoutRemote::new();
    let output = git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("compdef _git_mate_complete git-mate"),
        "expected 'compdef _git_mate_complete git-mate' in output, got: {stdout:?}"
    );
    assert!(
        stdout.contains("compdef _git_mate_complete gm"),
        "expected 'compdef _git_mate_complete gm' in output, got: {stdout:?}"
    );
}

#[test]
fn completion_uses_custom_wrapper_name() {
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
        stdout.contains("compdef _git_mate_complete g"),
        "expected 'compdef _git_mate_complete g' in output, got: {stdout:?}"
    );
}

#[test]
fn zsh_completion_strips_command_name_from_words() {
    let repo = common::RepoWithoutRemote::new();
    let output = git_mate()
        .args(["init", "zsh"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("${words[@]:1}"),
        "expected '${{words[@]:1}}' (command name stripped) in zsh output, got: {stdout:?}"
    );
    assert!(
        !stdout.contains("\"${words[@]}\""),
        "unexpected bare '\"${{words[@]}}\"' (includes command name) in zsh output: {stdout:?}"
    );
}

#[test]
fn bash_completion_clears_compreply_before_filling() {
    let repo = common::RepoWithoutRemote::new();
    let output = git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("COMPREPLY=()"),
        "expected 'COMPREPLY=()' (clear before fill) in bash output, got: {stdout:?}"
    );
}

#[test]
fn bash_completion_strips_command_name_from_comp_words() {
    let repo = common::RepoWithoutRemote::new();
    let output = git_mate()
        .args(["init", "bash"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("${COMP_WORDS[@]:1}"),
        "expected '${{COMP_WORDS[@]:1}}' (command name stripped) in bash output, got: {stdout:?}"
    );
}
