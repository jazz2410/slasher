//! The opposing spartan. Same body as the player — only the hand on the
//! controls differs, so this module just fills in [`Intent`].

use bevy::prelude::*;

use crate::character::{spawn_character, Attacking, CharacterSet, Facing, Intent, Kinds};
use crate::player::Player;

const START_X: f32 = 180.0;
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
        app.add_systems(Startup, spawn_enemy)
            .add_systems(Update, think.before(CharacterSet::Control));
    }
}

#[derive(Component)]
pub struct Enemy;

#[derive(Component)]
struct Brain {
    cooldown: Timer,
}

fn spawn_enemy(mut commands: Commands, kinds: Res<Kinds>) {
    let enemy = spawn_character(&mut commands, &kinds.spartan, "Enemy", START_X, -1.0, TINT);
    commands.entity(enemy).insert((
        Enemy,
        Brain {
            cooldown: Timer::from_seconds(ATTACK_COOLDOWN, TimerMode::Once),
        },
    ));
}

fn think(
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(&Transform, &mut Intent, &mut Facing, &mut Brain, Option<&Attacking>), With<Enemy>>,
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
