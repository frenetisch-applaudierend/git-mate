use std::path::PathBuf;

/// A parsed shell protocol message.
#[derive(Debug, PartialEq)]
pub enum Message {
    /// Ask the shell to change its working directory.
    Cd(PathBuf),
}

impl Message {
    /// Parse a bare message string (prefix already stripped), e.g. `"CD:/some/path"`.
    pub fn parse(s: &str) -> Option<Self> {
        let (tag, value) = s.split_once(':')?;
        match tag {
            "CD" => Some(Message::Cd(PathBuf::from(value))),
            _ => None,
        }
    }

    /// Serialize to the bare message form written to the protocol file, e.g. `"CD:/some/path"`.
    pub fn to_wire(&self) -> String {
        match self {
            Message::Cd(path) => format!("CD:{}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_cd() {
        let msg = Message::Cd(PathBuf::from("/some/path"));
        let wire = msg.to_wire();
        assert_eq!(wire, "CD:/some/path");
        assert_eq!(Message::parse(&wire), Some(Message::Cd(PathBuf::from("/some/path"))));
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert_eq!(Message::parse("UNKNOWN:foo"), None);
    }

    #[test]
    fn parse_missing_colon_returns_none() {
        assert_eq!(Message::parse("CD"), None);
    }
}
