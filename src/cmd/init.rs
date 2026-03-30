mod bash;
mod zsh;

#[derive(clap::ValueEnum, Clone)]
pub enum Shell {
    Zsh,
    Bash,
}

#[derive(clap::Args)]
pub struct InitArgs {
    pub shell: Shell,
}

#[derive(Clone, Copy)]
pub enum GitAutoCdMode {
    Off,
    Always,
    IfSafe,
}

impl GitAutoCdMode {
    fn from_config_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" => Some(Self::Always),
            "false" | "no" | "off" | "0" => Some(Self::Off),
            "if-safe" => Some(Self::IfSafe),
            _ => None,
        }
    }
}

pub fn run(args: InitArgs) -> Result<(), String> {
    let git_auto_cd = match crate::git::config::read_string("mate.gitAutoCd") {
        Some(value) => GitAutoCdMode::from_config_value(&value).ok_or_else(|| {
            format!(
                "invalid value for mate.gitAutoCd: {value:?}; expected boolean true/false, yes/no, on/off, 1/0, or 'if-safe'"
            )
        })?,
        None => GitAutoCdMode::Off,
    };
    match args.shell {
        Shell::Zsh => zsh::emit_init_script(git_auto_cd),
        Shell::Bash => bash::emit_init_script(git_auto_cd),
    }
    Ok(())
}
