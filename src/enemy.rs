//! The opposing spartan. Same body as the player — only the hand on the
//! controls differs, so this module just fills in [`Intent`].

use bevy::prelude::*;

use crate::character::{spawn_character, Attacking, CharacterSet, Dying, Facing, Intent, Kinds};
use crate::level::Level;
use crate::player::Player;
use crate::run::{run_scoped, Run};
use crate::world::GROUND_Y;

const START_X: f32 = 180.0;
/// Marker names accepted for an enemy placement, so the level can call it
/// either thing.
const SPAWN_MARKERS: [&str; 2] = ["EnemySpawn", "Enemy"];
/// Both spartans share one spritesheet, so the enemy is tinted cold to keep the
/// two readable at a glance.
const TINT: Color = Color::srgb(0.55, 0.68, 1.0);

/// Beyond this he ignores you.
const SIGHT_RANGE: f32 = 420.0;
/// Inside this he stops walking and starts thrusting. A little under the
/// spear's reach so his attacks actually connect.
const ENGAGE_RANGE: f32 = 44.0;
/// Breathing room between thrusts, so he is beatable.
const ATTACK_COOLDOWN: f32 = 1.1;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Run::Playing), spawn_enemy)
            .add_systems(Update, think.before(CharacterSet::Control));
    }
}

#[derive(Component)]
pub struct Enemy;

#[derive(Component)]
struct Brain {
    cooldown: Timer,
}

fn spawn_enemy(mut commands: Commands, kinds: Res<Kinds>, level: Option<Res<Level>>) {
    // One per Enemy marker in the level, or a single fallback without one.
    let placements: Vec<Vec2> = match level.as_deref() {
        Some(level) => SPAWN_MARKERS
            .iter()
            .flat_map(|name| level.all_spawns(name))
            .collect(),
        None => Vec::new(),
    };
    let placements = if !placements.is_empty() {
        placements
    } else if let Some(level) = level.as_deref() {
        // No Enemy markers: put one on the ground a little way in.
        vec![level.default_spawn() + Vec2::new(120.0, 0.0)]
    } else {
        vec![Vec2::new(START_X, GROUND_Y)]
    };

    for feet in placements {
        let enemy = spawn_character(&mut commands, &kinds.spartan, "Enemy", feet, -1.0, TINT);
        commands.entity(enemy).insert((
            Enemy,
            run_scoped(),
            Brain {
                cooldown: Timer::from_seconds(ATTACK_COOLDOWN, TimerMode::Once),
            },
        ));
    }
}

fn think(
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<Enemy>)>,
    mut enemies: Query<
        (&Transform, &mut Intent, &mut Facing, &mut Brain, Option<&Attacking>),
        (With<Enemy>, Without<Dying>),
    >,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    for (transform, mut intent, mut facing, mut brain, attacking) in &mut enemies {
        brain.cooldown.tick(time.delta());

        let offset = player_transform.translation.x - transform.translation.x;
        let distance = offset.abs();

        // Rebuilt from scratch each frame; nothing here should persist.
        *intent = Intent::default();

        if distance > SIGHT_RANGE {
            continue;
        }

        // Square up to the player, but never mid-thrust — the spear would
        // swing through him.
        if attacking.is_none() && offset != 0.0 {
            facing.0 = offset.signum();
        }

        if distance > ENGAGE_RANGE {
            intent.direction = offset.signum();
        } else if brain.cooldown.is_finished() {
            intent.attack_pressed = true;
            brain.cooldown.reset();
        }
    }
}
