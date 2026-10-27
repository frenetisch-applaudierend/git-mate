use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::CompleteEnv;
mod cmd;
mod complete;
mod cwd;
mod fs;
mod git;
mod output;
mod shell_protocol;

#[derive(Parser)]
#[command(name = "git-mate")]
struct Cli {
    #[arg(long, global = true, help = "Show raw git output")]
    verbose: bool,
    #[arg(
        long,
        global = true,
        help = "Emit machine-readable JSON on stdout instead of interactive text"
    )]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Switch to an existing branch
    #[command(alias = "co")]
    Checkout(cmd::checkout::CheckoutArgs),
    /// Leave a branch and clean up its worktree
    Finish(cmd::finish::FinishArgs),
    /// Set up shell integration
    Init(cmd::init::InitArgs),
    /// Create and switch to a new branch
    New(cmd::new::NewArgs),
    Sync(cmd::sync::SyncArgs),
    /// Internal: interpret shell protocol messages
    #[command(name = "_protocol", hide = true)]
    Protocol(cmd::protocol::ProtocolArgs),
}

fn build_cli() -> clap::Command {
    Cli::command()
}

fn main() {
    CompleteEnv::with_factory(build_cli).complete();

    let matches = build_cli().get_matches();
    crate::git::set_verbose(*matches.get_one::<bool>("verbose").unwrap_or(&false));
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    crate::output::set_json_mode(cli.json);

    let command_name = command_name(&cli.command);

    let recovered = match cli.command {
        Commands::Init(_) | Commands::Protocol(_) => Ok(()),
        _ => crate::cwd::recover(),
    };
    if let Err(e) = recovered {
        crate::output::error(command_name, &e.into());
        std::process::exit(1);
    }

    let result: Result<(), crate::output::CommandError> = match cli.command {
        // `init` prints a shell script to eval, and `_protocol` is internal:
        // neither has a result to describe in JSON.
        Commands::Init(_) | Commands::Protocol(_) if cli.json => {
            Err(crate::output::CommandError::new(
                format!("--json is not supported for {command_name}"),
                "unsupported",
            ))
        }
        Commands::Checkout(args) => cmd::checkout::run(args).map_err(Into::into),
        Commands::Finish(args) => cmd::finish::run(args).map_err(Into::into),
        Commands::Init(args) => cmd::init::run(args).map_err(Into::into),
        Commands::New(args) => cmd::new::run(args),
        Commands::Sync(args) => cmd::sync::run(args).map_err(Into::into),
        Commands::Protocol(args) => cmd::protocol::run(args).map_err(Into::into),
    };

    if let Err(e) = result {
        crate::output::error(command_name, &e);
        std::process::exit(1);
    }
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Checkout(_) => "checkout",
        Commands::Finish(_) => "finish",
        Commands::Init(_) => "init",
        Commands::New(_) => "new",
        Commands::Sync(_) => "sync",
        Commands::Protocol(_) => "_protocol",
    }
}
