use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::shell_protocol::{collect::Output, interpreter, Message};

#[derive(Args)]
pub struct ProtocolArgs {
    #[command(subcommand)]
    pub command: ProtocolCommand,
}

#[derive(Subcommand)]
pub enum ProtocolCommand {
    /// Filter stdin, writing protocol messages to a file and forwarding other output.
    Collect(CollectArgs),
    /// Read collected protocol messages and print the equivalent shell statements.
    Interpret(InterpretArgs),
}

#[derive(Args)]
pub struct CollectArgs {
    /// File to write collected protocol messages to.
    pub proto_file: PathBuf,

    /// Forward non-protocol output to stderr instead of stdout.
    #[arg(long, conflicts_with = "stdout")]
    pub stderr: bool,

    /// Forward non-protocol output to stdout (default).
    #[arg(long, conflicts_with = "stderr")]
    pub stdout: bool,
}

#[derive(Args)]
pub struct InterpretArgs {
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
}

pub fn run(args: ProtocolArgs) -> Result<(), String> {
    match args.command {
        ProtocolCommand::Collect(a) => {
            let output = if a.stderr { Output::Stderr } else { Output::Stdout };
            crate::shell_protocol::collect::run(&a.proto_file, output)
        }
        ProtocolCommand::Interpret(a) => interpret(a),
    }
}

fn interpret(args: InterpretArgs) -> Result<(), String> {
    let content = std::fs::read_to_string(&args.proto_file)
        .map_err(|e| format!("Failed to read protocol file: {e}"))?;

    let emit: fn(&Message) -> String = if args.shell.bash {
        interpreter::interpret_bash
    } else {
        interpreter::interpret_zsh
    };

    for line in content.lines() {
        if let Some(msg) = Message::parse(line) {
            println!("{}", emit(&msg));
        }
    }

    Ok(())
}
