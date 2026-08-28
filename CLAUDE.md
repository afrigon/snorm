# snorm

CLI tool that rewrites Litematica schematics to match a preferred block
palette, preserving block states. See README.md for user-facing behavior.

## Architecture

`main.rs` -> `SnormContext` + clap `Cli` -> `CliCommand` enum -> command
structs implementing the sync `CommandHandler` trait -> `ops/` functions ->
`core/` logic. Mirrors the structure of the sibling `mc` project, minus
async: there is no tokio; HTTP uses blocking `ureq` and the only subprocess
is `java`.

- `cli/` — clap definitions and dynamic completers. Command structs
  translate arguments into an `*Options` struct and call one `ops` function.
- `ops/` — orchestration: load, call core, print through `Shell`, save.
  `normalize` owns the interactive solid prompt; `inspect` loads the same
  palette configuration so its markers match what normalize would do.
- `core/` — deliberately CLI-free (no `Shell`, no clap, no printing);
  returns data (`ChangeReport`, decisions) so an interactive UI can reuse it.
  - `block.rs` — `BlockId` (resource location, bare ids normalize to the
    `minecraft:` namespace), `BlockStateKey` (name + sorted properties,
    hashable identity of a palette entry).
  - `palette.rs` — `snorm.toml` model, discovery, `MemberSet` (explicit ids
    plus `#minecraft:...` tag references resolved via extracted data).
  - `category.rs` — layered category detection: vanilla tag ->
    blocks-report definition type -> state property signature -> name
    pattern. Later layers cover modded blocks and missing data.
    `is_solid_candidate` is separate from `detect` (see invariants).
  - `mcdata.rs` — extracted game data cache (`manifest.json` per version;
    `McData::empty()` is the degraded mode, never a hard error).
  - `plan.rs` — `ReplacementPlan::build` decides once per palette entry
    (override > protected > category), `apply` walks a region,
    `solid_candidates` ranks prompt candidates by usage.
  - `schematic.rs` — rustmatica load/save helpers, region renaming.
- `utils/` — `Shell` (Cargo-style status output, stderr), `CliError` with
  exit codes, `Verbosity`. Ported from `mc`.

## Key libraries

- `rustmatica` (default features off — the `image`/`chrono` defaults pull a
  large codec stack only to decode preview thumbnails, which round-trip
  fine as raw data) with `mcdata` generic types: block states are name +
  string property map, version independent. The direct `mcdata` dependency
  version must match the one rustmatica depends on, or the types will not
  unify.
- `clap_complete` with `unstable-dynamic`: completion goes through
  `CompleteEnv` in `main` before argument parsing; completers receive the
  partial token and read the full command line from `env::args_os()`.
- `ureq` (blocking) keeps the no-async architecture for the Mojang
  downloads; `sha1`/`hex` verify the published jar checksums.

## Design decisions (do not revisit without asking)

- The override separator is `=` (`-o minecraft:dirt=minecraft:stone`, empty
  target keeps the block). It was `>` originally; shells tokenize `>` as a
  redirection during completion, which breaks right-side completion
  unfixably. `=` matches `--rename-region OLD=NEW` and block ids cannot
  contain it.
- The solid category is selection based, never automatic. Candidates are
  blocks whose blocks-report class is the featureless `minecraft:block`
  (plus `minecraft:terracotta`, a class some versions give dyed terracotta)
  with no state properties; every functional block has its own class and
  never qualifies. Nothing is replaced without an explicit choice: the
  prompt's empty input skips, `--solid` and `[categories.solid] members`
  are the non-interactive paths, no TTY means skip with a warning.
- Pillar-class blocks (deepslate, logs, basalt — `rotated_pillar`, `axis`
  property) are intentionally not solid candidates; users pass
  `--solid minecraft:deepslate` for builds whose main block is one.
- The terracotta category covers glazed terracotta only. Dyed and plain
  terracotta are ordinary solid candidates.
- There is no light category: the game exports no luminance data, so light
  blocks (glowstone -> sea lantern etc.) are handled with `[overrides]`.
- `[categories.protected]` blocks are never replaced or suggested by any
  category; explicit overrides still replace them. Member lists accept
  `#minecraft:...` vanilla tag references only (no other namespaces).
- Minecraft data is never vendored and no local game install is assumed:
  `data extract` downloads the official server jar from Mojang, unzips the
  block tags, and runs the official data generator (requires Java, found
  via PATH/JAVA_HOME/launcher runtimes). Cache lives under the XDG data
  dir keyed by version id; normalize picks the oldest cached version whose
  data version is at least the schematic's.

## Behavior invariants

- Air, cave air, void air, and structure void are never replaced and are
  excluded from the reported block counts ("x of y blocks replaced" counts
  non-air only).
- Blocks with block entity data are only replaced by explicit overrides
  (which also remove the stale block entity); category replacements skip
  them with a warning.
- Properties carried to a replacement are validated against the target's
  schema from blocks.json; invalid ones are dropped with a warning. Without
  extracted data they carry verbatim.
- Coral is normalized by family (`coral = "tube"` in the palette): only the
  family segment of `[dead_]<family>_coral[_block|_fan|_wall_fan]` is
  swapped; shape and dead/alive status always survive.
- Modded blocks never error: they normalize when a signature or name layer
  matches, pass through otherwise. Completion emits modded ids fully
  qualified and only shortens the implicit `minecraft:` namespace.
- Region names live in an NBT compound: order is not preserved across a
  load/save round trip.
- rustmatica's `set_block` appends to the region palette and never garbage
  collects; unused palette entries after normalization are valid and
  harmless, so counting uses `blocks()` iteration, not the palette.

## Conventions

- Panic-free: no `unwrap`/`expect` outside tests; propagate with `?` and
  `.context()`. `SnormResult<T>` internally, `CliResult`/`CliError` at the
  CLI boundary.
- User-facing output goes through `Shell` (status/warn/note, respects
  verbosity); `tracing` `debug!`/`trace!` for diagnostics only.
- Format with `cargo +nightly fmt` (rustfmt.toml uses unstable features);
  keep `cargo clippy` warning-free.
- Tests are in-module `#[cfg(test)]`; the binary has no lib target.
  Checked-in fixtures may contain minecraft data snippets; the "never
  vendor" rule is about runtime data.
- After changes, install with `cargo build --release && cp target/release/snorm ~/bin/snorm`
  — the user runs the installed binary, not `cargo run`.
- The user's palette lives at `~/.config/snorm/palette.toml`; treat it as
  user-owned configuration, not project state.
- Test schematics can be generated with rustmatica through a throwaway
  `examples/` file (generic types need explicit `rustmatica::Litematic` /
  `rustmatica::Region` annotations); remove the file afterwards.
