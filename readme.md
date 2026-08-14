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

The spear thrust runs on a fixed startup → active → recovery timeline. You are
committed once it starts: no turning, no jumping, and no re-triggering until it
finishes. The translucent red rectangle is the live hitbox, drawn only during
the active window — delete the `Sprite` in `tick_attack` to hide it.

The block is a *held* state rather than a timed one: the shield goes up while
the key is down and drops the instant it is released. It roots you in place and
locks out attacking and jumping, but you can still turn to face an attacker.
Frames 14-15 of the sheet are the spark-lit impact poses; they stay unused until
there are enemies to be hit by.

## Layout

| Path | Purpose |
| --- | --- |
| [src/main.rs](src/main.rs) | App setup, window config, plugin registration |
| [src/player.rs](src/player.rs) | Input, platformer physics, animation state |
| [src/animation.rs](src/animation.rs) | Reusable spritesheet animation driver |
| [src/camera.rs](src/camera.rs) | Follow camera with fixed vertical framing |
| [src/world.rs](src/world.rs) | Placeholder ground and pillars |

Each module is a Bevy `Plugin`, so adding a system means adding it to that
module's `build` rather than touching `main.rs`.

## Sprites

The game loads `assets/sprites/spartan_combat.png` — a 648x192 sheet, 6x3 cells
of 108x64. Row 0 is the walk cycle (atlas indices 0-5), row 1 the spear thrust
(6-11), row 2 the shield block (12-17). Frame constants live at the top of [src/player.rs](src/player.rs).

That sheet is **generated**, not hand-authored. `tools/process_sprite.py` builds
it from `spartan_sprite_all.png`, an opaque image whose twelve figures sit on
no common grid. The script reads the backdrop colours off the image's own border (a fixed
luminance threshold swallows the character's dark outline, then the flood fill
pours through the hole and shreds the shading inside), crops each figure,
rescales everything by one uniform factor, re-anchors each frame on feet-centre,
and quantises away the resampling noise. Re-run it after editing the source:

```sh
python3 tools/process_sprite.py
```

Frames are anchored on the **torso**, not the feet: feet alternate through a
walk cycle and swing the figure about, and using one anchor for both rows means
the body cannot jump sideways when an attack starts. The script picks the cell
width itself — the anchor sits at the cell's horizontal centre so `flip_x`
mirrors in place, and the spear reaches ~52px forward of it. It asserts no frame clips its cell, and prints the
`FRAME_SIZE` and `HALF_HEIGHT` values to copy into `src/player.rs`.

The world uses **1 unit = 1 source pixel**, and the camera is set to a fixed
360-unit viewport height, so the game letterboxes consistently at any window
size. Textures are sampled with `ImagePlugin::default_nearest()` to keep pixel
art sharp.

## Adding an animation

Animations are `AnimationClip { first, last, frame_duration, repeat }` over
atlas indices. To add an attack:

1. Add the spritesheet to `assets/sprites/`.
2. Build a second `TextureAtlasLayout` for its grid.
3. Define the clip constant and hand it to `play()` from `update_animation`.

Non-looping clips (`repeat: false`) hold on their last frame, which is what you
want for an attack that returns control on completion.
