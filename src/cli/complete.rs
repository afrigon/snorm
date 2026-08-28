use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;

use clap_complete::engine::CompletionCandidate;

use crate::core::mcdata;
use crate::core::schematic;

/// Complete an override spec (`src[,src]=target`): sources from the blocks
/// present in the schematic on the command line, the target from the block
/// registry of the newest extracted minecraft version. Targets are
/// suggestions only; any block id remains valid.
pub fn override_spec_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };

    match current.split_once('=') {
        Some((left, partial)) => {
            let prefix = format!("{left}=");

            candidates(&registry_blocks(), partial, &prefix)
        }
        None => {
            let (prefix, partial) = match current.rsplit_once(',') {
                Some((done, partial)) => (format!("{done},"), partial),
                None => (String::new(), current)
            };

            let blocks = schematic_blocks().unwrap_or_else(registry_blocks);

            candidates(&blocks, partial, &prefix)
        }
    }
}

/// Complete a single block id from the blocks present in the schematic on
/// the command line.
pub fn schematic_block_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(partial) = current.to_str() else {
        return Vec::new();
    };

    let blocks = schematic_blocks().unwrap_or_default();

    candidates(&blocks, partial, "")
}

/// Complete a region name from the schematic on the command line.
pub fn region_name_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(partial) = current.to_str() else {
        return Vec::new();
    };

    let Some(schematic) = load_command_line_schematic() else {
        return Vec::new();
    };

    schematic::region_names(&schematic)
        .into_iter()
        .filter(|name| name.starts_with(partial))
        .map(CompletionCandidate::new)
        .collect()
}

/// Complete the `OLD` part of an `OLD=NEW` rename pair.
pub fn rename_pair_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(partial) = current.to_str() else {
        return Vec::new();
    };

    if partial.contains('=') {
        return Vec::new();
    }

    region_name_completer(current)
        .into_iter()
        .map(|candidate| {
            CompletionCandidate::new(format!("{}=", candidate.get_value().to_string_lossy()))
        })
        .collect()
}

fn candidates(blocks: &[String], partial: &str, prefix: &str) -> Vec<CompletionCandidate> {
    // Block ids complete with or without their namespace; only the implicit
    // `minecraft:` namespace is shortened in the completed value, so modded
    // ids always stay fully qualified.
    let matches = |id: &str| {
        id.starts_with(partial)
            || id
                .split_once(':')
                .is_some_and(|(_, path)| path.starts_with(partial))
    };

    blocks
        .iter()
        .filter(|id| matches(id))
        .map(|id| {
            let id = match (partial.contains(':'), id.strip_prefix("minecraft:")) {
                (false, Some(path)) => path,
                _ => id.as_str()
            };

            CompletionCandidate::new(format!("{prefix}{id}"))
        })
        .collect()
}

fn registry_blocks() -> Vec<String> {
    let Ok(versions) = mcdata::cached_versions() else {
        return Vec::new();
    };

    let Some(newest) = versions.first() else {
        return Vec::new();
    };

    mcdata::block_registry(newest).unwrap_or_default()
}

fn schematic_blocks() -> Option<Vec<String>> {
    let schematic = load_command_line_schematic()?;

    let mut blocks: Vec<String> = schematic
        .regions
        .iter()
        .flat_map(|region| region.block_palette())
        .map(|state| state.name.to_string())
        .filter(|name| name != "minecraft:air")
        .collect();

    blocks.sort();
    blocks.dedup();

    Some(blocks)
}

/// During dynamic completion the full command line is passed through the
/// process arguments, so the schematic being completed against is whichever
/// existing `.litematic` path appears in them.
fn load_command_line_schematic() -> Option<schematic::Schematic> {
    let path = env::args_os()
        .map(PathBuf::from)
        .find(|arg| arg.extension().is_some_and(|e| e == "litematic") && arg.is_file())?;

    schematic::load(&path).ok()
}
