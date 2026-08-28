use anyhow::bail;

use crate::context::SnormContext;
use crate::core::mcdata;
use crate::utils::errors::SnormResult;

pub fn status(context: &mut SnormContext) -> SnormResult<()> {
    let versions = mcdata::cached_versions()?;

    if versions.is_empty() {
        context
            .shell()
            .note("no minecraft data extracted; run 'snorm data extract'")?;

        return Ok(());
    }

    let mut shell = context.shell();
    let out = shell.out();

    for version in versions {
        writeln!(
            out,
            "minecraft {}  data version {}  {}",
            version.manifest.id,
            version.manifest.data_version,
            version.path.display()
        )?;
    }

    Ok(())
}

pub struct DataCleanOptions {
    pub mc_version: Option<String>
}

pub fn clean(context: &mut SnormContext, options: &DataCleanOptions) -> SnormResult<()> {
    let versions = mcdata::cached_versions()?;

    let selected: Vec<_> = match &options.mc_version {
        Some(id) => {
            let matching: Vec<_> = versions
                .into_iter()
                .filter(|v| v.manifest.id == *id)
                .collect();

            if matching.is_empty() {
                bail!("minecraft {id} is not in the data cache");
            }

            matching
        }
        None => versions
    };

    for version in selected {
        std::fs::remove_dir_all(&version.path)?;

        context
            .shell()
            .status("Removed", format!("minecraft {} data", version.manifest.id))?;
    }

    Ok(())
}
