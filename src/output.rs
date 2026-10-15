use std::sync::atomic::{AtomicBool, Ordering};

static JSON_MODE: AtomicBool = AtomicBool::new(false);

/// Set once at startup from the global `--json` flag. Callers check
/// [`json_mode`] to suppress interactive/colored output and instead
/// accumulate data for a JSON envelope.
pub fn set_json_mode(enabled: bool) {
    JSON_MODE.store(enabled, Ordering::Relaxed);
}

pub fn json_mode() -> bool {
    JSON_MODE.load(Ordering::Relaxed)
}

pub fn success(msg: &str) {
    if json_mode() {
        return;
    }
    eprintln!("{} {}", console::style("✓").green().bold(), msg);
}

pub fn info(msg: &str) {
    if json_mode() {
        return;
    }
    eprintln!("{} {}", console::style("·").cyan(), msg);
}

/// Local config files were copied into a new worktree.
pub fn copied(msg: &str) {
    if json_mode() {
        return;
    }
    eprintln!("{} {}", console::style("⧉").blue(), msg);
}

/// Show `path` with the home directory abbreviated to `~`.
pub fn display_path(path: &std::path::Path) -> String {
    tilde_path(
        path,
        std::env::var_os("HOME")
            .as_deref()
            .map(std::path::Path::new),
    )
}

fn tilde_path(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    match home
        .filter(|home| !home.as_os_str().is_empty())
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// A command failure. Most commands only produce a message (via
/// `From<String>`); `code` and `data` let a command tell `--json` callers
/// more, e.g. that the operation partially succeeded.
pub struct CommandError {
    message: String,
    code: &'static str,
    data: Option<serde_json::Value>,
}

impl CommandError {
    pub fn with_data<T: serde::Serialize>(message: String, code: &'static str, data: T) -> Self {
        Self {
            message,
            code,
            data: Some(serde_json::to_value(data).expect("command data is always serializable")),
        }
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self {
            message,
            code: "error",
            data: None,
        }
    }
}

/// Report a command failure. In `--json` mode this emits the error envelope
/// (see [`emit_json_success`]) to stdout instead of colored text to stderr.
pub fn error(command: &str, err: &CommandError) {
    if json_mode() {
        emit_json_error(command, err);
        return;
    }
    eprintln!("{} {}", console::style("✗").red().bold(), err.message);
}

const SCHEMA_VERSION: u32 = 1;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonEnvelope<T: serde::Serialize> {
    ok: bool,
    command: String,
    schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonErrorBody>,
}

#[derive(serde::Serialize)]
struct JsonErrorBody {
    message: String,
    code: String,
}

/// Emit the success envelope `{"ok": true, "command", "schemaVersion", "data"}`
/// to stdout. `--verbose`'s raw git output goes to stderr, so this is the only
/// thing a `--json` invocation ever writes to stdout.
pub fn emit_json_success<T: serde::Serialize>(command: &str, data: T) {
    let envelope = JsonEnvelope {
        ok: true,
        command: command.to_string(),
        schema_version: SCHEMA_VERSION,
        data: Some(data),
        error: None,
    };
    println!(
        "{}",
        serde_json::to_string(&envelope).expect("envelope is always serializable")
    );
}

fn emit_json_error(command: &str, err: &CommandError) {
    let envelope = JsonEnvelope {
        ok: false,
        command: command.to_string(),
        schema_version: SCHEMA_VERSION,
        data: err.data.as_ref(),
        error: Some(JsonErrorBody {
            message: err.message.clone(),
            code: err.code.to_string(),
        }),
    };
    println!(
        "{}",
        serde_json::to_string(&envelope).expect("envelope is always serializable")
    );
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn tilde_path_abbreviates_home() {
        assert_eq!(
            super::tilde_path(Path::new("/home/me/src/repo"), Some(Path::new("/home/me"))),
            "~/src/repo"
        );
        assert_eq!(
            super::tilde_path(Path::new("/home/me"), Some(Path::new("/home/me"))),
            "~"
        );
    }

    #[test]
    fn tilde_path_leaves_other_paths() {
        assert_eq!(
            super::tilde_path(Path::new("/home/meow/repo"), Some(Path::new("/home/me"))),
            "/home/meow/repo"
        );
        assert_eq!(super::tilde_path(Path::new("/srv/repo"), None), "/srv/repo");
    }
}
