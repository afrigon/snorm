use crate::core::block::BlockStateKey;
use crate::core::mcdata::McData;

/// Block categories a palette can normalize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Solid,
    Glass,
    GlassPane,
    Terracotta,
    Wall,
    Stair,
    Slab,
    Coral
}

impl Category {
    pub fn key(self) -> &'static str {
        match self {
            Category::Solid => "solid",
            Category::Glass => "glass",
            Category::GlassPane => "glass_pane",
            Category::Terracotta => "terracotta",
            Category::Wall => "wall",
            Category::Stair => "stair",
            Category::Slab => "slab",
            Category::Coral => "coral"
        }
    }
}

/// Detect the intrinsic category of a block state, layered from most to
/// least authoritative: vanilla tag, blocks report definition type, state
/// property signature, name pattern. The signature and name layers also
/// cover modded blocks and degraded mode (no extracted data).
///
/// Solid is never detected: it is selection based (see
/// [`is_solid_candidate`]) because plain building blocks cannot be told
/// apart from special-purpose blocks like obsidian reliably.
pub fn detect(state: &BlockStateKey, mcdata: &McData) -> Option<Category> {
    detect_by_tag(state.name(), mcdata)
        .or_else(|| detect_by_definition(state.name(), mcdata))
        .or_else(|| detect_by_signature(state))
        .or_else(|| detect_by_name(state.name()))
}

/// Whether a block could be normalized as a solid building block: its class
/// in the blocks report is featureless with no state properties. Blocks with
/// gameplay behavior (redstone components, falling blocks, slime, ice, ores,
/// ...) have their own class and never qualify. Candidates are only ever
/// replaced after an explicit selection. Without extracted data nothing
/// qualifies.
pub fn is_solid_candidate(name: &str, mcdata: &McData) -> bool {
    // Some versions give dyed terracotta its own class even though it
    // behaves as a plain building block.
    const PLAIN_CLASSES: [&str; 2] = ["minecraft:block", "minecraft:terracotta"];

    let Some(info) = mcdata.block(name) else {
        return false;
    };

    info.definition
        .kind
        .as_deref()
        .is_some_and(|kind| PLAIN_CLASSES.contains(&kind))
        && info.properties.is_empty()
}

fn detect_by_tag(name: &str, mcdata: &McData) -> Option<Category> {
    const TAGS: [(&str, Category); 8] = [
        ("stairs", Category::Stair),
        ("slabs", Category::Slab),
        ("walls", Category::Wall),
        ("glazed_terracotta", Category::Terracotta),
        ("impermeable", Category::Glass),
        ("corals", Category::Coral),
        ("coral_blocks", Category::Coral),
        ("wall_corals", Category::Coral)
    ];

    TAGS.iter()
        .find(|(tag, _)| mcdata.tag_contains(tag, name))
        .map(|(_, category)| *category)
}

fn detect_by_definition(name: &str, mcdata: &McData) -> Option<Category> {
    let kind = mcdata.block(name)?.definition.kind.as_deref()?;

    match kind {
        "minecraft:stair" => Some(Category::Stair),
        "minecraft:slab" => Some(Category::Slab),
        "minecraft:wall" => Some(Category::Wall),
        "minecraft:stained_glass_pane" => Some(Category::GlassPane),
        // Vanilla glass panes share the iron bars block class; the name
        // check keeps actual iron bars out of the glass pane category.
        "minecraft:iron_bars" if name.contains("glass") => Some(Category::GlassPane),
        "minecraft:transparent" | "minecraft:stained_glass" | "minecraft:tinted_glass" => {
            Some(Category::Glass)
        }
        "minecraft:glazed_terracotta" => Some(Category::Terracotta),
        "minecraft:coral_plant"
        | "minecraft:base_coral_plant"
        | "minecraft:coral_fan"
        | "minecraft:base_coral_fan"
        | "minecraft:coral_wall_fan"
        | "minecraft:base_coral_wall_fan"
        | "minecraft:coral_block" => Some(Category::Coral),
        _ => None
    }
}

fn detect_by_signature(state: &BlockStateKey) -> Option<Category> {
    let has = |name: &str| state.properties().iter().any(|(n, _)| n == name);
    let value_of = |name: &str| {
        state
            .properties()
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };

    if has("facing") && has("half") && has("shape") {
        return Some(Category::Stair);
    }

    if let Some(value) = value_of("type")
        && matches!(value, "top" | "bottom" | "double")
    {
        return Some(Category::Slab);
    }

    let sides = ["north", "east", "south", "west"];

    if sides.iter().all(|side| has(side)) {
        let side_values: Vec<&str> = sides.iter().filter_map(|side| value_of(side)).collect();

        if side_values
            .iter()
            .all(|v| matches!(*v, "none" | "low" | "tall"))
            && has("up")
        {
            return Some(Category::Wall);
        }

        // Fences and iron bars share this boolean signature, so panes
        // additionally need a name hint.
        if side_values.iter().all(|v| matches!(*v, "true" | "false"))
            && (state.name().contains("pane") || state.name().contains("glass"))
        {
            return Some(Category::GlassPane);
        }
    }

    None
}

fn detect_by_name(name: &str) -> Option<Category> {
    let path = name.split_once(':').map(|(_, path)| path).unwrap_or(name);

    if path.contains("coral") {
        return Some(Category::Coral);
    }

    if path.ends_with("_stairs") {
        return Some(Category::Stair);
    }

    if path.ends_with("_slab") {
        return Some(Category::Slab);
    }

    if path.ends_with("_wall") {
        return Some(Category::Wall);
    }

    if path.ends_with("_pane") {
        return Some(Category::GlassPane);
    }

    if path.ends_with("glazed_terracotta") {
        return Some(Category::Terracotta);
    }

    if path.ends_with("glass") {
        return Some(Category::Glass);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str, properties: &[(&str, &str)]) -> BlockStateKey {
        BlockStateKey::new(
            name,
            properties
                .iter()
                .map(|(n, v)| (String::from(*n), String::from(*v)))
        )
    }

    #[test]
    fn detects_by_signature_without_mcdata() {
        let mcdata = McData::empty();

        let stairs = key(
            "mod:fancy_steps",
            &[
                ("facing", "north"),
                ("half", "bottom"),
                ("shape", "straight")
            ]
        );
        assert_eq!(detect(&stairs, &mcdata), Some(Category::Stair));

        let slab = key("mod:fancy_step", &[("type", "double")]);
        assert_eq!(detect(&slab, &mcdata), Some(Category::Slab));

        let wall = key(
            "mod:fancy_barrier",
            &[
                ("north", "low"),
                ("east", "none"),
                ("south", "tall"),
                ("west", "none"),
                ("up", "true")
            ]
        );
        assert_eq!(detect(&wall, &mcdata), Some(Category::Wall));

        let fence = key(
            "minecraft:oak_fence",
            &[
                ("north", "true"),
                ("east", "false"),
                ("south", "true"),
                ("west", "false")
            ]
        );
        assert_eq!(detect(&fence, &mcdata), None);

        let pane = key(
            "mod:crystal_pane",
            &[
                ("north", "true"),
                ("east", "false"),
                ("south", "true"),
                ("west", "false")
            ]
        );
        assert_eq!(detect(&pane, &mcdata), Some(Category::GlassPane));
    }

    #[test]
    fn detects_by_name_without_mcdata() {
        let mcdata = McData::empty();

        assert_eq!(
            detect(&key("minecraft:cyan_glazed_terracotta", &[]), &mcdata),
            Some(Category::Terracotta)
        );
        assert_eq!(
            detect(&key("minecraft:cyan_terracotta", &[]), &mcdata),
            None
        );
        assert_eq!(
            detect(&key("minecraft:tinted_glass", &[]), &mcdata),
            Some(Category::Glass)
        );
        assert_eq!(
            detect(&key("minecraft:dead_brain_coral_fan", &[]), &mcdata),
            Some(Category::Coral)
        );
        assert_eq!(detect(&key("minecraft:wall_torch", &[]), &mcdata), None);
        assert_eq!(detect(&key("minecraft:stone", &[]), &mcdata), None);
    }
}
