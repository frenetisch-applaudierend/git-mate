use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
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

fn build_cli() -> clap::Command {
    let mut cmd = Cli::command();
    for name in &["checkout", "finish", "init", "new", "sync"] {
        if let Some(alias) = crate::git::config::read_string(&format!("mate.{name}.shorthand")) {
            let alias: &'static str = Box::leak(alias.into_boxed_str());
            cmd = cmd.mut_subcommand(name, |c| c.alias(alias));
        }
    }
    cmd
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
