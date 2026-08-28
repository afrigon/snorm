# snorm

A command line tool that normalizes [Litematica](https://litematica.org/)
schematics against a preferred block palette. It rewrites blocks by category
(all stairs become your stair, all terracotta becomes your color, ...) while
preserving every block state: stair orientation, pane connections,
waterlogging, slab type.

## Quick start

```sh
# extract block data from the official minecraft server jar (downloads it)
snorm data extract

# describe your palette
cat > snorm.toml << 'EOF'
[palette]
stair = "minecraft:stone_brick_stairs"
slab = "minecraft:stone_brick_slab"
wall = "minecraft:stone_brick_wall"
terracotta = "minecraft:white_glazed_terracotta"
EOF

# see what a schematic contains
snorm inspect build.litematic

# preview, then normalize
snorm normalize build.litematic --dry-run
snorm normalize build.litematic
```

Every change is reported per region:

```
 Normalizing build.litematic (2 regions, minecraft 26.2 data)
      Region "tower" 32x64x32
              minecraft:oak_stairs             -> minecraft:stone_brick_stairs  412
              minecraft:cyan_glazed_terracotta -> minecraft:white_glazed_terracotta  12
      Region "moat" 48x8x12
              (no changes)
     Summary 424 of 58210 blocks replaced across 2 regions
    Finished wrote build.normalized.litematic
```

## Palette configuration

Looked up as `--palette PATH`, then `./snorm.toml`, then
`~/.config/snorm/palette.toml`. All keys are optional; a missing category
passes through untouched.

```toml
[palette]
solid = "minecraft:stone_bricks"
glass = "minecraft:glass"
glass_pane = "minecraft:glass_pane"
terracotta = "minecraft:white_glazed_terracotta"  # glazed terracotta only
wall = "minecraft:stone_brick_wall"
stair = "minecraft:stone_brick_stairs"
slab = "minecraft:stone_brick_slab"
coral = "tube"                        # a coral family, not a block id

# standing solid selection; when set, normalize replaces exactly these
# blocks without asking
[categories.solid]
members = []

# never replaced by any category; overrides still apply
[categories.protected]
members = [
    "minecraft:obsidian",
    "minecraft:bedrock",
    "minecraft:packed_ice",
    "minecraft:blue_ice",
    "minecraft:reinforced_deepslate",
    "#minecraft:beacon_base_blocks",
]

# per-block exceptions; an empty target keeps the block unchanged
[overrides]
"minecraft:oak_stairs" = ""
"minecraft:mossy_cobblestone" = "minecraft:stone_bricks"
"minecraft:glowstone" = "minecraft:sea_lantern"
```

The solid category is selection based, because nothing in the game data
separates a build's main material from special-purpose blocks like
obsidian. With a `solid` target configured, normalize ranks the candidate
building blocks in the schematic by how often they appear and asks which
ones to replace; nothing is replaced without an explicit choice:

```
select the block(s) to normalize as solid:
   1. minecraft:stone     41230
   2. minecraft:andesite    892
solid [numbers or block ids, empty skips]:
```

Candidates are blocks whose class in the game's blocks report is the
featureless `minecraft:block` with no state properties; anything with
gameplay behavior has its own class and never qualifies (redstone
components, slime and honey, falling blocks, ice, ores, copper,
spawners). To skip the prompt: pass `--solid <BLOCK>` (repeatable), or set
`[categories.solid] members` as a standing selection. Without a terminal
the solid step is skipped.

Blocks listed in `[categories.protected]` are never replaced by any
category (overrides still apply) and never suggested; member lists accept
`#minecraft:...` vanilla tag references, resolved from the extracted game
data.

Coral is a family x shape matrix, so its palette value is a family name:
`dead_brain_coral_fan` with `coral = "tube"` becomes `dead_tube_coral_fan`.

## Overrides

Command line overrides beat configuration overrides, which beat categories:

```sh
# redirect one or more blocks
snorm normalize build.litematic -o minecraft:dirt,minecraft:grass_block=minecraft:stone

# exempt a block from normalization
snorm normalize build.litematic -o minecraft:oak_stairs=
```

Targets are not validated, so modded block ids work.

## Regions

Litematica selection areas are regions; each is normalized and reported
separately.

```sh
snorm region list build.litematic
snorm region rename build.litematic tower=keep --out renamed.litematic
snorm normalize build.litematic --region tower --rename-region moat=pond
```

## Minecraft data

Category membership (which blocks are stairs, terracotta colors, ...) and
block state validation come from the game itself, so new Minecraft versions
only need a re-extraction, never a new snorm binary:

```sh
snorm data extract                     # latest release
snorm data extract --mc-version 1.21.5 # a specific version
snorm data extract --jar server.jar    # a local jar, no download
snorm data status
```

`data extract` downloads the official server jar from Mojang (checksum
verified), copies the vanilla block tags out of it, and runs the official
data generator, which requires a Java installation. Results are cached under
the user data directory (`~/.local/share/snorm/mcdata` on Linux); when
normalizing, the cached version closest to the schematic's data version is
used.

Without extracted data snorm still works: detection falls back to block
state signatures and name patterns, and property validation is skipped.

Modded blocks are never an error. They are normalized when their state
signature or name reveals the category (a modded block with
`facing`/`half`/`shape` properties is a stair), and pass through otherwise.

## Tab completion

```sh
snorm completions fish   # prints: COMPLETE=fish snorm | source
```

Completions are dynamic: override sources complete from the blocks actually
present in the schematic on your command line, targets from the extracted
block registry, and region flags from the schematic's region names.
