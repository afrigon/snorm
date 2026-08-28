use anyhow::Context;
use anyhow::bail;

use crate::core::block::BlockId;
use crate::utils::errors::SnormResult;

/// A replacement override such as `minecraft:dirt,minecraft:stone=minecraft:deepslate`.
/// An empty target (`minecraft:dirt=`) exempts the sources from normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideSpec {
    pub sources: Vec<BlockId>,
    pub target: Option<BlockId>
}

impl OverrideSpec {
    pub fn parse(input: &str) -> SnormResult<OverrideSpec> {
        let Some((left, right)) = input.split_once('=') else {
            bail!("override '{input}' is missing '=' (expected 'source[,source]=target')");
        };

        if right.contains('=') {
            bail!("override '{input}' contains more than one '='");
        }

        let sources = left
            .split(',')
            .map(|source| BlockId::parse(source).with_context(|| format!("in override '{input}'")))
            .collect::<SnormResult<Vec<BlockId>>>()?;

        if sources.is_empty() {
            bail!("override '{input}' has no source blocks");
        }

        let right = right.trim();

        let target = if right.is_empty() {
            None
        } else {
            Some(BlockId::parse(right).with_context(|| format!("in override '{input}'"))?)
        };

        Ok(OverrideSpec { sources, target })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(input: &str) -> BlockId {
        BlockId::parse(input).unwrap()
    }

    #[test]
    fn parses_single_source() {
        let spec = OverrideSpec::parse("minecraft:dirt=minecraft:stone").unwrap();

        assert_eq!(spec.sources, vec![id("minecraft:dirt")]);
        assert_eq!(spec.target, Some(id("minecraft:stone")));
    }

    #[test]
    fn parses_multiple_sources() {
        let spec = OverrideSpec::parse("dirt,grass_block=stone").unwrap();

        assert_eq!(spec.sources, vec![id("dirt"), id("grass_block")]);
        assert_eq!(spec.target, Some(id("stone")));
    }

    #[test]
    fn empty_target_means_keep() {
        let spec = OverrideSpec::parse("minecraft:oak_stairs=").unwrap();

        assert_eq!(spec.sources, vec![id("minecraft:oak_stairs")]);
        assert_eq!(spec.target, None);
    }

    #[test]
    fn allows_modded_target() {
        let spec = OverrideSpec::parse("minecraft:stone=create:brass_block").unwrap();

        assert_eq!(spec.target, Some(id("create:brass_block")));
    }

    #[test]
    fn rejects_malformed_specs() {
        assert!(OverrideSpec::parse("minecraft:dirt").is_err());
        assert!(OverrideSpec::parse("=minecraft:stone").is_err());
        assert!(OverrideSpec::parse("a=b=c").is_err());
        assert!(OverrideSpec::parse("dirt,=stone").is_err());
        assert!(OverrideSpec::parse("").is_err());
    }
}
