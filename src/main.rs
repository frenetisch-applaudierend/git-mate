use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::CompleteEnv;
mod cmd;
mod complete;
mod git;
mod output;

#[derive(Parser)]
#[command(name = "mate")]
struct Cli {
    #[arg(long, global = true, help = "Show raw git output")]
    verbose: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Switch to an existing branch
    Checkout(cmd::checkout::CheckoutArgs),
    /// Merge branch into main and clean up
    Finish(cmd::finish::FinishArgs),
    /// Set up shell integration
    Init(cmd::init::InitArgs),
    /// Create and switch to a new branch
    New(cmd::new::NewArgs),
    /// Fetch and merge the latest changes
    Sync(cmd::sync::SyncArgs),
}

fn build_cli() -> clap::Command {
    Cli::command()
}

fn main() {
    CompleteEnv::with_factory(build_cli).complete();

    let matches = build_cli().get_matches();
    crate::git::set_verbose(*matches.get_one::<bool>("verbose").unwrap_or(&false));
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    let result = match cli.command {
        Commands::Checkout(args) => cmd::checkout::run(args),
        Commands::Finish(args) => cmd::finish::run(args),
        Commands::Init(args) => cmd::init::run(args),
        Commands::New(args) => cmd::new::run(args),
        Commands::Sync(args) => cmd::sync::run(args),
    };

    if let Err(e) = result {
        crate::output::error(&e);
        std::process::exit(1);
    }
}
