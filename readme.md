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

There is no health and no bar. A hit reads purely through the reaction:

| Outcome | Feedback |
| --- | --- |
| Clean hit | Red flash, hard knockback, 0.34s of hitstun |
| Guarded hit | Gold flash, light shove, 0.16s |

The shield only covers the side you face, so turning your back means eating the
thrust. One thrust can strike a given target only once, however many frames its
active window spans.

## Layout

| Path | Purpose |
| --- | --- |
| [src/main.rs](src/main.rs) | App setup, window config, plugin registration |
| [src/character.rs](src/character.rs) | Shared body: physics, attack/block state, animation |
| [src/player.rs](src/player.rs) | Keyboard and mouse into `Intent` |
| [src/enemy.rs](src/enemy.rs) | AI into `Intent` |
| [src/combat.rs](src/combat.rs) | Hit resolution, knockback, damage flash |
| [src/animation.rs](src/animation.rs) | Reusable spritesheet animation driver |
| [src/camera.rs](src/camera.rs) | Follow camera with fixed vertical framing |
| [src/world.rs](src/world.rs) | Placeholder ground and pillars |

Each module is a Bevy `Plugin`, so adding a system means adding it to that
module's `build` rather than touching `main.rs`.

## Sprites

The game loads `assets/sprites/spartan_combat.png` — a 540x256 atlas, 5x4 cells
of 108x64. Row 0 is the walk cycle (atlas indices 0-4); rows 1-3 are one
continuous 15-frame spear thrust (5-19). Frame constants live at the top of
[src/character.rs](src/character.rs).

That atlas is **generated**. `tools/build_sheet.py` stacks the already-gridded
source sheets into it, checking they agree on cell size and that every frame is
baselined identically — a drift between sheets shows up in game as the character
popping when the animation changes. Re-run it after replacing either sheet:

```sh
python3 tools/build_sheet.py
```

| Source | Contributes |
| --- | --- |
| `spartan_walk.png` | row 0, indices 0-4 |
| `spartan_attack_game.png` | rows 1-3, indices 5-19 |

`tools/process_sprite.py` is the older, heavier tool: it rescues art off an
opaque backdrop with no usable grid, keying out the background and re-anchoring
each frame. Use it only when a source is not already game-ready.

**There is currently no block animation.** `BLOCK_CLIP` borrows the thrust's
opening frame as a stand-in; blocking works mechanically, but it looks like a
wind-up rather than a guard.

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
