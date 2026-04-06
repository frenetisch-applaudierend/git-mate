use crate::shell_protocol::Message;

/// Emit PowerShell statements for a single protocol message.
pub fn interpret(msg: &Message) -> String {
    match msg {
        Message::Cd(path) => {
            let escaped = path.to_string_lossy().replace('\'', "''");
            format!("Set-Location -LiteralPath '{}'", escaped)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn cd_statement() {
        let msg = Message::Cd(PathBuf::from("/some/path"));
        assert_eq!(interpret(&msg), "Set-Location -LiteralPath '/some/path'");
    }

    #[test]
    fn cd_statement_with_single_quote() {
        let msg = Message::Cd(PathBuf::from("/some/path with 'quotes'"));
        assert_eq!(
            interpret(&msg),
            "Set-Location -LiteralPath '/some/path with ''quotes'''"
        );
    }
}
