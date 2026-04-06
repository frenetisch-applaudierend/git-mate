use clap::Args;
use std::path::PathBuf;

use crate::shell_protocol::{interpreter, Message};

#[derive(Args)]
pub struct ProtocolArgs {
    /// File containing collected protocol messages.
    pub proto_file: PathBuf,

    #[command(flatten)]
    pub shell: ShellArgs,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct ShellArgs {
    /// Emit bash-compatible shell statements.
    #[arg(long)]
    pub bash: bool,

    /// Emit zsh-compatible shell statements.
    #[arg(long)]
    pub zsh: bool,

    /// Emit PowerShell-compatible shell statements.
    #[arg(long)]
    pub pwsh: bool,
}

pub fn run(args: ProtocolArgs) -> Result<(), String> {
    let content = std::fs::read_to_string(&args.proto_file)
        .map_err(|e| format!("Failed to read protocol file: {e}"))?;

    let emit: fn(&Message) -> String = if args.shell.bash {
        interpreter::interpret_bash
    } else if args.shell.zsh {
        interpreter::interpret_zsh
    } else {
        interpreter::interpret_pwsh
    };

    for line in content.lines() {
        if let Some(msg) = Message::parse(line) {
            println!("{}", emit(&msg));
        }
    }

    Ok(())
}
