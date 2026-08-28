use std::path::PathBuf;

use clap::ArgGroup;
use clap::Args;
use clap::Subcommand;
use clap::ValueHint;
use clap_complete::engine::ArgValueCompleter;

use crate::cli::CommandHandler;
use crate::cli::complete;
use crate::context::SnormContext;
use crate::ops::OutputTarget;
use crate::ops::regions;
use crate::ops::regions::RegionListOptions;
use crate::ops::regions::RegionRenameOptions;
use crate::utils::errors::CliResult;

#[derive(Args)]
pub struct RegionCommand {
    #[command(subcommand)]
    pub command: RegionSubcommand
}

#[derive(Subcommand)]
pub enum RegionSubcommand {
    /// List the regions of a schematic
    List(RegionListCommand),

    /// Rename regions of a schematic
    Rename(RegionRenameCommand)
}

#[derive(Args)]
pub struct RegionListCommand {
    /// Path to the .litematic file
    #[arg(value_hint = ValueHint::FilePath)]
    pub schematic: PathBuf
}

impl CommandHandler for RegionListCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        let options = RegionListOptions {
            schematic: self.schematic.clone()
        };

        Ok(regions::list(context, &options)?)
    }
}

#[derive(Args)]
#[command(group = ArgGroup::new("output").required(true).args(["out", "in_place"]))]
pub struct RegionRenameCommand {
    /// Path to the .litematic file
    #[arg(value_hint = ValueHint::FilePath)]
    pub schematic: PathBuf,

    /// Renames as OLD=NEW pairs
    #[arg(
        required = true,
        value_name = "OLD=NEW",
        value_parser = parse_rename,
        add = ArgValueCompleter::new(complete::rename_pair_completer)
    )]
    pub renames: Vec<(String, String)>,

    /// Path of the schematic to write
    #[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub out: Option<PathBuf>,

    /// Overwrite the input schematic
    #[arg(long)]
    pub in_place: bool
}

pub fn parse_rename(input: &str) -> Result<(String, String), String> {
    let Some((old, new)) = input.split_once('=') else {
        return Err(String::from("expected 'OLD=NEW'"));
    };

    if old.is_empty() || new.is_empty() {
        return Err(String::from("expected 'OLD=NEW'"));
    }

    Ok((String::from(old), String::from(new)))
}

impl CommandHandler for RegionRenameCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        let output = match &self.out {
            Some(path) => OutputTarget::Path(path.clone()),
            None => OutputTarget::InPlace
        };

        let options = RegionRenameOptions {
            schematic: self.schematic.clone(),
            renames: self.renames.clone(),
            output
        };

        Ok(regions::rename(context, &options)?)
    }
}
