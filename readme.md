# Slasher

A 2D side-scroller with a spartan warrior, built with [Bevy](https://bevy.org) 0.19.

## Running

```sh
cargo run                    # normal build
cargo run --features dev     # dynamic linking, much faster rebuilds
cargo run --release          # for actually playing
```

## Controls

| Action | Keys |
| --- | --- |
| Move | `A` / `D` or `←` / `→` |
| Jump | `Space`, `W`, or `↑` (hold for height) |
| Attack | `J` or left mouse |
| Block | hold `K` or right mouse |
| Pray at a shrine | `E` (standing close) |
| Fire arrow | `L` (once blessed) |

The spear thrust runs on a fixed startup → active → recovery timeline. You are
committed once it starts: no turning, no jumping, and no re-triggering until it
finishes. The translucent red rectangle is the live hitbox, drawn only during
the active window — delete the `Sprite` in `tick_attack` to hide it.

The block is a *held* state rather than a timed one: it remains active while the
key is down and drops the instant it is released. It roots you in place and
locks out attacking and jumping, but you can still turn to face an attacker.

## Combat

Fighters are reused along two axes.

**Behaviour**: the player and the enemy are the same creature.
[src/character.rs](src/character.rs) owns physics, the attack/block state
machine, and animation; the two differ only in who fills `Intent` each frame — a
keyboard for one, a few AI rules for the other. A new enemy *behaviour* is one
system writing `Intent`.

**Data**: a kind of fighter is a `CharacterKind` blueprint — spritesheet, clips,
reach, hurtbox, body, stats. Spawning copies it onto the entity as components,
so two fighters with different art, different spear lengths and different attack
timings coexist without touching a system. A new enemy *kind* is one more entry
in `Kinds`.

The enemy is tinted cold to tell the two apart.

The player, `EnemyStandard`, and `Archer` currently each carry 100 health and
deal 25 damage, but those values live in separate profiles so any of them can
be tuned without changing the others. The archer keeps his distance and looses
blockable arrows instead of creating a melee hitbox.

| Guard | Health | What happens |
| --- | --- | --- |
| **Open** — no shield, or facing away | **-25** | Red flash, hard knockback, 0.34s hitstun |
| **Stale** — shield up over 0.18s | **-8** | Gold flash, light shove |
| **Parried** — shield raised within 0.18s | **none** | Attacker staggered for 0.5s |

Blocking is a timing test, not a state. Raise the shield as the thrust comes in
and you take nothing *and* leave the attacker open long enough to punish. Hold it
down and the guard goes stale — it still turns the blade, but the shock gets
through. Without that decay, turtling would be the correct answer to every
attack, which is the same as having no fight at all.

At zero health an enemy holds the final death frame as a harmless corpse. When
the **player** dies the level restarts: after a 0.8s pause everything the run
owns is cleared and rebuilt.

The shield only covers the side you face, so turning your back means eating the
thrust. One thrust can strike a given target only once, however many frames its
active window spans.

## The shrine

Praying at a shrine puts the player **in a state**: while blessed, `L` looses a
fire arrow that flies flat and kills the first body it touches. Without the
state the button does nothing.

| Source | Charge |
| --- | --- |
| `FireShrine` entity, prayed at with `E` | **Single** — loosing the arrow ends the state |
| `EternalFlame` entity anywhere in the level | **Endless** — never spent, 2.5s cooldown |

So a shrine is one arrow: spend it and you must find another shrine. A level
carrying an `EternalFlame` marker instead leaves the player permanently able to
cast — the marker's position is irrelevant, its presence is the whole statement.

Either way the state is scoped to the run, so dying costs it. While blessed the
spartan is tinted with the god's fire; because that tint is applied to his
*base* colour, a damage flash still restores to it and the state survives being
hit.

Nothing about it is level-specific: `level.rs` reads spawn markers by name, so
placing shrines is editor work with no code change.

## Layout

| Path | Purpose |
| --- | --- |
| [src/main.rs](src/main.rs) | App setup, window config, plugin registration |
| [src/character.rs](src/character.rs) | Shared body: physics, attack/block state, animation |
| [src/player.rs](src/player.rs) | Keyboard and mouse into `Intent` |
| [src/enemy.rs](src/enemy.rs) | AI into `Intent` |
| [src/combat.rs](src/combat.rs) | Hit resolution, guards, health, knockback |
| [src/run.rs](src/run.rs) | Playing / dying / restarting a level |
| [src/dev_menu.rs](src/dev_menu.rs) | F1 developer level picker |
| [src/shrine.rs](src/shrine.rs) | Shrine, blessing, fire arrow |
| [src/animation.rs](src/animation.rs) | Reusable spritesheet animation driver |
| [src/camera.rs](src/camera.rs) | Follow camera with fixed vertical framing |
| [src/world.rs](src/world.rs) | Placeholder ground and pillars |

Each module is a Bevy `Plugin`, so adding a system means adding it to that
module's `build` rather than touching `main.rs`.

## Runs and restarting

A level's *data* — collision grid, spawn points, art — is loaded once and never
mutated, so a restart is not a reload. It is a despawn of everything the run
owns followed by a respawn from that data, which is what makes a retry instant.

Everything belonging to a run carries `DespawnOnExit(Run::Playing)`, so leaving
that state clears the board with no bookkeeping. Anything that should survive a
death — the camera, the loaded level, the character blueprints — simply does not
carry it. Adding a level transition later means changing *which* level is loaded
on entering `Playing`, not adding more states.

`Run` starts in `Loading` rather than `Playing` for a specific reason:
`bevy_state` inserts the first `StateTransition` **before `PreStartup`**, so a
default of `Playing` would fire `OnEnter` before the level and blueprints exist.
`PostStartup` steps out of `Loading` once startup has run.

## Levels

Levels are authored in [LDtk](https://ldtk.io) (free desktop editor, 1.5.3).
The game loads every level from the first `.ldtk` in `assets/levels` and starts
on the first one. Name the main 16px collision layer `IntGrid`; additional
IntGrid layers such as the 8px `SmallIntgrid` are imported as one-way platforms.
Press **F1** in game to pause and open the developer level picker;
clicking a level clears the current run and rebuilds it from that level.

| Layer type | Purpose |
| --- | --- |
| Main `IntGrid` | Collision. `1` Solid, `2` Platform (one-way), `3` Hazard |
| Additional IntGrid | Every occupied cell is a finer one-way platform. |
| Tiles / AutoLayer | The art. Drawn exactly as painted. |
| Entities | `PlayerSpawn`, `EnemyStandardSpawn`, `Archer`, `FireShrine` |

A level with no painted tiles falls back to deriving terrain from its collision,
so blocked-out geometry is visible before any art is placed. A level with no
`PlayerSpawn` puts the player on the first bit of ground it can find. Melee
enemies spawn from `EnemyStandardSpawn`; ranged enemies spawn from `Archer`.
No marker means no enemy.

Paint collision first and let auto-rules derive the terrain art: you draw the
level's *shape* and the tiles follow.

[src/level.rs](src/level.rs) reads the project directly with `serde_json` rather
than pulling in a tilemap crate — LDtk bakes its *resolved* tile placements into
the saved file, so there is nothing left for such a crate to compute. It builds
the main collision grid plus any finer platform grids, reads spawn points from
the `Entities` layer, and draws the tiles.

Collision is axis-separated: move and resolve horizontally, then vertically.
Doing both at once cannot tell a wall from a floor. With no level loaded — which
is how the tests run — characters fall back to flat ground at `GROUND_Y`.

A level can skip tiles entirely: set a **background image** in LDtk's level
properties and let the IntGrid carry collision on its own. The image is stretched
to the level bounds, so it must be exactly the level's pixel size.

Two tools support that workflow:

```sh
python3 tools/import_level_art.py generated.png --out level2.png --anchor bottom
python3 tools/check_level.py
```

`import_level_art.py` crops a generated image to the level's aspect *then*
scales it — resizing straight to the target would squash the art and shift the
painted ground away from the collision. It reads the target size from the LDtk
project so the two cannot disagree. `--palette` snaps to
`assets/palettes/slasher.gpl`, which forces a shared look across levels
generated in different sessions.

`check_level.py` draws the collision over the art and reports the gap in pixels
between the IntGrid's top edge and the ground line it detects in the picture.
Run it after any art or collision change; it is the only way to catch drift
without playing.

`tools/blockout.py` writes a level's geometry from an ASCII map, which beats
clicking for broad strokes. Close LDtk before running it.

`assets/tiles/village.png` is **generated** by `tools/make_tileset.py` from the
palette — procedural blockout art, not hand-drawn, but every tile sits on the ID
the spec assigns it, so a drawn sheet can replace the file without touching a
level. The game picks nine-slice tiles from the collision grid itself, so a level
renders correctly straight from painted collision with no LDtk rule setup.

The art brief lives in [docs/tileset-spec.md](docs/tileset-spec.md) — a locked
28-colour palette (`assets/palettes/slasher.gpl`, derived from the spartan so the
world cannot clash with him), the exact tile inventory with fixed IDs, and the
lighting approach. `tools/make_ldtk.py` regenerates the project skeleton and
validates it against LDtk's published JSON schema.

## Sprites

The player loads `assets/sprites/player_combat.png` — a 768x480 atlas with
6x5 cells of 128x96. Its first three rows come from
`assets/sprites/spartan_sprites.png`; idle and jump come from
`assets/sprites/spartan_idle_jump.png`.

| Rows | Indices | Player animation |
| --- | --- | --- |
| 0 | 0-5 | Walk |
| 1 | 6-11 | Normal thrust |
| 2 | 12-17 | Throw spear (`L` special) |
| 3 | 18-23 | Idle cycle |
| 4 | 24-29 | Jump cycle |

Death uses `player_dies_game.png`, a separate six-frame row of 128x96 cells.
The importer preserves spear and cape pixels that cross the source image's
nominal 256px divisions, removes its faint alpha backdrop, and puts every pose
on the same feet baseline. To rebuild all player assets:

```sh
python3 tools/build_player_idle_jump.py
python3 tools/build_player_sheet.py
```

The script also saves the four individual action rows as
`player_walk_game.png`, `player_thrust_game.png`, `player_throw_game.png`,
`player_idle_game.png`, `player_jump_game.png`, and `player_dies_game.png`.

`EnemyStandard` uses `assets/sprites/enemy_standard_combat.png`, a 6x2 atlas of
128x96 cells generated from `assets/sprites/enemyStandard.png`. Frame constants live at the top of
[src/character.rs](src/character.rs).

| Rows | Indices | Animation |
| --- | --- | --- |
| 0 | 0-5 | Walk cycle |
| 1 | 6-11 | Sword attack |

The enemy atlas and its separate six-frame death row are **generated**. The
importer removes the presentation background and headings, preserves detached
sword effects, and shares one ground anchor across every pose:

```sh
python3 tools/build_enemy_standard_sheet.py
```

`process_sprite.py` produced that last one from `spartan_firespear.png`:

```sh
python3 tools/process_sprite.py assets/sprites/spartan_firespear.png \
    --out assets/sprites/spartan_firespear_game.png \
    --frames-per-row 5 --cell-width 108 --side-margin 1
```

`--cell-width` forces the result to match the atlas it is joining, and
`--side-margin` buys the last couple of pixels when a longer weapon would
otherwise not fit.

`tools/process_sprite.py` is the older, heavier tool: it rescues art off an
opaque backdrop with no usable grid, keying out the background and re-anchoring
each frame. Use it only when a source is not already game-ready.

The archer uses `archer_combat.png`: five rows of walking followed by five rows
of shooting, for 50 frames in 108x66 cells. His 25 death frames remain in the
separate 108x96 `archer_dies_game.png` atlas. Rebuild those derived files with:

```sh
python3 tools/process_sprite.py assets/sprites/archer_walk.png \
    --out assets/sprites/archer_walk_game.png --frames-per-row 5 \
    --cell-width 108 --cell-height 66 --target-height 54 --baseline-from-top 60
python3 tools/process_sprite.py assets/sprites/archer_shoot.png \
    --out assets/sprites/archer_shoot_game.png --frames-per-row 5 \
    --cell-width 108 --cell-height 66 --target-height 54 --baseline-from-top 60
python3 tools/process_sprite.py assets/sprites/archer_dies.png \
    --out assets/sprites/archer_dies_game.png --frames-per-row 5 \
    --cell-width 108 --cell-height 96 --target-height 49 \
    --baseline-from-top 75 --prone-anchor-ratio 0.63
python3 tools/build_archer_sheet.py
python3 tools/import_weapon.py assets/sprites/arrow.png \
    --out archer_arrow.png --length 40
```

The last command removes the source arrow's black canvas and writes the
transparent projectile used in game to `assets/weapons/archer_arrow.png`.

**There is currently no block animation.** `Clips::block` borrows the thrust's
opening frame as a stand-in; blocking works mechanically, but it looks like a
wind-up rather than a guard.

**`START_BLESSED` in [src/shrine.rs](src/shrine.rs) is off.** Walk to the
highlighted `FireShrine` and press `E` to receive one fire-spear cast.

The world uses **1 unit = 1 source pixel**, and the camera is set to a fixed
360-unit viewport height, so the game letterboxes consistently at any window
size. Textures are sampled with `ImagePlugin::default_nearest()` to keep pixel
art sharp.

## Adding an animation

Animations are `AnimationClip { first, last, frame_duration, repeat }` over
atlas indices. To add an animation:

1. Add the action source PNG under `assets/sprites/`.
2. Add a processor that outputs 108x64 cells with the same body/feet anchor.
3. Load that action's image and atlas layout in the character blueprint.
4. Select its clip and sprite sheet from `update_animation`.

Non-looping clips (`repeat: false`) hold on their last frame, which is what you
want for the death animation while its state timer finishes.
