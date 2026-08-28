use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use directories::ProjectDirs;
use serde::Deserialize;
use serde::Serialize;

use crate::utils::errors::SnormResult;

/// Identity of one extracted Minecraft version in the data cache, stored as
/// `manifest.json` next to the extracted files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManifest {
    pub id: String,
    pub data_version: i32
}

#[derive(Debug, Clone)]
pub struct CachedVersion {
    pub manifest: CacheManifest,
    pub path: PathBuf
}

pub fn cache_root() -> SnormResult<PathBuf> {
    let Some(dirs) = ProjectDirs::from("", "", "snorm") else {
        bail!("could not determine the data directory");
    };

    Ok(dirs.data_dir().join("mcdata"))
}

pub fn version_dir(id: &str) -> SnormResult<PathBuf> {
    Ok(cache_root()?.join(id))
}

pub fn cached_versions() -> SnormResult<Vec<CachedVersion>> {
    let root = cache_root()?;

    let mut versions = Vec::new();

    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(versions),
        Err(e) => {
            return Err(e).with_context(|| format!("could not read '{}'", root.display()));
        }
    };

    for entry in entries {
        let entry = entry?;
        let manifest_path = entry.path().join("manifest.json");

        let Ok(contents) = fs::read_to_string(&manifest_path) else {
            continue;
        };

        let manifest: CacheManifest = serde_json::from_str(&contents)
            .with_context(|| format!("could not parse '{}'", manifest_path.display()))?;

        versions.push(CachedVersion {
            manifest,
            path: entry.path()
        });
    }

    versions.sort_by(|a, b| b.manifest.data_version.cmp(&a.manifest.data_version));

    Ok(versions)
}

/// Find the best cached version for a schematic: the oldest cached version
/// that is at least the schematic's data version, or the newest cached
/// version if none are.
pub fn best_cached_version(data_version: i32) -> SnormResult<Option<CachedVersion>> {
    let versions = cached_versions()?;

    let best = versions
        .iter()
        .filter(|v| v.manifest.data_version >= data_version)
        .min_by_key(|v| v.manifest.data_version)
        .or(versions.first())
        .cloned();

    Ok(best)
}

pub fn cached_version(id: &str) -> SnormResult<Option<CachedVersion>> {
    Ok(cached_versions()?.into_iter().find(|v| v.manifest.id == id))
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockInfo {
    #[serde(default)]
    pub definition: BlockDefinition,

    /// State property schema: property name to its allowed values.
    #[serde(default)]
    pub properties: HashMap<String, Vec<String>>
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BlockDefinition {
    #[serde(rename = "type")]
    pub kind: Option<String>
}

/// Block data extracted from a Minecraft jar: state property schemas and
/// definition types from the blocks report, the block registry, and the
/// vanilla block tags (with nested tag references resolved).
///
/// [`McData::empty`] models the degraded mode used when no data has been
/// extracted: every lookup misses and detection falls back to
/// version-independent layers.
#[derive(Debug)]
pub struct McData {
    manifest: Option<CacheManifest>,
    blocks: HashMap<String, BlockInfo>,
    tags: HashMap<String, HashSet<String>>
}

impl McData {
    pub fn empty() -> McData {
        McData {
            manifest: None,
            blocks: HashMap::new(),
            tags: HashMap::new()
        }
    }

    /// Load the cached version best matching a schematic data version, or the
    /// degraded [`McData::empty`] when the cache is empty.
    pub fn load_best(data_version: i32) -> SnormResult<McData> {
        match best_cached_version(data_version)? {
            Some(version) => McData::load(&version),
            None => Ok(McData::empty())
        }
    }

    pub fn load(version: &CachedVersion) -> SnormResult<McData> {
        let blocks_path = version.path.join("blocks.json");
        let blocks_json = fs::read_to_string(&blocks_path)
            .with_context(|| format!("could not read '{}'", blocks_path.display()))?;

        let blocks: HashMap<String, BlockInfo> = serde_json::from_str(&blocks_json)
            .with_context(|| format!("could not parse '{}'", blocks_path.display()))?;

        let tags = load_tags(&version.path.join("tags"))?;

        Ok(McData {
            manifest: Some(version.manifest.clone()),
            blocks,
            tags
        })
    }

    #[cfg(test)]
    pub fn for_tests(
        blocks: HashMap<String, BlockInfo>,
        tags: HashMap<String, HashSet<String>>
    ) -> McData {
        McData {
            manifest: None,
            blocks,
            tags
        }
    }

    pub fn manifest(&self) -> Option<&CacheManifest> {
        self.manifest.as_ref()
    }

    pub fn is_degraded(&self) -> bool {
        self.manifest.is_none()
    }

    pub fn block(&self, id: &str) -> Option<&BlockInfo> {
        self.blocks.get(id)
    }

    pub fn tag_contains(&self, tag: &str, id: &str) -> bool {
        self.tags.get(tag).is_some_and(|values| values.contains(id))
    }
}

/// Load only the block id registry of a cached version; much cheaper than a
/// full [`McData::load`].
pub fn block_registry(version: &CachedVersion) -> SnormResult<Vec<String>> {
    let registries_path = version.path.join("registries.json");
    let registries_json = fs::read_to_string(&registries_path)
        .with_context(|| format!("could not read '{}'", registries_path.display()))?;

    #[derive(Deserialize)]
    struct Registry {
        entries: HashMap<String, serde_json::Value>
    }

    let mut registries: HashMap<String, Registry> = serde_json::from_str(&registries_json)
        .with_context(|| format!("could not parse '{}'", registries_path.display()))?;

    let mut block_ids: Vec<String> = registries
        .remove("minecraft:block")
        .map(|registry| registry.entries.into_keys().collect())
        .unwrap_or_default();

    block_ids.sort();

    Ok(block_ids)
}

fn load_tags(dir: &Path) -> SnormResult<HashMap<String, HashSet<String>>> {
    let mut raw = HashMap::new();
    collect_raw_tags(dir, dir, &mut raw)?;

    let mut resolved = HashMap::new();

    for name in raw.keys() {
        let mut values = HashSet::new();
        let mut visiting = HashSet::new();
        resolve_tag(name, &raw, &mut values, &mut visiting);
        resolved.insert(name.clone(), values);
    }

    Ok(resolved)
}

fn collect_raw_tags(
    root: &Path,
    dir: &Path,
    raw: &mut HashMap<String, Vec<String>>
) -> SnormResult<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("could not read '{}'", dir.display()))
    };

    for entry in entries {
        let path = entry?.path();

        if path.is_dir() {
            collect_raw_tags(root, &path, raw)?;
            continue;
        }

        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }

        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };

        let name = relative
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");

        #[derive(Deserialize)]
        struct Tag {
            values: Vec<serde_json::Value>
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("could not read '{}'", path.display()))?;

        let tag: Tag = serde_json::from_str(&contents)
            .with_context(|| format!("could not parse '{}'", path.display()))?;

        let values = tag
            .values
            .into_iter()
            .filter_map(|value| match value {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Object(mut o) => match o.remove("id") {
                    Some(serde_json::Value::String(s)) => Some(s),
                    _ => None
                },
                _ => None
            })
            .collect();

        raw.insert(name, values);
    }

    Ok(())
}

fn resolve_tag(
    name: &str,
    raw: &HashMap<String, Vec<String>>,
    values: &mut HashSet<String>,
    visiting: &mut HashSet<String>
) {
    if !visiting.insert(String::from(name)) {
        return;
    }

    let Some(entries) = raw.get(name) else {
        return;
    };

    for entry in entries {
        match entry.strip_prefix("#minecraft:") {
            Some(nested) => resolve_tag(nested, raw, values, visiting),
            None if entry.starts_with('#') => {}
            None => {
                values.insert(entry.clone());
            }
        }
    }
}
