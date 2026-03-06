use clap::{Parser, Subcommand};
mod cmd;

#[derive(Parser)]
#[command(name = "git-mate")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Sync(cmd::sync::SyncArgs),
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Sync(args) => cmd::sync::run(args),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
