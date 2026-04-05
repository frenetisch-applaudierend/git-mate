static VERBOSE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

pub fn set_verbose(v: bool) {
    let _ = VERBOSE.set(v);
}

pub(super) fn is_verbose() -> bool {
    *VERBOSE.get().unwrap_or(&false)
}

pub(super) fn run_output(args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .stderr(if is_verbose() {
            std::process::Stdio::inherit()
        } else {
            std::process::Stdio::piped()
        })
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else if is_verbose() {
        Err(format!("`git {}` failed", args.join(" ")))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Err(format!("`git {}` failed", args.join(" ")))
        } else {
            Err(format!("`git {}` failed: {stderr}", args.join(" ")))
        }
    }
}

pub(super) fn run(args: &[&str]) -> Result<(), String> {
    if is_verbose() {
        let status = std::process::Command::new("git")
            .args(args)
            .status()
            .map_err(|e| format!("failed to run git: {e}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err(format!("`git {}` failed", args.join(" ")))
        };
    }
    let output = std::process::Command::new("git")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Err(format!("`git {}` failed", args.join(" ")))
        } else {
            Err(format!("`git {}` failed: {stderr}", args.join(" ")))
        }
    }
}
