use std::path::PathBuf;

use crate::context::SnormContext;
use crate::core::schematic;
use crate::ops::OutputTarget;
use crate::utils::errors::SnormResult;

pub struct RegionListOptions {
    pub schematic: PathBuf
}

pub fn list(context: &mut SnormContext, options: &RegionListOptions) -> SnormResult<()> {
    let schematic = schematic::load(&options.schematic)?;

    let mut shell = context.shell();
    let out = shell.out();

    for region in &schematic.regions {
        let size = region.size.abs();

        writeln!(
            out,
            "\"{}\"  position ({}, {}, {})  size {}x{}x{}  blocks {}  block entities {}  entities {}",
            region.name,
            region.position.x,
            region.position.y,
            region.position.z,
            size.x,
            size.y,
            size.z,
            region.total_blocks(),
            region.block_entities.len(),
            region.entities.len()
        )?;
    }

    Ok(())
}

pub struct RegionRenameOptions {
    pub schematic: PathBuf,
    pub renames: Vec<(String, String)>,
    pub output: OutputTarget
}

pub fn rename(context: &mut SnormContext, options: &RegionRenameOptions) -> SnormResult<()> {
    let mut schematic = schematic::load(&options.schematic)?;

    schematic::rename_regions(&mut schematic, &options.renames)?;

    for (old, new) in &options.renames {
        context
            .shell()
            .status("Renamed", format!("'{old}' -> '{new}'"))?;
    }

    let output = options.output.resolve(&options.schematic);

    schematic::save(&schematic, &output)?;

    context
        .shell()
        .status("Finished", format!("wrote {}", output.display()))?;

    Ok(())
}
