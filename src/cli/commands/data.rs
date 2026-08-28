use std::path::PathBuf;

use clap::Args;
use clap::Subcommand;

use crate::cli::CommandHandler;
use crate::context::SnormContext;
use crate::ops::data_status;
use crate::ops::data_status::DataCleanOptions;
use crate::ops::extract;
use crate::ops::extract::DataExtractOptions;
use crate::utils::errors::CliResult;

#[derive(Args)]
pub struct DataCommand {
    #[command(subcommand)]
    pub command: DataSubcommand
}

#[derive(Subcommand)]
pub enum DataSubcommand {
    /// Download a minecraft server jar and extract block data from it
    Extract(DataExtractCommand),

    /// List the extracted minecraft versions
    Status(DataStatusCommand),

    /// Remove extracted minecraft data
    Clean(DataCleanCommand)
}

#[derive(Args)]
pub struct DataExtractCommand {
    /// Minecraft version to extract (defaults to the latest release)
    #[arg(long, value_name = "ID")]
    pub mc_version: Option<String>,

    /// Extract from a local jar instead of downloading one
    #[arg(long, value_name = "PATH", conflicts_with = "mc_version")]
    pub jar: Option<PathBuf>,

    /// Extract again even if this version is already cached
    #[arg(long)]
    pub force: bool
}

impl CommandHandler for DataExtractCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        let options = DataExtractOptions {
            mc_version: self.mc_version.clone(),
            jar: self.jar.clone(),
            force: self.force
        };

        Ok(extract::extract(context, &options)?)
    }
}

#[derive(Args)]
pub struct DataStatusCommand {}

impl CommandHandler for DataStatusCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        Ok(data_status::status(context)?)
    }
}

#[derive(Args)]
pub struct DataCleanCommand {
    /// Only remove the data of this minecraft version
    #[arg(long, value_name = "ID")]
    pub mc_version: Option<String>
}

impl CommandHandler for DataCleanCommand {
    fn handle(&self, context: &mut SnormContext) -> CliResult {
        let options = DataCleanOptions {
            mc_version: self.mc_version.clone()
        };

        Ok(data_status::clean(context, &options)?)
    }
}
