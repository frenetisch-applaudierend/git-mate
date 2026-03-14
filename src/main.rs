use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::CompleteEnv;
mod cmd;
mod complete;
mod git;
mod output;

#[derive(Parser)]
#[command(name = "git-mate")]
struct Cli {
    #[arg(long, global = true, help = "Show raw git output")]
    verbose: bool,
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
    crate::git::set_verbose(cli.verbose);
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
