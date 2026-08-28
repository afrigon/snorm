use std::env;
use std::fs;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use anyhow::bail;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use sha1::Digest;
use sha1::Sha1;
use zip::ZipArchive;
use zip::result::ZipError;

use crate::context::SnormContext;
use crate::core::mcdata;
use crate::core::mcdata::CacheManifest;
use crate::core::palette;
use crate::utils::errors::SnormResult;

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

const USER_AGENT: &str = concat!(
    "afrigon/",
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION")
);

pub struct DataExtractOptions {
    pub mc_version: Option<String>,
    pub jar: Option<PathBuf>,
    pub force: bool
}

#[derive(Deserialize)]
struct VersionManifest {
    latest: LatestVersions,
    versions: Vec<ManifestVersion>
}

#[derive(Deserialize)]
struct LatestVersions {
    release: String
}

#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    url: String
}

#[derive(Deserialize)]
struct VersionDetails {
    downloads: VersionDownloads
}

#[derive(Deserialize)]
struct VersionDownloads {
    server: Option<DownloadInfo>
}

#[derive(Deserialize)]
struct DownloadInfo {
    url: String,
    sha1: String,
    size: u64
}

#[derive(Deserialize)]
struct JarVersionInfo {
    id: String,
    world_version: i32
}

pub fn extract(context: &mut SnormContext, options: &DataExtractOptions) -> SnormResult<()> {
    let workdir = tempfile::tempdir().context("could not create a temporary directory")?;

    let config_jar = match &options.jar {
        Some(_) => None,
        None => palette::discover(&context.cwd, None)?.and_then(|(palette, _)| palette.jar)
    };

    let jar_path = match options.jar.clone().or(config_jar) {
        Some(path) => path,
        None => {
            let (id, server) = resolve_server_download(options.mc_version.as_deref())?;

            if mcdata::version_dir(&id)?.join("manifest.json").exists() && !options.force {
                context.shell().status(
                    "Skipped",
                    format!("minecraft {id} is already extracted (use --force to redo)")
                )?;

                return Ok(());
            }

            download_server_jar(context, &id, &server, workdir.path())?
        }
    };

    let (mut data_zip, bundler) = open_data_archive(&jar_path)?;

    let version_json = zip_entry(&mut data_zip, "version.json")?
        .with_context(|| format!("no version.json in '{}'", jar_path.display()))?;

    let version: JarVersionInfo = serde_json::from_slice(&version_json)
        .with_context(|| format!("could not parse version.json in '{}'", jar_path.display()))?;

    let cache_dir = mcdata::version_dir(&version.id)?;

    if cache_dir.join("manifest.json").exists() && !options.force {
        context.shell().status(
            "Skipped",
            format!(
                "minecraft {} is already extracted (use --force to redo)",
                version.id
            )
        )?;

        return Ok(());
    }

    let cache_root = mcdata::cache_root()?;
    fs::create_dir_all(&cache_root)
        .with_context(|| format!("could not create '{}'", cache_root.display()))?;

    let stage = tempfile::tempdir_in(&cache_root)
        .context("could not create a staging directory in the data cache")?;

    let tag_count = extract_tags(&mut data_zip, &stage.path().join("tags"))?;

    context
        .shell()
        .status("Extracted", format!("{tag_count} block tags"))?;

    let java = find_java()?;

    context.shell().status(
        "Running",
        "minecraft data generator (this may take a minute)"
    )?;

    run_datagen(&java, &jar_path, bundler, workdir.path())?;

    for name in ["blocks.json", "registries.json"] {
        let report = workdir.path().join("generated/reports").join(name);

        fs::copy(&report, stage.path().join(name))
            .with_context(|| format!("data generator did not produce '{name}'"))?;
    }

    let manifest = CacheManifest {
        id: version.id.clone(),
        data_version: version.world_version
    };

    fs::write(
        stage.path().join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?
    )?;

    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)
            .with_context(|| format!("could not clear '{}'", cache_dir.display()))?;
    }

    let staged = stage.keep();
    fs::rename(&staged, &cache_dir)
        .with_context(|| format!("could not move extracted data to '{}'", cache_dir.display()))?;

    context.shell().status(
        "Finished",
        format!(
            "minecraft {} (data version {}) cached at {}",
            version.id,
            version.world_version,
            cache_dir.display()
        )
    )?;

    Ok(())
}

fn get_json<T: DeserializeOwned>(url: &str) -> SnormResult<T> {
    let mut response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("could not fetch '{url}'"))?;

    let body = response
        .body_mut()
        .read_to_string()
        .with_context(|| format!("could not read the response of '{url}'"))?;

    serde_json::from_str(&body).with_context(|| format!("could not parse the response of '{url}'"))
}

fn resolve_server_download(version: Option<&str>) -> SnormResult<(String, DownloadInfo)> {
    let manifest: VersionManifest = get_json(VERSION_MANIFEST_URL)?;

    let id = version.unwrap_or(&manifest.latest.release);

    let Some(entry) = manifest.versions.iter().find(|v| v.id == id) else {
        bail!("minecraft version '{id}' does not exist");
    };

    let details: VersionDetails = get_json(&entry.url)?;

    let Some(server) = details.downloads.server else {
        bail!("minecraft {id} does not provide a server jar");
    };

    Ok((String::from(id), server))
}

fn download_server_jar(
    context: &mut SnormContext,
    id: &str,
    server: &DownloadInfo,
    dir: &Path
) -> SnormResult<PathBuf> {
    context.shell().status(
        "Downloading",
        format!(
            "minecraft {id} server jar ({} MiB)",
            server.size / (1024 * 1024)
        )
    )?;

    let jar_path = dir.join("server.jar");

    let mut response = ureq::get(&server.url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("could not download '{}'", server.url))?;

    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(&jar_path)
        .with_context(|| format!("could not create '{}'", jar_path.display()))?;

    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("could not download '{}'", server.url))?;

        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])?;
    }

    let digest = hex::encode(hasher.finalize());

    if digest != server.sha1 {
        bail!(
            "checksum mismatch for the minecraft {id} server jar (expected {}, got {digest})",
            server.sha1
        );
    }

    Ok(jar_path)
}

/// Open the archive containing `version.json` and the vanilla datapack. For
/// bundler-style server jars (1.18+) that is the inner versioned jar, for
/// client jars and older server jars it is the jar itself.
fn open_data_archive(jar_path: &Path) -> SnormResult<(ZipArchive<Cursor<Vec<u8>>>, bool)> {
    let bytes =
        fs::read(jar_path).with_context(|| format!("could not read '{}'", jar_path.display()))?;

    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .with_context(|| format!("'{}' is not a valid jar", jar_path.display()))?;

    let Some(versions_list) = zip_entry(&mut zip, "META-INF/versions.list")? else {
        return Ok((zip, false));
    };

    let versions_list = String::from_utf8(versions_list)
        .with_context(|| format!("invalid versions.list in '{}'", jar_path.display()))?;

    let inner_path = versions_list
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .with_context(|| format!("empty versions.list in '{}'", jar_path.display()))?;

    let inner_path = format!("META-INF/versions/{inner_path}");

    let inner = zip_entry(&mut zip, &inner_path)?
        .with_context(|| format!("missing '{inner_path}' in '{}'", jar_path.display()))?;

    let inner_zip = ZipArchive::new(Cursor::new(inner))
        .with_context(|| format!("invalid bundled jar in '{}'", jar_path.display()))?;

    Ok((inner_zip, true))
}

fn zip_entry<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    name: &str
) -> SnormResult<Option<Vec<u8>>> {
    match zip.by_name(name) {
        Ok(mut entry) => {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;

            Ok(Some(bytes))
        }
        Err(ZipError::FileNotFound) => Ok(None),
        Err(e) => Err(e.into())
    }
}

/// Copy every block tag of the vanilla datapack into `dest`, preserving
/// subdirectories such as `mineable/`. Handles both the `tags/block/` (1.21+)
/// and `tags/blocks/` (older) layouts.
fn extract_tags<R: Read + std::io::Seek>(
    zip: &mut ZipArchive<R>,
    dest: &Path
) -> SnormResult<usize> {
    const PREFIXES: [&str; 2] = ["data/minecraft/tags/block/", "data/minecraft/tags/blocks/"];

    let names: Vec<String> = zip
        .file_names()
        .filter(|name| {
            !name.ends_with('/') && PREFIXES.iter().any(|prefix| name.starts_with(prefix))
        })
        .map(String::from)
        .collect();

    if names.is_empty() {
        bail!("no block tags found in the jar");
    }

    for name in &names {
        let relative = PREFIXES
            .iter()
            .find_map(|prefix| name.strip_prefix(prefix))
            .unwrap_or(name);

        let path = dest.join(relative);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let bytes = zip_entry(zip, name)?.unwrap_or_default();
        fs::write(&path, bytes)?;
    }

    Ok(names.len())
}

fn run_datagen(java: &Path, jar: &Path, bundler: bool, workdir: &Path) -> SnormResult<()> {
    let mut command = Command::new(java);

    if bundler {
        command
            .arg("-DbundlerMainClass=net.minecraft.data.Main")
            .arg("-jar")
            .arg(jar);
    } else {
        command.arg("-cp").arg(jar).arg("net.minecraft.data.Main");
    }

    command.arg("--reports").current_dir(workdir);

    let output = command
        .output()
        .with_context(|| format!("could not run '{}'", java.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(10).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();

        let hint = if bundler {
            ""
        } else {
            "\nnote: client jars are not self-contained; \
             omit --jar to download a matching server jar instead"
        };

        bail!(
            "the minecraft data generator failed ({}):\n{}{hint}",
            output.status,
            tail.join("\n")
        );
    }

    Ok(())
}

fn find_java() -> SnormResult<PathBuf> {
    let binary = if cfg!(windows) { "java.exe" } else { "java" };

    if let Some(home) = env::var_os("JAVA_HOME") {
        let candidate = Path::new(&home).join("bin").join(binary);

        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let candidate = dir.join(binary);

            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Some(dirs) = directories::BaseDirs::new() {
        let launcher_roots = [
            dirs.data_dir().join("PrismLauncher/java"),
            dirs.home_dir().join(".minecraft/runtime")
        ];

        for root in launcher_roots {
            if let Some(candidate) = find_java_under(&root, binary, 4) {
                return Ok(candidate);
            }
        }
    }

    bail!(
        "could not find a java installation \
         (set JAVA_HOME or add java to PATH; java is required to run the minecraft data generator)"
    );
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);

        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }

        writer.finish().unwrap();
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(&mut buffer);

        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }

        writer.finish().unwrap();

        buffer.into_inner()
    }

    const VERSION_JSON: &[u8] = br#"{"id": "1.0.0", "world_version": 42}"#;

    #[test]
    fn opens_plain_jars_directly() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("plain.jar");

        write_zip(&jar, &[("version.json", VERSION_JSON)]);

        let (mut zip, bundler) = open_data_archive(&jar).unwrap();

        assert!(!bundler);
        assert!(zip_entry(&mut zip, "version.json").unwrap().is_some());
    }

    #[test]
    fn opens_the_inner_jar_of_bundlers() {
        let inner = zip_bytes(&[
            ("version.json", VERSION_JSON),
            (
                "data/minecraft/tags/block/stairs.json",
                br#"{"values": []}"#
            )
        ]);

        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("bundler.jar");

        write_zip(
            &jar,
            &[
                (
                    "META-INF/versions.list",
                    b"abc123\t1.0.0\t1.0.0/server-1.0.0.jar"
                ),
                ("META-INF/versions/1.0.0/server-1.0.0.jar", &inner)
            ]
        );

        let (mut zip, bundler) = open_data_archive(&jar).unwrap();

        assert!(bundler);

        let version = zip_entry(&mut zip, "version.json").unwrap().unwrap();
        let info: JarVersionInfo = serde_json::from_slice(&version).unwrap();
        assert_eq!(info.id, "1.0.0");
        assert_eq!(info.world_version, 42);

        let dest = dir.path().join("tags");
        let count = extract_tags(&mut zip, &dest).unwrap();
        assert_eq!(count, 1);
        assert!(dest.join("stairs.json").is_file());
    }
}

fn find_java_under(root: &Path, binary: &str, depth: usize) -> Option<PathBuf> {
    let candidate = root.join("bin").join(binary);

    if candidate.is_file() {
        return Some(candidate);
    }

    if depth == 0 {
        return None;
    }

    let entries = fs::read_dir(root).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir()
            && let Some(found) = find_java_under(&path, binary, depth - 1)
        {
            return Some(found);
        }
    }

    None
}
