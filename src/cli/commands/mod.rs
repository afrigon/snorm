pub mod completions;
pub mod data;
pub mod inspect;
pub mod normalize;
pub mod region;

use clap::Subcommand;

use crate::cli::commands::completions::CompletionsCommand;
use crate::cli::commands::data::DataCommand;
use crate::cli::commands::inspect::InspectCommand;
use crate::cli::commands::normalize::NormalizeCommand;
use crate::cli::commands::region::RegionCommand;

#[derive(Subcommand)]
pub enum CliCommand {
    /// Replace blocks to match a palette, preserving block states
    Normalize(NormalizeCommand),

    /// Show schematic metadata, regions and block palettes
    Inspect(InspectCommand),

    /// List or rename schematic regions
    Region(RegionCommand),

    /// Manage extracted minecraft block data
    Data(DataCommand),

    /// Print the shell line that enables tab completion
    Completions(CompletionsCommand)
}
