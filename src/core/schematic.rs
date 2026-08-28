use std::path::Path;

use anyhow::Context;
use anyhow::bail;
use mcdata::GenericBlockState;
use rustmatica::Litematic;

use crate::core::block::BlockStateKey;
use crate::utils::errors::SnormResult;

pub type Schematic = Litematic;
pub type SchematicRegion = rustmatica::Region;

pub fn load(path: &Path) -> SnormResult<Schematic> {
    Litematic::read_file(path)
        .with_context(|| format!("could not read schematic '{}'", path.display()))
}

pub fn save(schematic: &Schematic, path: &Path) -> SnormResult<()> {
    schematic
        .write_file(path)
        .with_context(|| format!("could not write schematic '{}'", path.display()))
}

pub fn state_key(state: &GenericBlockState) -> BlockStateKey {
    BlockStateKey::new(
        state.name.to_string(),
        state
            .properties
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
    )
}

pub fn region_names(schematic: &Schematic) -> Vec<String> {
    schematic
        .regions
        .iter()
        .map(|region| region.name.to_string())
        .collect()
}

/// Rename regions according to `(old, new)` pairs, validating that every old
/// name exists and no new name collides with another region.
pub fn rename_regions(schematic: &mut Schematic, renames: &[(String, String)]) -> SnormResult<()> {
    for (old, new) in renames {
        if old == new {
            bail!("region rename '{old}={new}' does not change the name");
        }

        if schematic.regions.iter().any(|r| r.name == *new) {
            bail!("a region named '{new}' already exists");
        }

        let Some(region) = schematic.regions.iter_mut().find(|r| r.name == *old) else {
            let available = schematic
                .regions
                .iter()
                .map(|r| format!("'{}'", r.name))
                .collect::<Vec<String>>()
                .join(", ");

            bail!("no region named '{old}' (available: {available})");
        };

        region.name = new.clone().into();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use mcdata::util::BlockPos;
    use rustmatica::Region;

    use super::*;

    fn stairs(facing: &'static str) -> GenericBlockState {
        GenericBlockState {
            name: "minecraft:oak_stairs".into(),
            properties: [
                ("facing".into(), facing.into()),
                ("half".into(), "bottom".into()),
                ("shape".into(), "straight".into()),
                ("waterlogged".into(), "false".into())
            ]
            .into()
        }
    }

    fn fixture() -> Schematic {
        let mut tower = Region::new("tower", BlockPos::new(0, 0, 0), BlockPos::new(2, 2, 2));
        tower.set_block(BlockPos::new(0, 0, 0), stairs("north"));
        tower.set_block(BlockPos::new(1, 0, 0), stairs("east"));
        tower.set_block(
            BlockPos::new(0, 1, 0),
            GenericBlockState {
                name: "minecraft:stone".into(),
                properties: [].into()
            }
        );

        let mut moat = Region::new("moat", BlockPos::new(4, 0, 4), BlockPos::new(-2, 1, -2));
        moat.set_block(
            BlockPos::new(1, 0, 1),
            GenericBlockState {
                name: "minecraft:water".into(),
                properties: [("level".into(), "0".into())].into()
            }
        );

        let mut schematic = Schematic::new("fixture", "a test build", "snorm");
        schematic.regions.push(tower);
        schematic.regions.push(moat);
        schematic
    }

    #[test]
    fn survives_file_round_trip() {
        let schematic = fixture();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.litematic");

        save(&schematic, &path).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded.metadata.name, schematic.metadata.name);
        assert_eq!(loaded.metadata.description, schematic.metadata.description);
        assert_eq!(loaded.metadata.author, schematic.metadata.author);
        assert_eq!(
            loaded.metadata.time_created,
            schematic.metadata.time_created
        );

        // Regions live in an NBT compound, so their order is not preserved
        // and rustmatica pads its block index vector on read; compare by name
        // and semantic content instead of Region equality.
        for region in &schematic.regions {
            let loaded_region = loaded
                .regions
                .iter()
                .find(|r| r.name == region.name)
                .unwrap();

            assert_eq!(loaded_region.position, region.position);
            assert_eq!(loaded_region.size, region.size);
            assert_eq!(loaded_region.block_entities, region.block_entities);
            assert_eq!(loaded_region.entities, region.entities);

            let blocks: Vec<_> = region.blocks().collect();
            let loaded_blocks: Vec<_> = loaded_region.blocks().collect();
            assert_eq!(loaded_blocks, blocks);
        }
    }

    #[test]
    fn renames_regions_with_validation() {
        let mut schematic = fixture();

        rename_regions(
            &mut schematic,
            &[(String::from("tower"), String::from("keep"))]
        )
        .unwrap();

        assert_eq!(region_names(&schematic), vec!["keep", "moat"]);

        let missing = rename_regions(
            &mut schematic,
            &[(String::from("tower"), String::from("other"))]
        );
        assert!(missing.is_err());

        let collision = rename_regions(
            &mut schematic,
            &[(String::from("keep"), String::from("moat"))]
        );
        assert!(collision.is_err());

        let unchanged = rename_regions(
            &mut schematic,
            &[(String::from("keep"), String::from("keep"))]
        );
        assert!(unchanged.is_err());
    }
}
