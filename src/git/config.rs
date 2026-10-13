pub fn read_string(key: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["config", "--get", key])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Read a boolean key, accepting git's spellings (`true`/`yes`/`on`/`1` and
/// `false`/`no`/`off`/`0`). Returns `None` when the key is unset.
pub fn read_bool(key: &str) -> Result<Option<bool>, String> {
    let Some(value) = read_string(key) else {
        return Ok(None);
    };
    match value.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Ok(Some(true)),
        "false" | "no" | "off" | "0" => Ok(Some(false)),
        _ => Err(format!(
            "invalid value for {key}: {value:?}; expected a boolean"
        )),
    }
}
