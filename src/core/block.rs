use std::fmt;
use std::str::FromStr;

use anyhow::bail;

use crate::utils::errors::SnormResult;

/// A namespaced block identifier such as `minecraft:stone`, stored in its
/// canonical `namespace:path` form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(String);

impl BlockId {
    pub fn parse(input: &str) -> SnormResult<BlockId> {
        let input = input.trim();

        if input.is_empty() {
            bail!("block id is empty");
        }

        let (namespace, path) = match input.split_once(':') {
            Some((namespace, path)) => (namespace, path),
            None => ("minecraft", input)
        };

        if namespace.is_empty() {
            bail!("block id '{input}' has an empty namespace");
        }

        if path.is_empty() {
            bail!("block id '{input}' has an empty path");
        }

        if path.contains(':') {
            bail!("block id '{input}' contains more than one ':'");
        }

        let valid_namespace = namespace
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-.".contains(c));

        if !valid_namespace {
            bail!("block id '{input}' has invalid characters in its namespace");
        }

        let valid_path = path
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./".contains(c));

        if !valid_path {
            bail!("block id '{input}' has invalid characters in its path");
        }

        Ok(BlockId(format!("{namespace}:{path}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn namespace(&self) -> &str {
        self.0
            .split_once(':')
            .map(|(namespace, _)| namespace)
            .unwrap_or_default()
    }

    pub fn path(&self) -> &str {
        self.0
            .split_once(':')
            .map(|(_, path)| path)
            .unwrap_or(&self.0)
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for BlockId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BlockId::parse(s)
    }
}

/// A block state identity usable as a hash map key: block name plus its
/// properties sorted by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockStateKey {
    name: String,
    properties: Vec<(String, String)>
}

impl BlockStateKey {
    pub fn new<I>(name: impl Into<String>, properties: I) -> BlockStateKey
    where
        I: IntoIterator<Item = (String, String)>
    {
        let mut properties: Vec<(String, String)> = properties.into_iter().collect();
        properties.sort();

        BlockStateKey {
            name: name.into(),
            properties
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn properties(&self) -> &[(String, String)] {
        &self.properties
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_namespaced_id() {
        let id = BlockId::parse("minecraft:stone_bricks").unwrap();

        assert_eq!(id.as_str(), "minecraft:stone_bricks");
        assert_eq!(id.namespace(), "minecraft");
        assert_eq!(id.path(), "stone_bricks");
    }

    #[test]
    fn defaults_to_minecraft_namespace() {
        let id = BlockId::parse("dirt").unwrap();

        assert_eq!(id.as_str(), "minecraft:dirt");
    }

    #[test]
    fn accepts_modded_namespaces() {
        let id = BlockId::parse("create:brass_block").unwrap();

        assert_eq!(id.namespace(), "create");
    }

    #[test]
    fn rejects_invalid_ids() {
        assert!(BlockId::parse("").is_err());
        assert!(BlockId::parse("  ").is_err());
        assert!(BlockId::parse(":stone").is_err());
        assert!(BlockId::parse("minecraft:").is_err());
        assert!(BlockId::parse("a:b:c").is_err());
        assert!(BlockId::parse("Minecraft:Stone").is_err());
        assert!(BlockId::parse("minecraft:sto ne").is_err());
    }

    #[test]
    fn state_key_sorts_properties() {
        let a = BlockStateKey::new(
            "minecraft:oak_stairs",
            [
                (String::from("half"), String::from("bottom")),
                (String::from("facing"), String::from("north"))
            ]
        );

        let b = BlockStateKey::new(
            "minecraft:oak_stairs",
            [
                (String::from("facing"), String::from("north")),
                (String::from("half"), String::from("bottom"))
            ]
        );

        assert_eq!(a, b);
    }
}
