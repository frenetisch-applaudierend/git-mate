mod common;

#[test]
fn zsh_init_script_is_valid_shell() {
    let output = common::git_mate().args(["init", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let script = String::from_utf8(output.stdout).unwrap();
    let status = std::process::Command::new("zsh")
        .args(["-n", "-c", &script])
        .status()
        .unwrap();
    assert!(status.success(), "zsh -n rejected the init script");
}

#[test]
fn bash_init_script_is_valid_shell() {
    let output = common::git_mate().args(["init", "bash"]).output().unwrap();
    assert!(output.status.success());
    let script = String::from_utf8(output.stdout).unwrap();
    let status = std::process::Command::new("bash")
        .args(["-n", "-c", &script])
        .status()
        .unwrap();
    assert!(status.success(), "bash -n rejected the init script");
}
