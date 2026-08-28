use std::path::PathBuf;

use clap::Args;
use clap::ValueHint;

use crate::cli::CommandHandler;
use crate::context::SnormContext;
use crate::ops::inspect;
use crate::ops::inspect::InspectOptions;
use crate::utils::errors::CliResult;

#[derive(Args)]
pub struct InspectCommand {
    /// Path to the .litematic file
    #[arg(value_hint = ValueHint::FilePath)]
    pub schematic: PathBuf,

    /// Path to the palette configuration
    #[arg(short, long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub palette: Option<PathBuf>
}

impl CommandHandler for InspectCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        let options = InspectOptions {
            schematic: self.schematic.clone(),
            palette: self.palette.clone()
        };

        Ok(inspect::inspect(context, &options)?)
    }
}
