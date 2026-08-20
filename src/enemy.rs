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
const SPAWN_MARKERS: [&str; 4] = [
    "EnemyStandardSpawn",
    "EnemyStandard",
    "EnemySpawn",
    "Enemy",
];
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
pub struct EnemyStandard;

#[derive(Component)]
struct Brain {
    cooldown: Timer,
}

fn spawn_enemy(mut commands: Commands, kinds: Res<Kinds>, level: Option<Res<Level>>) {
    // A real level is authoritative: no marker means no enemy. The fixed
    // fallback exists only for isolated tests that intentionally load no level.
    let placements: Vec<Vec2> = match level.as_deref() {
        Some(level) => SPAWN_MARKERS
            .iter()
            .flat_map(|name| level.all_spawns(name))
            .collect(),
        None => vec![Vec2::new(START_X, GROUND_Y)],
    };

    for feet in placements {
        let enemy = spawn_character(
            &mut commands,
            &kinds.enemy_standard,
            "EnemyStandard",
            feet,
            -1.0,
            TINT,
        );
        commands.entity(enemy).insert((
            EnemyStandard,
            run_scoped(),
            Brain {
                cooldown: Timer::from_seconds(ATTACK_COOLDOWN, TimerMode::Once),
            },
        ));
    }
}

fn think(
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<EnemyStandard>)>,
    mut enemies: Query<
        (&Transform, &mut Intent, &mut Facing, &mut Brain, Option<&Attacking>),
        (With<EnemyStandard>, Without<Dying>),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::CharacterPlugin;
    use crate::level::SpawnPoint;
    use crate::run::RunPlugin;

    fn app_with_level(spawns: Vec<SpawnPoint>) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Image>()
            .init_asset::<TextureAtlasLayout>()
            .add_plugins(bevy::state::app::StatesPlugin)
            .insert_resource(Level::with_spawns(spawns))
            .add_plugins((CharacterPlugin, EnemyPlugin, RunPlugin));
        app.update();
        app.update();
        app
    }

    fn enemies(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<EnemyStandard>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn a_real_level_without_an_enemy_marker_stays_empty() {
        let mut app = app_with_level(Vec::new());
        assert_eq!(enemies(&mut app), 0);
    }

    #[test]
    fn each_standard_enemy_marker_spawns_one_enemy() {
        let mut app = app_with_level(vec![
            SpawnPoint {
                identifier: "EnemyStandardSpawn".into(),
                at: Vec2::new(100.0, 0.0),
            },
            SpawnPoint {
                identifier: "EnemyStandardSpawn".into(),
                at: Vec2::new(200.0, 0.0),
            },
        ]);
        assert_eq!(enemies(&mut app), 2);
    }
}
