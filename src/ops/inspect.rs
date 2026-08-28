use std::collections::HashMap;
use std::path::PathBuf;

use crate::context::SnormContext;
use crate::core::block::BlockId;
use crate::core::category;
use crate::core::category::Category;
use crate::core::mcdata::McData;
use crate::core::palette;
use crate::core::palette::Palette;
use crate::core::schematic;
use crate::utils::errors::SnormResult;

pub struct InspectOptions {
    pub schematic: PathBuf,
    pub palette: Option<PathBuf>
}

pub fn inspect(context: &mut SnormContext, options: &InspectOptions) -> SnormResult<()> {
    let schematic = schematic::load(&options.schematic)?;

    let palette = palette::discover(&context.cwd.clone(), options.palette.as_deref())?
        .map(|(palette, _)| palette)
        .unwrap_or_default();

    let mcdata = McData::load_best(schematic.metadata.minecraft_data_version)?;

    if mcdata.is_degraded() {
        context.shell().warn(
            "no minecraft data extracted; category detection is limited \
             (run 'snorm data extract')"
        )?;
    }

    let mut shell = context.shell();
    let out = shell.out();

    let metadata = &schematic.metadata;
    let enclosing = schematic.enclosing_size();

    writeln!(out, "name:           {}", metadata.name)?;
    writeln!(out, "author:         {}", metadata.author)?;
    writeln!(out, "description:    {}", metadata.description)?;

    match metadata.sub_version {
        Some(sub_version) => {
            writeln!(out, "format version: {}.{}", metadata.version, sub_version)?;
        }
        None => writeln!(out, "format version: {}", metadata.version)?
    }

    writeln!(out, "data version:   {}", metadata.minecraft_data_version)?;

    if let Some(manifest) = mcdata.manifest() {
        writeln!(
            out,
            "block data:     minecraft {} (data version {})",
            manifest.id, manifest.data_version
        )?;
    }

    writeln!(out, "regions:        {}", schematic.regions.len())?;
    writeln!(out, "total blocks:   {}", schematic.total_blocks())?;
    writeln!(out, "total volume:   {}", schematic.total_volume())?;
    writeln!(
        out,
        "enclosing size: {}x{}x{}",
        enclosing.x, enclosing.y, enclosing.z
    )?;

    for region in &schematic.regions {
        let size = region.size.abs();

        writeln!(out)?;
        writeln!(
            out,
            "region \"{}\"  position ({}, {}, {})  size {}x{}x{}",
            region.name,
            region.position.x,
            region.position.y,
            region.position.z,
            size.x,
            size.y,
            size.z
        )?;
        writeln!(out, "  block entities: {}", region.block_entities.len())?;
        writeln!(out, "  entities:       {}", region.entities.len())?;

        let mut categories: HashMap<&str, Option<String>> = HashMap::new();

        for state in region.block_palette() {
            categories
                .entry(state.name.as_ref())
                .or_insert_with(|| marker(state, &palette, &mcdata));
        }

        let mut counts: HashMap<&str, u64> = HashMap::new();

        for (_, state) in region.blocks() {
            *counts.entry(state.name.as_ref()).or_default() += 1;
        }

        let mut counts: Vec<(&str, u64)> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

        writeln!(out, "  blocks:")?;

        for (name, count) in counts {
            match categories.get(name).map(Option::as_deref) {
                Some(Some(category)) => {
                    writeln!(out, "    {count:>9}  {name}  [{category}]")?;
                }
                _ => writeln!(out, "    {count:>9}  {name}")?
            }
        }
    }

    Ok(())
}

/// The marker shown next to a block, mirroring what normalize would do with
/// the discovered palette: protection and overrides beat categories, and
/// only blocks the solid prompt would actually offer are candidates.
fn marker(state: &mcdata::GenericBlockState, palette: &Palette, mcdata: &McData) -> Option<String> {
    let key = schematic::state_key(state);
    let detected = category::detect(&key, mcdata).map(|c| String::from(c.key()));

    let Ok(id) = BlockId::parse(key.name()) else {
        return detected;
    };

    if palette.protected.contains(&id, mcdata) {
        return Some(String::from("protected"));
    }

    let overridden = palette
        .overrides
        .iter()
        .any(|spec| spec.sources.contains(&id));

    if overridden {
        return Some(String::from("override"));
    }

    if palette.solid_members.contains(&id, mcdata) {
        return Some(String::from("solid"));
    }

    detected.or_else(|| {
        let eligible = category::is_solid_candidate(id.as_str(), mcdata)
            && palette.targets.get(&Category::Solid) != Some(&id);

        eligible.then(|| String::from("solid candidate"))
    })
}
