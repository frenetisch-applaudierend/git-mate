use super::message::{Message, PREFIX};

/// Returns true when the shell protocol is active (i.e. `mate` was invoked from a wrapper).
pub fn is_active() -> bool {
    std::env::var("GIT_MATE_SHELL").is_ok()
}

/// Emit a `CD` protocol message on stdout if the shell protocol is active.
pub fn emit_cd(path: &std::path::Path) {
    if is_active() {
        println!("{}{}", PREFIX, Message::Cd(path.to_path_buf()).to_wire());
    }
}
