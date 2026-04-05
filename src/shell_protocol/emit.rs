use std::io::Write;

use super::message::Message;

/// Emit a `CD` protocol message to the protocol file if shell integration is active
/// (i.e. `GIT_MATE_PROTO` is set to a file path).
pub fn emit_cd(path: &std::path::Path) {
    let Some(proto_path) = std::env::var_os("GIT_MATE_PROTO") else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(proto_path)
    {
        let _ = writeln!(file, "{}", Message::Cd(path.to_path_buf()).to_wire());
    }
}
