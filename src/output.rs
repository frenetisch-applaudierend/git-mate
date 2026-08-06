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

/// Report a command failure. In `--json` mode this emits the error envelope
/// (see [`emit_json_success`]) to stdout instead of colored text to stderr.
pub fn error(command: &str, msg: &str) {
    if json_mode() {
        emit_json_error(command, msg);
        return;
    }
    eprintln!("{} {}", console::style("✗").red().bold(), msg);
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

fn emit_json_error(command: &str, msg: &str) {
    let envelope: JsonEnvelope<()> = JsonEnvelope {
        ok: false,
        command: command.to_string(),
        schema_version: SCHEMA_VERSION,
        data: None,
        error: Some(JsonErrorBody {
            message: msg.to_string(),
            code: "error".to_string(),
        }),
    };
    println!(
        "{}",
        serde_json::to_string(&envelope).expect("envelope is always serializable")
    );
}
