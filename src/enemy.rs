//! Enemy controllers and projectiles. Each LDtk marker chooses a character
//! blueprint; the standard enemy closes for a thrust, while the archer keeps
//! his distance and looses arrows.

use bevy::prelude::*;

use crate::character::{
    spawn_character, Attacking, Blocking, CharacterSet, Dying, Facing, Intent, Kinds, Velocity,
};
use crate::combat::{guard_against, AttackDamage, CombatSet, Health, Hurt, Hurtbox};
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
const ARCHER_MARKERS: [&str; 2] = ["Archer", "ArcherSpawn"];
/// The supplied enemy art already has distinct black armour and purple cloth,
/// so preserve its authored colours rather than multiplying in another tint.
const TINT: Color = Color::WHITE;

/// Beyond this he ignores you.
const SIGHT_RANGE: f32 = 420.0;
/// Inside this he stops walking and starts thrusting. A little under the
/// spear's reach so his attacks actually connect.
const ENGAGE_RANGE: f32 = 44.0;
/// Breathing room between thrusts, so he is beatable.
const ATTACK_COOLDOWN: f32 = 1.1;

const ARCHER_SIGHT_RANGE: f32 = 560.0;
const ARCHER_SHOOT_RANGE: f32 = 420.0;
const ARCHER_RETREAT_RANGE: f32 = 130.0;
const ARCHER_COOLDOWN: f32 = 1.6;
const ARCHER_ARROW_SPEED: f32 = 360.0;
const ARCHER_ARROW_OFFSET: Vec2 = Vec2::new(32.0, 5.0);
const ARCHER_ARROW_PATH: &str = "weapons/archer_arrow.png";
const ARCHER_ARROW_SIZE: Vec2 = Vec2::new(40.0, 7.0);
const ARCHER_ARROW_HITBOX: Vec2 = Vec2::new(14.0, 5.0);

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Run::Playing), spawn_enemies).add_systems(
            Update,
            (
                (standard_think, archer_think).before(CharacterSet::Control),
                loose_archer_arrows
                    .after(CharacterSet::Control)
                    .before(CharacterSet::Physics),
                fly_archer_arrows
                    .after(CharacterSet::Physics)
                    .before(CombatSet::Death),
            )
                .run_if(in_state(Run::Playing)),
        );
    }
}

#[derive(Component)]
pub struct EnemyStandard;

#[derive(Component)]
pub struct Archer;

#[derive(Component)]
struct Brain {
    cooldown: Timer,
}

#[derive(Component)]
struct ArcherBrain {
    cooldown: Timer,
    loosed_this_attack: bool,
}

#[derive(Component)]
struct ArcherArrow {
    velocity: Vec2,
    damage: f32,
}

fn spawn_enemies(mut commands: Commands, kinds: Res<Kinds>, level: Option<Res<Level>>) {
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

    let archer_placements: Vec<Vec2> = match level.as_deref() {
        Some(level) => ARCHER_MARKERS
            .iter()
            .flat_map(|name| level.all_spawns(name))
            .collect(),
        None => Vec::new(),
    };

    for feet in archer_placements {
        let archer = spawn_character(
            &mut commands,
            &kinds.archer,
            "Archer",
            feet,
            -1.0,
            Color::WHITE,
        );
        commands.entity(archer).insert((
            Archer,
            run_scoped(),
            ArcherBrain {
                cooldown: Timer::from_seconds(ARCHER_COOLDOWN, TimerMode::Once),
                loosed_this_attack: false,
            },
        ));
    }
}

fn standard_think(
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

fn archer_think(
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<Archer>)>,
    mut archers: Query<
        (
            &Transform,
            &mut Intent,
            &mut Facing,
            &mut ArcherBrain,
            Option<&Attacking>,
        ),
        (With<Archer>, Without<Dying>),
    >,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    for (transform, mut intent, mut facing, mut brain, attacking) in &mut archers {
        brain.cooldown.tick(time.delta());
        *intent = Intent::default();

        let offset = player_transform.translation.x - transform.translation.x;
        let distance = offset.abs();
        if attacking.is_none() {
            brain.loosed_this_attack = false;
            if offset != 0.0 {
                facing.0 = offset.signum();
            }
        }

        if distance > ARCHER_SIGHT_RANGE || attacking.is_some() {
            continue;
        }
        if distance < ARCHER_RETREAT_RANGE {
            intent.direction = -offset.signum();
            // Turn and run instead of moonwalking away while still aiming at
            // the player. He faces the player again as soon as he stops.
            facing.0 = intent.direction;
        } else if distance > ARCHER_SHOOT_RANGE {
            intent.direction = offset.signum();
        } else if brain.cooldown.is_finished() {
            intent.attack_pressed = true;
            brain.cooldown.reset();
        }
    }
}

fn loose_archer_arrows(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut archers: Query<
        (
            &Attacking,
            &Transform,
            &Facing,
            &AttackDamage,
            &mut ArcherBrain,
        ),
        (With<Archer>, Without<Dying>),
    >,
) {
    for (attack, transform, facing, damage, mut brain) in &mut archers {
        if !attack.is_active() || brain.loosed_this_attack {
            continue;
        }
        brain.loosed_this_attack = true;
        let from = transform.translation.truncate()
            + Vec2::new(facing.0 * ARCHER_ARROW_OFFSET.x, ARCHER_ARROW_OFFSET.y);
        commands.spawn((
            Name::new("Archer Arrow"),
            ArcherArrow {
                velocity: Vec2::new(facing.0 * ARCHER_ARROW_SPEED, 0.0),
                damage: damage.0,
            },
            run_scoped(),
            Sprite {
                image: asset_server.load(ARCHER_ARROW_PATH),
                custom_size: Some(ARCHER_ARROW_SIZE),
                // The supplied art points right.
                flip_x: facing.0 < 0.0,
                ..default()
            },
            Transform::from_xyz(from.x, from.y, 9.0),
        ));
    }
}

fn fly_archer_arrows(
    mut commands: Commands,
    time: Res<Time>,
    level: Option<Res<Level>>,
    mut arrows: Query<(Entity, &ArcherArrow, &mut Transform), Without<Player>>,
    mut player: Query<
        (
            Entity,
            &Transform,
            &Hurtbox,
            &Facing,
            Option<&Blocking>,
            &mut Velocity,
            &mut Health,
        ),
        (With<Player>, Without<Dying>, Without<ArcherArrow>),
    >,
) {
    for (arrow_entity, arrow, mut transform) in &mut arrows {
        transform.translation += arrow.velocity.extend(0.0) * time.delta_secs();
        let at = transform.translation.truncate();

        if let Some(level) = level.as_deref() {
            if level.is_solid_at(at) || !level.bounds().contains(at) {
                commands.entity(arrow_entity).despawn();
                continue;
            }
        }

        let Ok((player_entity, player_transform, hurtbox, facing, blocking, mut velocity, mut health)) =
            player.single_mut()
        else {
            continue;
        };
        let arrow_box = Rect::from_center_size(at, ARCHER_ARROW_HITBOX);
        let player_box =
            Rect::from_center_size(player_transform.translation.truncate(), hurtbox.0);
        if arrow_box.intersect(player_box).is_empty() {
            continue;
        }

        let away = arrow.velocity.x.signum();
        let guard = guard_against(blocking, facing.0 * away < 0.0);
        velocity.0.x = away * guard.knockback();
        commands.entity(player_entity).insert(if guard == crate::combat::Guard::Open {
            Hurt::wounded()
        } else {
            Hurt::guarded()
        });
        health.current = (health.current - guard.damage(arrow.damage)).max(0.0);
        commands.entity(arrow_entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{
        AttackStyle, BaseTint, CharacterPlugin, ARCHER_ATTACK_DAMAGE, ARCHER_MAX_HEALTH,
    };
    use crate::combat::{AttackHitbox, CombatPlugin};
    use crate::level::SpawnPoint;
    use crate::player::PlayerPlugin;
    use crate::run::RunPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const DT: f32 = 1.0 / 60.0;

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

    fn archers(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<Archer>>()
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

    #[test]
    fn an_archer_marker_spawns_the_ranged_enemy_only() {
        let mut app = app_with_level(vec![SpawnPoint {
            identifier: "Archer".into(),
            at: Vec2::new(200.0, 0.0),
        }]);

        assert_eq!(archers(&mut app), 1);
        assert_eq!(enemies(&mut app), 0);
        let (health, damage, style, tint, sprite) = app
            .world_mut()
            .query_filtered::<
                (&Health, &AttackDamage, &AttackStyle, &BaseTint, &Sprite),
                With<Archer>,
            >()
            .single(app.world())
            .unwrap();
        assert_eq!(health.max, ARCHER_MAX_HEALTH);
        assert_eq!(damage.0, ARCHER_ATTACK_DAMAGE);
        assert_eq!(*style, AttackStyle::Ranged);
        assert_eq!(tint.0, Color::WHITE);
        assert_eq!(sprite.color, Color::WHITE);
    }

    #[test]
    fn a_retreating_archer_faces_the_way_he_runs() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, archer_think);
        app.world_mut().spawn((
            Player,
            Transform::from_xyz(0.0, 30.0, 10.0),
        ));
        app.world_mut().spawn((
            Archer,
            Transform::from_xyz(100.0, 30.0, 10.0),
            Intent::default(),
            Facing(-1.0),
            ArcherBrain {
                cooldown: Timer::from_seconds(ARCHER_COOLDOWN, TimerMode::Once),
                loosed_this_attack: false,
            },
        ));

        app.update();

        let (intent, facing) = app
            .world_mut()
            .query_filtered::<(&Intent, &Facing), With<Archer>>()
            .single(app.world())
            .unwrap();
        assert!(intent.direction > 0.0, "he should retreat away to the right");
        assert_eq!(
            facing.0, intent.direction,
            "the walk animation should face the retreat direction"
        );
    }

    #[test]
    fn the_archer_looses_an_arrow_instead_of_a_melee_hitbox() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Image>()
            .init_asset::<TextureAtlasLayout>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                DT,
            )))
            .add_plugins(bevy::state::app::StatesPlugin)
            .insert_resource(Level::with_spawns(vec![
                SpawnPoint {
                    identifier: "PlayerSpawn".into(),
                    at: Vec2::ZERO,
                },
                SpawnPoint {
                    identifier: "Archer".into(),
                    at: Vec2::new(200.0, 0.0),
                },
            ]))
            .add_plugins((
                CharacterPlugin,
                CombatPlugin,
                PlayerPlugin,
                EnemyPlugin,
                RunPlugin,
            ));
        app.update();
        app.update();

        let mut saw_arrow = false;
        for _ in 0..240 {
            app.update();
            saw_arrow |= app
                .world_mut()
                .query_filtered::<Entity, With<ArcherArrow>>()
                .iter(app.world())
                .next()
                .is_some();
            assert_eq!(
                app.world_mut()
                    .query_filtered::<Entity, With<AttackHitbox>>()
                    .iter(app.world())
                    .count(),
                0,
                "a ranged attack must not create a spear hitbox"
            );
        }

        assert!(saw_arrow, "the bow animation never released its projectile");
        let player_health = app
            .world_mut()
            .query_filtered::<&Health, With<Player>>()
            .single(app.world())
            .unwrap();
        assert!(
            player_health.current < player_health.max,
            "the archer's projectile never damaged the player"
        );
    }
}
