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
pub enum ShellIntegration {
    /// Never emit the git() wrapper.
    False,
    /// Emit the wrapper only when `git` is not already a shell function; warn otherwise.
    True,
    /// Always emit the wrapper unconditionally.
    Force,
}

pub fn run(args: InitArgs) -> Result<(), String> {
    let integration = match crate::git::config::read_string("mate.shellIntegration")
        .as_deref()
    {
        Some("force") => ShellIntegration::Force,
        Some("false") | Some("off") | Some("0") | Some("no") => ShellIntegration::False,
        Some("true") | Some("on") | Some("1") | Some("yes") => ShellIntegration::True,
        _ => ShellIntegration::False,
    };
    match args.shell {
        Shell::Zsh => zsh::emit_init_script(integration),
        Shell::Bash => bash::emit_init_script(integration),
    }
    Ok(())
}
