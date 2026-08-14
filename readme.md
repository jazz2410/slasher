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

The block is a *held* state rather than a timed one: it remains active while the
key is down and drops the instant it is released. It roots you in place and
locks out attacking and jumping, but you can still turn to face an attacker.

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

Each action has its own PNG rather than sharing one large combat sheet:

- `spartan_walk.png` is a generated 540x64 atlas containing five 108x64 frames
  from the first row of `spartan_walking.png`.
- `spartan_attack_game.png` is a generated 540x192 atlas containing all fifteen
  108x64 frames from `spartan_attack.png`. It plays once over the attack's
  startup, active, and recovery timeline.

The processing scripts crop the poses, normalize their scale and anchor,
preserve transparency, and quantise downsampling noise. Re-run them after
editing either source:

```sh
python3 tools/process_walking_sprite.py
python3 tools/process_attack_sprite.py
```

Frames are horizontally anchored on the body rather than alternating feet, so
the torso stays steady through the walk cycle. Every frame shares a feet
baseline, and the script rejects any frame that clips its cell.

Block, idle, and airborne keep their gameplay behavior but temporarily hold the
first walking pose. Each will switch to its own processed PNG when that source
art is added.

The world uses **1 unit = 1 source pixel**, and the camera is set to a fixed
360-unit viewport height, so the game letterboxes consistently at any window
size. Textures are sampled with `ImagePlugin::default_nearest()` to keep pixel
art sharp.

## Adding an animation

Animations are `AnimationClip { first, last, frame_duration, repeat }` over
atlas indices. To add an animation:

1. Add the action source PNG under `assets/sprites/`.
2. Add a processor that outputs 108x64 cells with the same body/feet anchor.
3. Load that action's image and atlas layout in `spawn_player`.
4. Select its clip and sprite sheet from `update_animation`.

Non-looping clips (`repeat: false`) hold on their last frame, which is what you
want for an attack that returns control on completion.
