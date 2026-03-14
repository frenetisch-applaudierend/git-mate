use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;
mod cmd;
mod git;

#[derive(Parser)]
#[command(name = "git-mate")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Checkout(cmd::checkout::CheckoutArgs),
    Finish(cmd::finish::FinishArgs),
    Init(cmd::init::InitArgs),
    New(cmd::new::NewArgs),
    Sync(cmd::sync::SyncArgs),
}

fn main() {
    CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Checkout(args) => cmd::checkout::run(args),
        Commands::Finish(args) => cmd::finish::run(args),
        Commands::Init(args) => cmd::init::run(args),
        Commands::New(args) => cmd::new::run(args),
        Commands::Sync(args) => cmd::sync::run(args),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
