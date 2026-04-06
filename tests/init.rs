mod common;

/// Run a shell's syntax-check on `script`. Silently skips if the shell binary is not installed.
fn assert_valid_syntax(shell: &str, check_args: &[&str], script: &str) {
    let result = std::process::Command::new(shell)
        .args(check_args)
        .arg(script)
        .status();

    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Skipping {shell} syntax check: binary not found");
        }
        Err(e) => panic!("Failed to invoke {shell}: {e}"),
        Ok(status) => assert!(status.success(), "{shell} rejected the init script"),
    }
}

#[test]
fn bash_init_script_is_valid_shell() {
    let output = common::git_mate().args(["init", "bash"]).output().unwrap();
    assert!(output.status.success());
    let script = String::from_utf8(output.stdout).unwrap();
    assert_valid_syntax("bash", &["-n", "-c"], &script);
}

#[test]
fn zsh_init_script_is_valid_shell() {
    let output = common::git_mate().args(["init", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let script = String::from_utf8(output.stdout).unwrap();
    assert_valid_syntax("zsh", &["-n", "-c"], &script);
}

#[test]
fn pwsh_init_script_is_valid_shell() {
    let output = common::git_mate().args(["init", "pwsh"]).output().unwrap();
    assert!(output.status.success());
    let script = String::from_utf8(output.stdout).unwrap();

    // Write to a temp .ps1 file so PowerShell's AST parser can check syntax
    // without executing the script.
    let tmp = tempfile::Builder::new().suffix(".ps1").tempfile().unwrap();
    std::fs::write(tmp.path(), &script).unwrap();
    let path = tmp.path().to_string_lossy().replace('\'', "''");
    let parse_cmd = format!(
        "$e = @(); $null = [System.Management.Automation.Language.Parser]::ParseFile('{path}', [ref]$null, [ref]$e); exit $e.Count"
    );

    assert_valid_syntax("pwsh", &["-NoProfile", "-Command"], &parse_cmd);
}
