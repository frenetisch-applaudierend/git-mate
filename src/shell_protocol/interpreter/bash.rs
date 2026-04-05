use crate::shell_protocol::Message;

/// Emit bash statements for a single protocol message.
pub fn interpret(msg: &Message) -> String {
    match msg {
        Message::Cd(path) => format!("builtin cd {:?}", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn cd_statement() {
        let msg = Message::Cd(PathBuf::from("/some/path"));
        assert_eq!(interpret(&msg), r#"builtin cd "/some/path""#);
    }
}
