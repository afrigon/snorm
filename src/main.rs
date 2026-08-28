mod cli;
mod context;
mod core;
mod ops;
mod utils;

use std::process::exit;

use clap::Parser;
use tracing::debug;

use crate::cli::Cli;
use crate::cli::CommandHandler;
use crate::cli::commands::CliCommand;
use crate::cli::commands::data::DataSubcommand;
use crate::cli::commands::region::RegionSubcommand;
use crate::context::SnormContext;
use crate::utils::errors::CliError;
use crate::utils::errors::CliResult;
use crate::utils::errors::InternalError;
use crate::utils::shell::Shell;
use crate::utils::verbosity::Verbosity;

fn main() {
    clap_complete::CompleteEnv::with_factory(|| {
        use clap::CommandFactory;

        Cli::command()
    })
    .complete();

    let mut context = match SnormContext::default() {
        Ok(context) => context,
        Err(e) => {
            let mut shell = Shell::new();

            exit_with_error(e.into(), &mut shell)
        }
    };

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => exit_with_error(e.into(), &mut context.shell())
    };

    let verbose = cli.globals.verbose;
    let quiet = cli.globals.quiet;

    let verbosity = if quiet {
        Verbosity::Quiet
    } else {
        match verbose {
            0 => Verbosity::Regular,
            1 => Verbosity::Verbose,
            _ => Verbosity::VeryVerbose
        }
    };

    let log_level = if quiet {
        tracing::Level::ERROR
    } else {
        match verbose {
            0 => tracing::Level::WARN,
            1 => tracing::Level::INFO,
            2 => tracing::Level::DEBUG,
            _ => tracing::Level::TRACE
        }
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_writer(std::io::stderr)
        .init();

    context.log_level = log_level;
    context.shell().set_verbosity(verbosity);

    let color_choice = cli.globals.color;
    context.shell().set_color_choice(color_choice);

    if let Err(e) = run(&cli, &mut context) {
        exit_with_error(e, &mut context.shell())
    };
}

fn run(cli: &Cli, context: &mut SnormContext) -> CliResult {
    match &cli.command {
        CliCommand::Normalize(command) => command.handle(context),
        CliCommand::Inspect(command) => command.handle(context),
        CliCommand::Region(command) => match &command.command {
            RegionSubcommand::List(command) => command.handle(context),
            RegionSubcommand::Rename(command) => command.handle(context)
        },
        CliCommand::Data(command) => match &command.command {
            DataSubcommand::Extract(command) => command.handle(context),
            DataSubcommand::Status(command) => command.handle(context),
            DataSubcommand::Clean(command) => command.handle(context)
        },
        CliCommand::Completions(command) => command.handle(context)
    }
}

fn exit_with_error(error: CliError, shell: &mut Shell) -> ! {
    debug!("exit_with_error; error={:?}", error);

    if let Some(ref err) = error.error
        && let Some(clap_err) = err.downcast_ref::<clap::Error>()
    {
        let exit_code = if clap_err.use_stderr() { 1 } else { 0 };
        let _ = clap_err.print();

        exit(exit_code)
    }

    let CliError { error, exit_code } = error;

    if let Some(error) = error {
        for (i, error) in error.chain().enumerate() {
            if i == 0 {
                drop(shell.error(error));
            } else {
                let lines: String = error
                    .to_string()
                    .lines()
                    .map(|line| {
                        if line.is_empty() {
                            String::from("\n")
                        } else {
                            format!("  {}\n", line)
                        }
                    })
                    .collect();

                drop(writeln!(shell.err(), "\nCaused by:"));
                drop(writeln!(shell.err(), "{}", lines));
            }
        }

        if error
            .chain()
            .any(|e| e.downcast_ref::<InternalError>().is_some())
        {
            drop(shell.note("this is an unexpected snorm internal error"));

            drop(shell.note(format!(
                "{} {}",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            )));
        }
    }

    exit(exit_code)
}
