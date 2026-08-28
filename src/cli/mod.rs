pub mod commands;
pub mod complete;
pub mod globals;
pub mod styles;

use clap::ColorChoice;
use clap::Parser;

use crate::cli::commands::CliCommand;
use crate::cli::globals::GlobalOptions;
use crate::context::SnormContext;
use crate::utils::errors::CliResult;

#[derive(Parser)]
#[command(version, about, name = "snorm", styles = styles::styles(), color = ColorChoice::Auto)]
pub struct Cli {
    #[command(flatten)]
    pub globals: GlobalOptions,

    #[command(subcommand)]
    pub command: CliCommand
}

pub trait CommandHandler {
    fn handle(&self, context: &mut SnormContext) -> CliResult;
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
