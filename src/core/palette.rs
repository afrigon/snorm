use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use directories::ProjectDirs;
use serde::Deserialize;

use crate::core::block::BlockId;
use crate::core::category::Category;
use crate::core::mcdata::McData;
use crate::core::overrides::OverrideSpec;
use crate::utils::errors::SnormResult;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    data: RawData,

    #[serde(default)]
    palette: RawPalette,

    #[serde(default)]
    categories: RawCategories,

    #[serde(default)]
    overrides: BTreeMap<String, String>
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawData {
    jar: Option<PathBuf>
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPalette {
    solid: Option<String>,
    glass: Option<String>,
    glass_pane: Option<String>,
    terracotta: Option<String>,
    wall: Option<String>,
    stair: Option<String>,
    slab: Option<String>,
    coral: Option<String>
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCategories {
    #[serde(default)]
    solid: RawMembers,

    #[serde(default)]
    protected: RawMembers
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMembers {
    #[serde(default)]
    members: Vec<String>
}

/// A configured set of blocks: explicit ids plus `#minecraft:...` vanilla
/// tag references resolved against the extracted game data.
#[derive(Debug, Default)]
pub struct MemberSet {
    ids: HashSet<BlockId>,
    tags: Vec<String>
}

impl MemberSet {
    #[cfg(test)]
    pub fn insert(&mut self, id: BlockId) {
        self.ids.insert(id);
    }

    pub fn has_entries(&self) -> bool {
        !self.ids.is_empty() || !self.tags.is_empty()
    }

    pub fn contains(&self, id: &BlockId, mcdata: &McData) -> bool {
        self.ids.contains(id)
            || self
                .tags
                .iter()
                .any(|tag| mcdata.tag_contains(tag, id.as_str()))
    }
}

fn parse_members(raw: RawMembers, category: &str) -> SnormResult<MemberSet> {
    let mut set = MemberSet::default();

    for entry in raw.members {
        match entry.strip_prefix('#') {
            Some(tag) => {
                let Some(name) = tag.strip_prefix("minecraft:") else {
                    bail!(
                        "invalid {category} member '{entry}' \
                         (only #minecraft: tag references are supported)"
                    );
                };

                set.tags.push(String::from(name));
            }
            None => {
                let id =
                    BlockId::parse(&entry).with_context(|| format!("invalid {category} member"))?;

                set.ids.insert(id);
            }
        }
    }

    Ok(set)
}

/// A parsed and validated palette configuration.
#[derive(Debug, Default)]
pub struct Palette {
    pub targets: HashMap<Category, BlockId>,

    /// Coral is normalized to a family (e.g. `tube`) rather than a block,
    /// preserving each block's shape and dead/alive state.
    pub coral_family: Option<String>,

    /// Standing solid selection: replaced without the interactive prompt.
    pub solid_members: MemberSet,

    /// Blocks no category may replace; explicit overrides still apply.
    pub protected: MemberSet,

    pub overrides: Vec<OverrideSpec>,
    pub jar: Option<PathBuf>
}

pub fn load(path: &Path) -> SnormResult<Palette> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("could not read palette '{}'", path.display()))?;

    let raw: RawConfig = toml::from_str(&contents)
        .with_context(|| format!("could not parse palette '{}'", path.display()))?;

    parse(raw).with_context(|| format!("in palette '{}'", path.display()))
}

fn parse(raw: RawConfig) -> SnormResult<Palette> {
    let mut palette = Palette {
        jar: raw.data.jar,
        ..Palette::default()
    };

    let targets = [
        (Category::Solid, raw.palette.solid),
        (Category::Glass, raw.palette.glass),
        (Category::GlassPane, raw.palette.glass_pane),
        (Category::Terracotta, raw.palette.terracotta),
        (Category::Wall, raw.palette.wall),
        (Category::Stair, raw.palette.stair),
        (Category::Slab, raw.palette.slab)
    ];

    for (category, target) in targets {
        let Some(target) = target else {
            continue;
        };

        let id = BlockId::parse(&target)
            .with_context(|| format!("invalid {} target", category.key()))?;

        palette.targets.insert(category, id);
    }

    if let Some(family) = raw.palette.coral {
        let valid = !family.is_empty()
            && family
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');

        if !valid {
            bail!("invalid coral family '{family}' (expected a family name such as 'tube')");
        }

        palette.coral_family = Some(family);
    }

    palette.solid_members = parse_members(raw.categories.solid, "solid")?;
    palette.protected = parse_members(raw.categories.protected, "protected")?;

    for (source, target) in raw.overrides {
        let spec = OverrideSpec::parse(&format!("{source}={target}"))?;
        palette.overrides.push(spec);
    }

    Ok(palette)
}

/// Locate the palette configuration: an explicit path, `snorm.toml` in the
/// working directory, or `palette.toml` in the user configuration directory.
pub fn discover(cwd: &Path, explicit: Option<&Path>) -> SnormResult<Option<(Palette, PathBuf)>> {
    if let Some(path) = explicit {
        return Ok(Some((load(path)?, path.to_path_buf())));
    }

    for candidate in search_paths(cwd) {
        if candidate.is_file() {
            return Ok(Some((load(&candidate)?, candidate)));
        }
    }

    Ok(None)
}

pub fn search_paths(cwd: &Path) -> Vec<PathBuf> {
    let mut paths = vec![cwd.join("snorm.toml")];

    if let Some(dirs) = ProjectDirs::from("", "", "snorm") {
        paths.push(dirs.config_dir().join("palette.toml"));
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_config() {
        let raw: RawConfig = toml::from_str(
            r##"
            [data]
            jar = "/tmp/server.jar"

            [palette]
            solid = "minecraft:stone_bricks"
            stair = "stone_brick_stairs"
            coral = "tube"

            [categories.solid]
            members = ["minecraft:cobblestone", "andesite"]

            [categories.protected]
            members = ["minecraft:obsidian", "#minecraft:beacon_base_blocks"]

            [overrides]
            "minecraft:oak_stairs" = ""
            "minecraft:mossy_cobblestone" = "minecraft:stone_bricks"
            "##
        )
        .unwrap();

        let palette = parse(raw).unwrap();
        let mcdata = McData::empty();

        assert_eq!(
            palette.targets.get(&Category::Solid).unwrap().as_str(),
            "minecraft:stone_bricks"
        );
        assert_eq!(
            palette.targets.get(&Category::Stair).unwrap().as_str(),
            "minecraft:stone_brick_stairs"
        );
        assert_eq!(palette.coral_family.as_deref(), Some("tube"));
        assert!(
            palette
                .solid_members
                .contains(&BlockId::parse("minecraft:andesite").unwrap(), &mcdata)
        );
        assert!(
            palette
                .protected
                .contains(&BlockId::parse("minecraft:obsidian").unwrap(), &mcdata)
        );
        assert_eq!(palette.overrides.len(), 2);
        assert!(
            palette
                .overrides
                .iter()
                .any(|o| o.sources[0].as_str() == "minecraft:oak_stairs" && o.target.is_none())
        );
    }

    #[test]
    fn member_tags_resolve_through_mcdata() {
        let raw: RawConfig = toml::from_str(
            r##"
            [categories.protected]
            members = ["#minecraft:beacon_base_blocks"]
            "##
        )
        .unwrap();

        let palette = parse(raw).unwrap();

        let mcdata = McData::for_tests(
            Default::default(),
            [(
                String::from("beacon_base_blocks"),
                [String::from("minecraft:iron_block")].into()
            )]
            .into()
        );

        let iron = BlockId::parse("minecraft:iron_block").unwrap();
        assert!(palette.protected.contains(&iron, &mcdata));
        assert!(!palette.protected.contains(&iron, &McData::empty()));
    }

    #[test]
    fn rejects_non_minecraft_tag_references() {
        let raw: RawConfig = toml::from_str(
            r##"
            [categories.protected]
            members = ["#c:ores"]
            "##
        )
        .unwrap();

        assert!(parse(raw).is_err());
    }

    #[test]
    fn rejects_unknown_keys() {
        let result: Result<RawConfig, _> = toml::from_str(
            r#"
            [palette]
            stairs = "minecraft:stone_brick_stairs"
            "#
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_coral_family() {
        let raw: RawConfig = toml::from_str(
            r#"
            [palette]
            coral = "minecraft:tube_coral"
            "#
        )
        .unwrap();

        assert!(parse(raw).is_err());
    }
}
