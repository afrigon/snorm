use std::collections::BTreeMap;

/// Everything that changed (or would change) in one region.
#[derive(Debug, Default)]
pub struct RegionReport {
    pub name: String,
    pub size: (i32, i32, i32),

    /// Replacement counts keyed by `(source block, target block)`.
    pub replacements: BTreeMap<(String, String), u64>,

    /// Blocks left untouched because they carry block entity data, keyed by
    /// block name.
    pub skipped_block_entities: BTreeMap<String, u64>,

    /// Property-drop notes for replacements that occurred in this region.
    pub warnings: Vec<String>,

    /// Counts of blocks that passed through unchanged (air excluded), keyed
    /// by block name and the detected category key, if any.
    pub kept: BTreeMap<(String, Option<String>), u64>,

    /// Number of non-air blocks in the region.
    pub blocks: u64
}

impl RegionReport {
    pub fn replaced(&self) -> u64 {
        self.replacements.values().sum()
    }
}

#[derive(Debug, Default)]
pub struct ChangeReport {
    pub regions: Vec<RegionReport>
}

impl ChangeReport {
    pub fn replaced(&self) -> u64 {
        self.regions.iter().map(RegionReport::replaced).sum()
    }

    pub fn blocks(&self) -> u64 {
        self.regions.iter().map(|r| r.blocks).sum()
    }

    pub fn warning_count(&self) -> usize {
        self.regions
            .iter()
            .map(|r| r.warnings.len() + r.skipped_block_entities.len())
            .sum()
    }
}
