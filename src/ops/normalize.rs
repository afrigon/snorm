use std::collections::HashSet;
use std::io::BufRead;
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::bail;

use crate::context::SnormContext;
use crate::core::block::BlockId;
use crate::core::category::Category;
use crate::core::mcdata;
use crate::core::mcdata::McData;
use crate::core::overrides::OverrideSpec;
use crate::core::palette;
use crate::core::palette::Palette;
use crate::core::plan;
use crate::core::plan::ReplacementPlan;
use crate::core::report::ChangeReport;
use crate::core::schematic;
use crate::core::schematic::Schematic;
use crate::ops::OutputTarget;
use crate::utils::errors::SnormResult;
use crate::utils::verbosity::Verbosity;

pub struct NormalizeOptions {
    pub schematic: PathBuf,
    pub palette: Option<PathBuf>,
    pub overrides: Vec<OverrideSpec>,
    pub solids: Vec<BlockId>,
    pub regions: Vec<String>,
    pub renames: Vec<(String, String)>,
    pub output: OutputTarget,
    pub dry_run: bool,
    pub mc_version: Option<String>
}

pub fn normalize(context: &mut SnormContext, options: &NormalizeOptions) -> SnormResult<()> {
    let mut schematic = schematic::load(&options.schematic)?;

    let discovered = palette::discover(&context.cwd.clone(), options.palette.as_deref())?;

    if discovered.is_none() && options.overrides.is_empty() {
        let searched: Vec<String> = palette::search_paths(&context.cwd)
            .iter()
            .map(|path| format!("  {}", path.display()))
            .collect();

        bail!(
            "no palette configuration found and no overrides given\nsearched:\n{}",
            searched.join("\n")
        );
    }

    let palette = discovered.map(|(palette, _)| palette).unwrap_or_default();

    let mcdata = match &options.mc_version {
        Some(id) => match mcdata::cached_version(id)? {
            Some(version) => McData::load(&version)?,
            None => bail!(
                "minecraft {id} is not in the data cache \
                 (run 'snorm data extract --mc-version {id}')"
            )
        },
        None => McData::load_best(schematic.metadata.minecraft_data_version)?
    };

    if mcdata.is_degraded() {
        context.shell().warn(
            "no minecraft data extracted; category detection is limited and \
             block state properties are not validated (run 'snorm data extract')"
        )?;
    }

    for name in &options.regions {
        if !schematic.regions.iter().any(|r| r.name == *name) {
            let available = schematic::region_names(&schematic).join("', '");

            bail!("no region named '{name}' (available: '{available}')");
        }
    }

    let data_note = match mcdata.manifest() {
        Some(manifest) => format!("minecraft {} data", manifest.id),
        None => String::from("no block data")
    };

    context.shell().status(
        "Normalizing",
        format!(
            "{} ({} regions, {data_note})",
            options.schematic.display(),
            schematic.regions.len()
        )
    )?;

    let solid_selection = resolve_solid_selection(context, &schematic, options, &palette, &mcdata)?;

    let mut report = ChangeReport::default();

    for region in schematic.regions.iter_mut() {
        if !options.regions.is_empty() && !options.regions.iter().any(|n| region.name == *n) {
            continue;
        }

        let region_plan = ReplacementPlan::build(
            region.block_palette(),
            &palette,
            &options.overrides,
            &solid_selection,
            &mcdata
        );

        report.regions.push(plan::apply(region, &region_plan));
    }

    render(context, &report)?;

    schematic::rename_regions(&mut schematic, &options.renames)?;

    for (old, new) in &options.renames {
        context
            .shell()
            .status("Renamed", format!("'{old}' -> '{new}'"))?;
    }

    if options.dry_run {
        context.shell().note("dry run: nothing was written")?;

        return Ok(());
    }

    let output = options.output.resolve(&options.schematic);

    schematic::save(&schematic, &output)?;

    context
        .shell()
        .status("Finished", format!("wrote {}", output.display()))?;

    Ok(())
}

/// Decide which blocks the solid category replaces in this run. `--solid`
/// flags win; otherwise standing configuration members suffice; otherwise
/// the ranked candidates are offered interactively, and nothing is replaced
/// without an explicit choice. Without a terminal the solid step is skipped
/// rather than guessed.
fn resolve_solid_selection(
    context: &mut SnormContext,
    schematic: &Schematic,
    options: &NormalizeOptions,
    palette: &Palette,
    mcdata: &McData
) -> SnormResult<HashSet<BlockId>> {
    let target = palette.targets.get(&Category::Solid);

    if !options.solids.is_empty() {
        if target.is_none() {
            bail!("--solid requires a solid target in the palette configuration");
        }

        return Ok(options.solids.iter().cloned().collect());
    }

    if target.is_none() || palette.solid_members.has_entries() {
        return Ok(HashSet::new());
    }

    let regions: Vec<&schematic::SchematicRegion> = schematic
        .regions
        .iter()
        .filter(|region| {
            options.regions.is_empty() || options.regions.iter().any(|n| region.name == *n)
        })
        .collect();

    let candidates = plan::solid_candidates(&regions, palette, &options.overrides, mcdata);

    if candidates.is_empty() {
        return Ok(HashSet::new());
    }

    if !std::io::stdin().is_terminal() {
        context.shell().warn(
            "skipping solid normalization: no terminal to confirm the block selection \
             (pass --solid <BLOCK> or configure [categories.solid] members)"
        )?;

        return Ok(HashSet::new());
    }

    prompt_solid_selection(context, &candidates)
}

fn prompt_solid_selection(
    context: &mut SnormContext,
    candidates: &[(BlockId, u64)]
) -> SnormResult<HashSet<BlockId>> {
    const DISPLAY_LIMIT: usize = 10;

    {
        let mut shell = context.shell();
        let err = shell.err();

        writeln!(err, "select the block(s) to normalize as solid:")?;

        let width = candidates
            .iter()
            .take(DISPLAY_LIMIT)
            .map(|(id, _)| id.as_str().len())
            .max()
            .unwrap_or(0);

        for (i, (id, count)) in candidates.iter().take(DISPLAY_LIMIT).enumerate() {
            writeln!(err, "  {:>2}. {:<width$}  {count}", i + 1, id.as_str())?;
        }

        if candidates.len() > DISPLAY_LIMIT {
            writeln!(
                err,
                "  ... {} more (type a block id instead)",
                candidates.len() - DISPLAY_LIMIT
            )?;
        }
    }

    let stdin = std::io::stdin();

    loop {
        {
            let mut shell = context.shell();
            write!(shell.err(), "solid [numbers or block ids, empty skips]: ")?;
            shell.err().flush()?;
        }

        let mut line = String::new();

        if stdin.lock().read_line(&mut line)? == 0 {
            return Ok(HashSet::new());
        }

        let line = line.trim();

        if line.is_empty() || line.eq_ignore_ascii_case("none") {
            return Ok(HashSet::new());
        }

        let mut selection = HashSet::new();
        let mut valid = true;

        for token in line.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if let Ok(index) = token.parse::<usize>() {
                match candidates.get(index.wrapping_sub(1)) {
                    Some((id, _)) => {
                        selection.insert(id.clone());
                    }
                    None => {
                        drop(
                            context
                                .shell()
                                .error(format!("no candidate number {token}"))
                        );
                        valid = false;
                        break;
                    }
                }

                continue;
            }

            match BlockId::parse(token) {
                Ok(id) => {
                    selection.insert(id);
                }
                Err(e) => {
                    drop(context.shell().error(e));
                    valid = false;
                    break;
                }
            }
        }

        if valid && !selection.is_empty() {
            return Ok(selection);
        }
    }
}

fn render(context: &mut SnormContext, report: &ChangeReport) -> SnormResult<()> {
    for region in &report.regions {
        context.shell().status(
            "Region",
            format!(
                "\"{}\" {}x{}x{}",
                region.name, region.size.0, region.size.1, region.size.2
            )
        )?;

        let width = region
            .replacements
            .keys()
            .map(|(from, _)| from.len())
            .max()
            .unwrap_or(0);

        let mut shell = context.shell();

        if shell.verbosity() != Verbosity::Quiet {
            if region.replacements.is_empty() {
                writeln!(shell.err(), "              (no changes)")?;
            }

            for ((from, to), count) in &region.replacements {
                writeln!(shell.err(), "              {from:<width$} -> {to}  {count}")?;
            }

            if matches!(
                shell.verbosity(),
                Verbosity::Verbose | Verbosity::VeryVerbose
            ) {
                for ((name, category), count) in &region.kept {
                    let category = match category {
                        Some(category) => format!("  [{category}]"),
                        None => String::new()
                    };

                    writeln!(shell.err(), "              kept {name}{category}  {count}")?;
                }
            }
        }

        drop(shell);

        for (name, count) in &region.skipped_block_entities {
            context.shell().warn(format!(
                "\"{}\": skipped {count} {name}: has block entity data \
                 (use an override to replace anyway)",
                region.name
            ))?;
        }

        for warning in &region.warnings {
            context
                .shell()
                .warn(format!("\"{}\": {warning}", region.name))?;
        }
    }

    let warnings = match report.warning_count() {
        0 => String::new(),
        count => format!(", {count} warnings")
    };

    context.shell().status(
        "Summary",
        format!(
            "{} of {} blocks replaced across {} regions{warnings}",
            report.replaced(),
            report.blocks(),
            report.regions.len()
        )
    )?;

    Ok(())
}
