use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;

use mcdata::BlockEntity;
use mcdata::GenericBlockState;
use mcdata::util::BlockPos;

use crate::core::block::BlockId;
use crate::core::block::BlockStateKey;
use crate::core::category;
use crate::core::category::Category;
use crate::core::mcdata::McData;
use crate::core::overrides::OverrideSpec;
use crate::core::palette::Palette;
use crate::core::report::RegionReport;
use crate::core::schematic;

const UNTOUCHABLE: [&str; 4] = [
    "minecraft:air",
    "minecraft:cave_air",
    "minecraft:void_air",
    "minecraft:structure_void"
];

#[derive(Debug, Clone)]
pub enum Decision {
    Keep,
    Replace {
        state: GenericBlockState,

        /// Property names of the source state the target's schema does not
        /// accept, dropped from the replacement.
        dropped: Vec<String>,

        /// Whether this replacement came from an explicit override rather
        /// than a palette category; explicit replacements also apply to
        /// blocks with block entity data.
        explicit: bool
    }
}

#[derive(Debug, Clone)]
pub struct EntryPlan {
    pub decision: Decision,
    pub category: Option<Category>,

    /// Whether the block is configuration protected: kept despite its
    /// category, unless an explicit override targets it.
    pub protected: bool
}

/// Precomputed replacement decisions for every entry of a region's block
/// state palette, so the per-block pass is a plain map lookup.
#[derive(Debug, Default)]
pub struct ReplacementPlan {
    entries: HashMap<BlockStateKey, EntryPlan>
}

impl ReplacementPlan {
    pub fn get(&self, key: &BlockStateKey) -> Option<&EntryPlan> {
        self.entries.get(key)
    }

    /// Decide the fate of every palette entry. Precedence per entry: CLI
    /// overrides (in argument order), then configuration overrides, then the
    /// palette category target. `solid_selection` holds the blocks chosen
    /// for solid normalization in this run, on top of the configured
    /// standing members.
    pub fn build(
        palette_entries: &[GenericBlockState],
        palette: &Palette,
        cli_overrides: &[OverrideSpec],
        solid_selection: &HashSet<BlockId>,
        mcdata: &McData
    ) -> ReplacementPlan {
        let mut plan = ReplacementPlan::default();

        for state in palette_entries {
            let key = schematic::state_key(state);

            if plan.entries.contains_key(&key) {
                continue;
            }

            let entry = plan_entry(state, &key, palette, cli_overrides, solid_selection, mcdata);
            plan.entries.insert(key, entry);
        }

        plan
    }
}

/// Rank the blocks eligible for solid normalization across regions by how
/// often they occur: plain building blocks (see
/// [`category::is_solid_candidate`]) that are not protected, not standing
/// members, not overridden, not in another configured set, and not already
/// the solid target.
pub fn solid_candidates(
    regions: &[&schematic::SchematicRegion],
    palette: &Palette,
    cli_overrides: &[OverrideSpec],
    mcdata: &McData
) -> Vec<(BlockId, u64)> {
    let target = palette.targets.get(&Category::Solid);

    let mut counts: HashMap<BlockId, u64> = HashMap::new();

    for region in regions {
        for (_, state) in region.blocks() {
            let Ok(id) = BlockId::parse(&state.name) else {
                continue;
            };

            if !category::is_solid_candidate(id.as_str(), mcdata) {
                continue;
            }

            if Some(&id) == target
                || palette.protected.contains(&id, mcdata)
                || palette.solid_members.contains(&id, mcdata)
            {
                continue;
            }

            let overridden = cli_overrides
                .iter()
                .chain(palette.overrides.iter())
                .any(|spec| spec.sources.contains(&id));

            if overridden {
                continue;
            }

            *counts.entry(id).or_default() += 1;
        }
    }

    let mut candidates: Vec<(BlockId, u64)> = counts.into_iter().collect();
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    candidates
}

/// Walk a region and apply a [`ReplacementPlan`] to every block. Blocks with
/// block entity data are only replaced by explicit overrides (removing the
/// stale block entity); category replacements skip them.
pub fn apply(region: &mut schematic::SchematicRegion, plan: &ReplacementPlan) -> RegionReport {
    let size = region.size.abs();

    let mut report = RegionReport {
        name: region.name.to_string(),
        size: (size.x, size.y, size.z),
        ..RegionReport::default()
    };

    let block_entity_positions: HashSet<(i32, i32, i32)> = region
        .block_entities
        .iter()
        .map(|entity| {
            let position = entity.position();
            (position.x, position.y, position.z)
        })
        .collect();

    let mut warned: HashSet<(String, String)> = HashSet::new();

    let x_range = region.x_range();
    let y_range = region.y_range();
    let z_range = region.z_range();

    for y in y_range {
        for z in z_range.clone() {
            for x in x_range.clone() {
                let position = BlockPos::new(x, y, z);

                let Some(current) = region.get_block_opt(position) else {
                    continue;
                };

                let key = schematic::state_key(current);

                if !UNTOUCHABLE.contains(&key.name()) {
                    report.blocks += 1;
                }

                let Some(entry) = plan.get(&key) else {
                    continue;
                };

                let Decision::Replace {
                    state,
                    dropped,
                    explicit
                } = &entry.decision
                else {
                    if !UNTOUCHABLE.contains(&key.name()) {
                        let category = if entry.protected {
                            Some(String::from("protected"))
                        } else {
                            entry.category.map(|c| String::from(c.key()))
                        };

                        *report
                            .kept
                            .entry((String::from(key.name()), category))
                            .or_default() += 1;
                    }

                    continue;
                };

                let from = current.name.to_string();
                let to = state.name.to_string();
                let replacement = state.clone();

                if block_entity_positions.contains(&(x, y, z)) {
                    if !explicit {
                        *report.skipped_block_entities.entry(from).or_default() += 1;
                        continue;
                    }

                    region.remove_block_entity(position);
                }

                region.set_block(position, replacement);

                if !dropped.is_empty() && warned.insert((from.clone(), to.clone())) {
                    report.warnings.push(format!(
                        "dropped {} replacing {from} -> {to}",
                        dropped.join(", ")
                    ));
                }

                *report.replacements.entry((from, to)).or_default() += 1;
            }
        }
    }

    report
}

fn plan_entry(
    state: &GenericBlockState,
    key: &BlockStateKey,
    palette: &Palette,
    cli_overrides: &[OverrideSpec],
    solid_selection: &HashSet<BlockId>,
    mcdata: &McData
) -> EntryPlan {
    if UNTOUCHABLE.contains(&key.name()) {
        return EntryPlan {
            decision: Decision::Keep,
            category: None,
            protected: false
        };
    }

    let Ok(source) = BlockId::parse(key.name()) else {
        return EntryPlan {
            decision: Decision::Keep,
            category: None,
            protected: false
        };
    };

    let category =
        if solid_selection.contains(&source) || palette.solid_members.contains(&source, mcdata) {
            Some(Category::Solid)
        } else {
            category::detect(key, mcdata)
        };

    let matched_override = cli_overrides
        .iter()
        .chain(palette.overrides.iter())
        .find(|spec| spec.sources.contains(&source));

    if let Some(spec) = matched_override {
        let decision = match &spec.target {
            None => Decision::Keep,
            Some(target) => replace_with(state, target, true, mcdata)
        };

        return EntryPlan {
            decision,
            category,
            protected: false
        };
    }

    if palette.protected.contains(&source, mcdata) {
        return EntryPlan {
            decision: Decision::Keep,
            category,
            protected: true
        };
    }

    let decision = match category {
        Some(Category::Coral) => match &palette.coral_family {
            Some(family) => coral_replacement(state, &source, family, mcdata),
            None => Decision::Keep
        },
        Some(category) => match palette.targets.get(&category) {
            Some(target) if *target != source => replace_with(state, target, false, mcdata),
            _ => Decision::Keep
        },
        None => Decision::Keep
    };

    EntryPlan {
        decision,
        category,
        protected: false
    }
}

fn replace_with(
    state: &GenericBlockState,
    target: &BlockId,
    explicit: bool,
    mcdata: &McData
) -> Decision {
    let schema = mcdata.block(target.as_str()).map(|info| &info.properties);

    let mut properties: HashMap<Cow<'static, str>, Cow<'static, str>> = HashMap::new();
    let mut dropped = Vec::new();

    for (name, value) in &state.properties {
        let accepted = match schema {
            // Without extracted data the properties carry over unvalidated.
            None => true,
            Some(schema) => schema
                .get(name.as_ref())
                .is_some_and(|values| values.iter().any(|v| v == value.as_ref()))
        };

        if accepted {
            properties.insert(Cow::Owned(name.to_string()), Cow::Owned(value.to_string()));
        } else {
            dropped.push(name.to_string());
        }
    }

    dropped.sort();

    Decision::Replace {
        state: GenericBlockState {
            name: Cow::Owned(String::from(target.as_str())),
            properties
        },
        dropped,
        explicit
    }
}

/// Swap the family of a coral block while preserving its shape and dead or
/// alive state: `dead_brain_coral_fan` with family `tube` becomes
/// `dead_tube_coral_fan`.
fn coral_replacement(
    state: &GenericBlockState,
    source: &BlockId,
    family: &str,
    mcdata: &McData
) -> Decision {
    let path = source.path();

    let (dead, rest) = match path.strip_prefix("dead_") {
        Some(rest) => ("dead_", rest),
        None => ("", path)
    };

    let Some(coral) = rest.find("_coral") else {
        return Decision::Keep;
    };

    let (source_family, suffix) = rest.split_at(coral);

    if source_family == family {
        return Decision::Keep;
    }

    let target = format!("{}:{dead}{family}{suffix}", source.namespace());

    let Ok(target) = BlockId::parse(&target) else {
        return Decision::Keep;
    };

    replace_with(state, &target, false, mcdata)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, properties: &[(&str, &str)]) -> GenericBlockState {
        GenericBlockState {
            name: Cow::Owned(String::from(name)),
            properties: properties
                .iter()
                .map(|(n, v)| (Cow::Owned(String::from(*n)), Cow::Owned(String::from(*v))))
                .collect()
        }
    }

    fn palette_with_stair(target: &str) -> Palette {
        let mut palette = Palette::default();
        palette
            .targets
            .insert(Category::Stair, BlockId::parse(target).unwrap());
        palette
    }

    fn plan_for(entry: &GenericBlockState, palette: &Palette, cli: &[OverrideSpec]) -> EntryPlan {
        let entries = vec![entry.clone()];
        let plan =
            ReplacementPlan::build(&entries, palette, cli, &HashSet::new(), &McData::empty());

        plan.get(&schematic::state_key(entry)).unwrap().clone()
    }

    #[test]
    fn palette_category_replaces_and_preserves_state() {
        let stairs = state(
            "minecraft:oak_stairs",
            &[("facing", "north"), ("half", "top"), ("shape", "straight")]
        );

        let entry = plan_for(
            &stairs,
            &palette_with_stair("minecraft:stone_brick_stairs"),
            &[]
        );

        let Decision::Replace {
            state,
            dropped,
            explicit
        } = &entry.decision
        else {
            panic!("expected a replacement");
        };

        assert_eq!(state.name, "minecraft:stone_brick_stairs");
        assert_eq!(state.properties.get("facing").unwrap(), "north");
        assert_eq!(state.properties.get("half").unwrap(), "top");
        assert!(dropped.is_empty());
        assert!(!explicit);
    }

    #[test]
    fn cli_override_beats_palette() {
        let stairs = state("minecraft:oak_stairs", &[("facing", "north")]);
        let palette = palette_with_stair("minecraft:stone_brick_stairs");

        let keep = OverrideSpec::parse("minecraft:oak_stairs=").unwrap();
        let entry = plan_for(&stairs, &palette, &[keep]);
        assert!(matches!(entry.decision, Decision::Keep));

        let redirect = OverrideSpec::parse("minecraft:oak_stairs=minecraft:spruce_stairs").unwrap();
        let entry = plan_for(&stairs, &palette, &[redirect]);

        let Decision::Replace {
            state, explicit, ..
        } = &entry.decision
        else {
            panic!("expected a replacement");
        };

        assert_eq!(state.name, "minecraft:spruce_stairs");
        assert!(explicit);
    }

    #[test]
    fn cli_override_beats_config_override() {
        let stairs = state("minecraft:oak_stairs", &[]);

        let mut palette = Palette::default();
        palette.overrides.push(
            OverrideSpec::parse("minecraft:oak_stairs=minecraft:stone_brick_stairs").unwrap()
        );

        let cli = OverrideSpec::parse("minecraft:oak_stairs=minecraft:spruce_stairs").unwrap();
        let entry = plan_for(&stairs, &palette, &[cli]);

        let Decision::Replace { state, .. } = &entry.decision else {
            panic!("expected a replacement");
        };

        assert_eq!(state.name, "minecraft:spruce_stairs");
    }

    #[test]
    fn solid_members_use_solid_target() {
        let mut palette = Palette::default();
        palette.targets.insert(
            Category::Solid,
            BlockId::parse("minecraft:stone_bricks").unwrap()
        );
        palette
            .solid_members
            .insert(BlockId::parse("minecraft:cobblestone").unwrap());

        let cobble = state("minecraft:cobblestone", &[]);
        let entry = plan_for(&cobble, &palette, &[]);

        let Decision::Replace {
            state: replacement, ..
        } = &entry.decision
        else {
            panic!("expected a replacement");
        };

        assert_eq!(replacement.name, "minecraft:stone_bricks");
        assert_eq!(entry.category, Some(Category::Solid));

        // Without extracted data nothing is automatically solid.
        let dirt = state("minecraft:dirt", &[]);
        let entry = plan_for(&dirt, &palette, &[]);
        assert!(matches!(entry.decision, Decision::Keep));
    }

    fn plain_block_mcdata(names: &[&str]) -> McData {
        use crate::core::mcdata::BlockDefinition;
        use crate::core::mcdata::BlockInfo;

        let blocks = names
            .iter()
            .map(|name| {
                (
                    String::from(*name),
                    BlockInfo {
                        definition: BlockDefinition {
                            kind: Some(String::from("minecraft:block"))
                        },
                        properties: Default::default()
                    }
                )
            })
            .collect();

        McData::for_tests(blocks, Default::default())
    }

    fn plan_for_with(
        entry: &GenericBlockState,
        palette: &Palette,
        cli: &[OverrideSpec],
        mcdata: &McData
    ) -> EntryPlan {
        let entries = vec![entry.clone()];
        let plan = ReplacementPlan::build(&entries, palette, cli, &HashSet::new(), mcdata);

        plan.get(&schematic::state_key(entry)).unwrap().clone()
    }

    #[test]
    fn solid_replaces_only_selected_blocks() {
        let mcdata = plain_block_mcdata(&["minecraft:dirt", "minecraft:obsidian"]);

        let mut palette = Palette::default();
        palette.targets.insert(
            Category::Solid,
            BlockId::parse("minecraft:stone_bricks").unwrap()
        );

        let selection: HashSet<BlockId> = [BlockId::parse("minecraft:dirt").unwrap()].into();

        let dirt = state("minecraft:dirt", &[]);
        let entries = vec![dirt.clone()];
        let plan = ReplacementPlan::build(&entries, &palette, &[], &selection, &mcdata);
        let entry = plan.get(&schematic::state_key(&dirt)).unwrap();

        let Decision::Replace {
            state: replacement, ..
        } = &entry.decision
        else {
            panic!("expected a replacement");
        };

        assert_eq!(replacement.name, "minecraft:stone_bricks");
        assert_eq!(entry.category, Some(Category::Solid));

        // A candidate that was not selected passes through untouched.
        let obsidian = state("minecraft:obsidian", &[]);
        let entry = plan_for_with(&obsidian, &palette, &[], &mcdata);
        assert!(matches!(entry.decision, Decision::Keep));
    }

    #[test]
    fn solid_candidates_are_ranked_by_usage() {
        let mcdata = plain_block_mcdata(&[
            "minecraft:dirt",
            "minecraft:stone",
            "minecraft:obsidian",
            "minecraft:stone_bricks"
        ]);

        let mut palette = Palette::default();
        palette.targets.insert(
            Category::Solid,
            BlockId::parse("minecraft:stone_bricks").unwrap()
        );
        palette
            .protected
            .insert(BlockId::parse("minecraft:obsidian").unwrap());

        let mut region =
            schematic::SchematicRegion::new("main", BlockPos::new(0, 0, 0), BlockPos::new(8, 1, 1));

        for x in 0..3 {
            region.set_block(BlockPos::new(x, 0, 0), state("minecraft:stone", &[]));
        }
        region.set_block(BlockPos::new(3, 0, 0), state("minecraft:dirt", &[]));
        region.set_block(BlockPos::new(4, 0, 0), state("minecraft:obsidian", &[]));
        region.set_block(BlockPos::new(5, 0, 0), state("minecraft:stone_bricks", &[]));
        region.set_block(BlockPos::new(6, 0, 0), state("minecraft:piston", &[]));

        let candidates = solid_candidates(&[&region], &palette, &[], &mcdata);

        let names: Vec<(&str, u64)> = candidates
            .iter()
            .map(|(id, count)| (id.as_str(), *count))
            .collect();

        // The most used candidate comes first; the protected block, the
        // target itself, and non-plain classes are absent.
        assert_eq!(names, vec![("minecraft:stone", 3), ("minecraft:dirt", 1)]);
    }

    #[test]
    fn protected_blocks_survive_categories_but_not_overrides() {
        let mcdata = plain_block_mcdata(&["minecraft:obsidian"]);

        let mut palette = Palette::default();
        palette.targets.insert(
            Category::Solid,
            BlockId::parse("minecraft:stone_bricks").unwrap()
        );
        palette
            .protected
            .insert(BlockId::parse("minecraft:obsidian").unwrap());

        let obsidian = state("minecraft:obsidian", &[]);

        let entry = plan_for_with(&obsidian, &palette, &[], &mcdata);
        assert!(matches!(entry.decision, Decision::Keep));
        assert!(entry.protected);

        let redirect = OverrideSpec::parse("minecraft:obsidian=minecraft:crying_obsidian").unwrap();
        let entry = plan_for_with(&obsidian, &palette, &[redirect], &mcdata);

        let Decision::Replace {
            state: replacement, ..
        } = &entry.decision
        else {
            panic!("expected a replacement");
        };

        assert_eq!(replacement.name, "minecraft:crying_obsidian");
    }

    #[test]
    fn air_is_untouchable() {
        let mut palette = Palette::default();
        palette.targets.insert(
            Category::Solid,
            BlockId::parse("minecraft:stone_bricks").unwrap()
        );
        palette
            .solid_members
            .insert(BlockId::parse("minecraft:air").unwrap());

        let air = state("minecraft:air", &[]);
        let entry = plan_for(&air, &palette, &[]);
        assert!(matches!(entry.decision, Decision::Keep));
    }

    #[test]
    fn coral_swaps_family_preserving_shape() {
        let mut palette = Palette::default();
        palette.coral_family = Some(String::from("tube"));

        let cases = [
            (
                "minecraft:dead_brain_coral_fan",
                "minecraft:dead_tube_coral_fan"
            ),
            ("minecraft:brain_coral_block", "minecraft:tube_coral_block"),
            (
                "minecraft:brain_coral_wall_fan",
                "minecraft:tube_coral_wall_fan"
            ),
            ("minecraft:brain_coral", "minecraft:tube_coral")
        ];

        for (source, expected) in cases {
            let coral = state(source, &[("waterlogged", "true")]);
            let entry = plan_for(&coral, &palette, &[]);

            let Decision::Replace { state, .. } = &entry.decision else {
                panic!("expected a replacement for {source}");
            };

            assert_eq!(state.name.as_ref(), expected);
            assert_eq!(state.properties.get("waterlogged").unwrap(), "true");
        }

        let already_tube = state("minecraft:tube_coral", &[]);
        let entry = plan_for(&already_tube, &palette, &[]);
        assert!(matches!(entry.decision, Decision::Keep));
    }

    #[test]
    fn same_target_is_a_keep() {
        let stairs = state("minecraft:stone_brick_stairs", &[("facing", "north")]);
        let entry = plan_for(
            &stairs,
            &palette_with_stair("minecraft:stone_brick_stairs"),
            &[]
        );

        assert!(matches!(entry.decision, Decision::Keep));
    }

    #[test]
    fn applies_a_plan_end_to_end_through_a_file() {
        use mcdata::GenericBlockEntity;

        let mut region = schematic::SchematicRegion::new(
            "build",
            BlockPos::new(0, 0, 0),
            BlockPos::new(4, 1, 1)
        );

        let stairs = state(
            "minecraft:oak_stairs",
            &[("facing", "east"), ("half", "top"), ("shape", "straight")]
        );
        region.set_block(BlockPos::new(0, 0, 0), stairs);

        let chest = state("minecraft:chest", &[("facing", "north")]);
        region.set_block(BlockPos::new(1, 0, 0), chest.clone());
        region.set_block(BlockPos::new(2, 0, 0), chest);

        for x in [1, 2] {
            region.block_entities.push(GenericBlockEntity {
                id: "minecraft:chest".into(),
                pos: BlockPos::new(x, 0, 0),
                properties: HashMap::new()
            });
        }

        let modded = state("create:brass_block", &[]);
        region.set_block(BlockPos::new(3, 0, 0), modded);

        let mut palette = Palette::default();
        palette.targets.insert(
            Category::Stair,
            BlockId::parse("minecraft:stone_brick_stairs").unwrap()
        );

        // The chest at x=1 stays (category replacement via solid would skip
        // it anyway); an explicit override targets chests, which must
        // replace them and drop their block entities.
        let chest_override = OverrideSpec::parse("minecraft:chest=minecraft:stone_bricks").unwrap();

        let mcdata = McData::empty();
        let plan = ReplacementPlan::build(
            region.block_palette(),
            &palette,
            &[chest_override],
            &HashSet::new(),
            &mcdata
        );

        let report = apply(&mut region, &plan);

        assert_eq!(
            report.replacements[&(
                String::from("minecraft:oak_stairs"),
                String::from("minecraft:stone_brick_stairs")
            )],
            1
        );
        assert_eq!(
            report.replacements[&(
                String::from("minecraft:chest"),
                String::from("minecraft:stone_bricks")
            )],
            2
        );
        assert_eq!(report.replaced(), 3);
        assert!(report.skipped_block_entities.is_empty());
        assert!(region.block_entities.is_empty());

        let mut built: schematic::Schematic = schematic::Schematic::new("test", "", "snorm");
        built.regions.push(region);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normalized.litematic");
        schematic::save(&built, &path).unwrap();
        let loaded = schematic::load(&path).unwrap();

        let region = &loaded.regions[0];

        let stairs = region.get_block(BlockPos::new(0, 0, 0));
        assert_eq!(stairs.name, "minecraft:stone_brick_stairs");
        assert_eq!(stairs.properties.get("facing").unwrap(), "east");
        assert_eq!(stairs.properties.get("half").unwrap(), "top");

        let former_chest = region.get_block(BlockPos::new(1, 0, 0));
        assert_eq!(former_chest.name, "minecraft:stone_bricks");
        assert!(former_chest.properties.get("facing").is_some());

        let untouched = region.get_block(BlockPos::new(3, 0, 0));
        assert_eq!(untouched.name, "create:brass_block");
    }

    #[test]
    fn category_replacement_skips_block_entities() {
        use mcdata::GenericBlockEntity;

        let mut region = schematic::SchematicRegion::new(
            "build",
            BlockPos::new(0, 0, 0),
            BlockPos::new(1, 1, 1)
        );

        region.set_block(BlockPos::new(0, 0, 0), state("minecraft:cobblestone", &[]));
        region.block_entities.push(GenericBlockEntity {
            id: "minecraft:cobblestone".into(),
            pos: BlockPos::new(0, 0, 0),
            properties: HashMap::new()
        });

        let mut palette = Palette::default();
        palette.targets.insert(
            Category::Solid,
            BlockId::parse("minecraft:stone_bricks").unwrap()
        );
        palette
            .solid_members
            .insert(BlockId::parse("minecraft:cobblestone").unwrap());

        let plan = ReplacementPlan::build(
            region.block_palette(),
            &palette,
            &[],
            &HashSet::new(),
            &McData::empty()
        );
        let report = apply(&mut region, &plan);

        assert_eq!(report.replaced(), 0);
        assert_eq!(report.skipped_block_entities["minecraft:cobblestone"], 1);
        assert_eq!(
            region.get_block(BlockPos::new(0, 0, 0)).name,
            "minecraft:cobblestone"
        );
        assert_eq!(region.block_entities.len(), 1);
    }
}
