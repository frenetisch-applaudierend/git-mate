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

pub fn run(args: InitArgs) -> Result<(), String> {
    match args.shell {
        Shell::Zsh => zsh::emit_init_script(),
        Shell::Bash => bash::emit_init_script(),
    }
    Ok(())
}
