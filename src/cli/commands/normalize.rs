use std::path::PathBuf;

use clap::Args;
use clap::ValueHint;
use clap_complete::engine::ArgValueCompleter;

use crate::cli::CommandHandler;
use crate::cli::commands::region::parse_rename;
use crate::cli::complete;
use crate::context::SnormContext;
use crate::core::block::BlockId;
use crate::core::overrides::OverrideSpec;
use crate::ops::OutputTarget;
use crate::ops::normalize;
use crate::ops::normalize::NormalizeOptions;
use crate::utils::errors::CliResult;

#[derive(Args)]
pub struct NormalizeCommand {
    /// Path to the .litematic file
    #[arg(value_hint = ValueHint::FilePath)]
    pub schematic: PathBuf,

    /// Path to the palette configuration
    #[arg(short, long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    pub palette: Option<PathBuf>,

    /// Replacement override, e.g. -o minecraft:dirt=minecraft:stone;
    /// an empty target (minecraft:dirt=) keeps the block unchanged
    #[arg(
        short = 'o',
        long = "override",
        value_name = "SRC[,SRC]=TARGET",
        value_parser = parse_override,
        add = ArgValueCompleter::new(complete::override_spec_completer)
    )]
    pub overrides: Vec<OverrideSpec>,

    /// Block to normalize as solid, bypassing the interactive selection
    /// (repeatable)
    #[arg(
        long = "solid",
        value_name = "BLOCK",
        value_parser = parse_block_id,
        add = ArgValueCompleter::new(complete::schematic_block_completer)
    )]
    pub solids: Vec<BlockId>,

    /// Path of the schematic to write (default: <input>.normalized.litematic)
    #[arg(
        long,
        value_name = "PATH",
        conflicts_with = "in_place",
        value_hint = ValueHint::FilePath
    )]
    pub out: Option<PathBuf>,

    /// Overwrite the input schematic
    #[arg(long)]
    pub in_place: bool,

    /// Only normalize this region (repeatable)
    #[arg(
        long = "region",
        value_name = "NAME",
        add = ArgValueCompleter::new(complete::region_name_completer)
    )]
    pub regions: Vec<String>,

    /// Rename a region while normalizing (repeatable)
    #[arg(
        long = "rename-region",
        value_name = "OLD=NEW",
        value_parser = parse_rename,
        add = ArgValueCompleter::new(complete::rename_pair_completer)
    )]
    pub renames: Vec<(String, String)>,

    /// Report the changes without writing a file
    #[arg(long)]
    pub dry_run: bool,

    /// Use the block data of this cached minecraft version
    #[arg(long, value_name = "ID")]
    pub mc_version: Option<String>
}

fn parse_override(input: &str) -> Result<OverrideSpec, String> {
    OverrideSpec::parse(input).map_err(|e| e.to_string())
}

fn parse_block_id(input: &str) -> Result<BlockId, String> {
    BlockId::parse(input).map_err(|e| e.to_string())
}

impl CommandHandler for NormalizeCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        let output = match (&self.out, self.in_place) {
            (_, true) => OutputTarget::InPlace,
            (Some(path), false) => OutputTarget::Path(path.clone()),
            (None, false) => {
                let stem = self
                    .schematic
                    .file_stem()
                    .map(|stem| stem.to_string_lossy())
                    .unwrap_or_default();

                OutputTarget::Path(
                    self.schematic
                        .with_file_name(format!("{stem}.normalized.litematic"))
                )
            }
        };

        let options = NormalizeOptions {
            schematic: self.schematic.clone(),
            palette: self.palette.clone(),
            overrides: self.overrides.clone(),
            solids: self.solids.clone(),
            regions: self.regions.clone(),
            renames: self.renames.clone(),
            output,
            dry_run: self.dry_run,
            mc_version: self.mc_version.clone()
        };

        Ok(normalize::normalize(context, &options)?)
    }
}
