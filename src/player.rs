//! The player spartan — keyboard and mouse translated into [`Intent`].

use bevy::prelude::*;

use crate::character::{spawn_character, Attacking, CharacterSet, Facing, Intent, Kinds};
use crate::level::Level;
use crate::world::GROUND_Y;

const START_X: f32 = -140.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(Update, read_input.before(CharacterSet::Control));
    }
}

#[derive(Component)]
pub struct Player;

fn spawn_player(mut commands: Commands, kinds: Res<Kinds>, level: Option<Res<Level>>) {
    // The level decides where he starts; the constant is only a fallback for
    // when no level is loaded, which is how the tests run.
    let feet = match level.as_deref() {
        // A level with no PlayerSpawn still has ground to stand on.
        Some(level) => level.spawn("PlayerSpawn").unwrap_or_else(|| level.default_spawn()),
        None => Vec2::new(START_X, GROUND_Y),
    };

    let player = spawn_character(&mut commands, &kinds.spartan, "Player", feet, 1.0, Color::WHITE);
    commands.entity(player).insert(Player);
}

fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut query: Query<(&mut Intent, &mut Facing, Option<&Attacking>), With<Player>>,
) {
    let left = keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]);
    let right = keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]);
    let jump_keys = [KeyCode::Space, KeyCode::KeyW, KeyCode::ArrowUp];

    for (mut intent, mut facing, attacking) in &mut query {
        intent.direction = (right as i32 - left as i32) as f32;
        intent.jump_pressed = keys.any_just_pressed(jump_keys);
        intent.jump_held = keys.any_pressed(jump_keys);
        intent.attack_pressed =
            keys.any_just_pressed([KeyCode::KeyJ]) || mouse.just_pressed(MouseButton::Left);
        intent.block_held =
            keys.any_pressed([KeyCode::KeyK]) || mouse.pressed(MouseButton::Right);

        // Turning mid-thrust would swing the spear through the character, so
        // facing locks for the duration once committed.
        if attacking.is_none() && intent.direction != 0.0 {
            facing.0 = intent.direction.signum();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{
        Blocking, CharacterPlugin, Reach, SPARTAN_BODY, SPARTAN_HURTBOX, SPARTAN_STATS,
    };

    /// The spartan's own numbers, so the tests move with the blueprint.
    const ATTACK_DURATION: f32 = SPARTAN_STATS.attack.total();
    use crate::combat::{CombatPlugin, Hurt};
    use crate::enemy::EnemyPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const DT: f32 = 1.0 / 60.0;

    fn test_app() -> App {
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
            .add_plugins((CharacterPlugin, CombatPlugin, PlayerPlugin));
        app.update();
        app
    }

    /// The full game, so the enemy exists and can be struck.
    fn duel_app() -> App {
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
            .add_plugins((CharacterPlugin, CombatPlugin, PlayerPlugin, EnemyPlugin));
        app.update();
        app
    }

    fn hitbox_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<crate::combat::AttackHitbox>>()
            .iter(app.world())
            .count()
    }

    fn is_attacking(app: &mut App) -> bool {
        app.world_mut()
            .query_filtered::<Entity, (With<Attacking>, With<Player>)>()
            .iter(app.world())
            .next()
            .is_some()
    }

    fn is_blocking(app: &mut App) -> bool {
        app.world_mut()
            .query_filtered::<Entity, (With<Blocking>, With<Player>)>()
            .iter(app.world())
            .next()
            .is_some()
    }

    fn player_x(app: &mut App) -> f32 {
        app.world_mut()
            .query_filtered::<&Transform, With<Player>>()
            .single(app.world())
            .unwrap()
            .translation
            .x
    }

    fn set_key(app: &mut App, key: KeyCode, down: bool) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        if down {
            input.press(key);
        } else {
            input.release(key);
        }
    }

    /// One discrete press-and-release, costing one frame.
    ///
    /// The release matters: `ButtonInput::press` only raises `just_pressed` for
    /// a button that was not already held, so tapping without releasing first
    /// silently produces no further input events.
    fn tap_attack(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyJ);
        app.update();
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.release(KeyCode::KeyJ);
        input.clear();
    }

    #[test]
    fn attack_runs_through_its_phases_and_ends() {
        let mut app = test_app();
        assert!(!is_attacking(&mut app));

        tap_attack(&mut app);
        assert!(is_attacking(&mut app), "tapping J starts a thrust");

        let mut elapsed = DT;
        let mut hitbox_seen = false;

        while elapsed < ATTACK_DURATION * 2.0 {
            let count = hitbox_count(&mut app);
            assert!(count <= 1, "at most one hitbox at a time, saw {count}");
            if count == 1 {
                hitbox_seen = true;
                assert!(
                    elapsed >= SPARTAN_STATS.attack.startup,
                    "hitbox live at {elapsed}s, before startup ends"
                );
                assert!(
                    elapsed
                        < SPARTAN_STATS.attack.startup + SPARTAN_STATS.attack.active + DT,
                    "hitbox still live at {elapsed}s, past the active window"
                );
            }
            app.update();
            elapsed += DT;
        }

        assert!(hitbox_seen, "the active window never produced a hitbox");
        assert!(!is_attacking(&mut app), "attack should have ended");
        assert_eq!(hitbox_count(&mut app), 0, "hitbox outlived the attack");
    }

    #[test]
    fn attack_cannot_be_retriggered_mid_swing() {
        let mut app = test_app();

        tap_attack(&mut app);
        for _ in 0..4 {
            tap_attack(&mut app);
        }
        let mut frames = 5;
        assert!(is_attacking(&mut app), "still mid-thrust after mashing");

        let deadline = (ATTACK_DURATION / DT).ceil() as usize + 1;
        while frames < deadline {
            app.update();
            frames += 1;
        }

        assert!(
            !is_attacking(&mut app),
            "attack still running {frames} frames in — mashing restarted its clock"
        );
    }

    #[test]
    fn shield_tracks_the_key() {
        let mut app = test_app();
        assert!(!is_blocking(&mut app));

        set_key(&mut app, KeyCode::KeyK, true);
        app.update();
        assert!(is_blocking(&mut app), "holding K raises the shield");

        app.update();
        assert!(is_blocking(&mut app));

        set_key(&mut app, KeyCode::KeyK, false);
        app.update();
        assert!(!is_blocking(&mut app), "releasing K drops the shield");
    }

    #[test]
    fn cannot_attack_while_blocking() {
        let mut app = test_app();
        set_key(&mut app, KeyCode::KeyK, true);
        app.update();
        assert!(is_blocking(&mut app));

        tap_attack(&mut app);
        assert!(
            !is_attacking(&mut app),
            "guard should refuse the thrust while the shield is up"
        );

        set_key(&mut app, KeyCode::KeyK, false);
        app.update();
        tap_attack(&mut app);
        assert!(is_attacking(&mut app), "attack works once the guard is down");
    }

    #[test]
    fn blocking_roots_movement() {
        let mut app = test_app();

        set_key(&mut app, KeyCode::KeyD, true);
        let before_walk = player_x(&mut app);
        for _ in 0..5 {
            app.update();
        }
        assert!(player_x(&mut app) > before_walk, "D should walk him right");

        set_key(&mut app, KeyCode::KeyK, true);
        app.update();
        let planted = player_x(&mut app);
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(
            player_x(&mut app),
            planted,
            "the shield should root him even with D held"
        );
    }

    #[test]
    fn facing_locks_while_attacking() {
        let mut app = test_app();

        set_key(&mut app, KeyCode::KeyA, true);
        app.update();
        set_key(&mut app, KeyCode::KeyA, false);

        let before = app
            .world_mut()
            .query_filtered::<&Facing, With<Player>>()
            .single(app.world())
            .unwrap()
            .0;
        assert_eq!(before, -1.0, "holding A should face left");

        tap_attack(&mut app);
        set_key(&mut app, KeyCode::KeyD, true);
        app.update();

        let during = app
            .world_mut()
            .query_filtered::<&Facing, With<Player>>()
            .single(app.world())
            .unwrap()
            .0;
        assert_eq!(during, -1.0, "facing must not flip mid-thrust");
    }

    // ---- combat ----

    fn hurt_of<T: Component>(app: &mut App) -> Option<bool> {
        app.world_mut()
            .query_filtered::<&Hurt, With<T>>()
            .iter(app.world())
            .next()
            .map(|h| h.blocked)
    }

    #[derive(Component)]
    struct Dummy;

    fn x_of<T: Component>(app: &mut App) -> f32 {
        app.world_mut()
            .query_filtered::<&Transform, With<T>>()
            .single(app.world())
            .unwrap()
            .translation
            .x
    }

    /// A stationary target, so reach is measured against the spear rather than
    /// against however far the enemy happened to walk mid-thrust.
    fn spawn_dummy(app: &mut App, x: f32) -> Entity {
        app.world_mut()
            .spawn((
                Dummy,
                crate::character::Character,
                Transform::from_xyz(x, crate::world::GROUND_Y + SPARTAN_BODY.half_height, 10.0),
                crate::character::Velocity::default(),
                crate::character::Grounded(true),
                Facing(-1.0),
                crate::character::BaseTint(Color::WHITE),
                crate::combat::Hurtbox(SPARTAN_HURTBOX),
                Intent::default(),
                // Without these `apply_physics` skips the dummy entirely, and
                // its knockback would never decay.
                SPARTAN_STATS,
                SPARTAN_BODY,
            ))
            .id()
    }

    /// Thrust from inside spear range; the target should flinch.
    #[test]
    fn thrust_reaches_a_target_in_front() {
        let mut app = test_app();
        let ahead = x_of::<Player>(&mut app) + 35.0;
        spawn_dummy(&mut app, ahead);
        tap_attack(&mut app);

        let mut struck = false;
        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 2 {
            if hurt_of::<Dummy>(&mut app).is_some() {
                struck = true;
                break;
            }
            app.update();
        }
        assert!(struck, "a thrust at 35px never reached the target");
    }

    #[test]
    fn thrust_falls_short_of_a_distant_target() {
        let mut app = test_app();
        let far = x_of::<Player>(&mut app) + 120.0;
        spawn_dummy(&mut app, far);
        tap_attack(&mut app);

        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 2 {
            assert!(
                hurt_of::<Dummy>(&mut app).is_none(),
                "a thrust at 120px should fall well short"
            );
            app.update();
        }
    }

    fn velocity_x(app: &App, entity: Entity) -> f32 {
        app.world()
            .get::<crate::character::Velocity>(entity)
            .unwrap()
            .0
            .x
    }

    /// The active window spans several frames, so the hitbox has to remember
    /// who it already hit. Knockback is the observable: a second hit would
    /// slam the velocity back up to full instead of letting it decay.
    #[test]
    fn a_thrust_strikes_a_target_only_once() {
        let mut app = test_app();
        let ahead = x_of::<Player>(&mut app) + 35.0;
        let dummy = spawn_dummy(&mut app, ahead);
        tap_attack(&mut app);

        let mut knockback = Vec::new();
        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 2 {
            app.update();
            if hurt_of::<Dummy>(&mut app).is_some() {
                knockback.push(velocity_x(&app, dummy));
            }
        }

        assert!(knockback.len() >= 3, "never struck: {knockback:?}");
        for pair in knockback.windows(2) {
            // Strictly decaying, not merely non-increasing: a hit landing
            // every frame would re-set the velocity to a constant, which a
            // `<=` check would happily accept.
            assert!(
                pair[1] < pair[0] - 0.5,
                "knockback stopped decaying ({knockback:?}) — the thrust hit more than once"
            );
        }
    }

    /// The point of the blueprint refactor: a fighter's reach travels on the
    /// entity, so a longer-speared enemy can exist beside a spartan. Before it,
    /// reach was a module constant and this was impossible to express.
    #[test]
    fn reach_belongs_to_the_fighter_not_the_module() {
        // Comfortably past a spartan's spear *plus* the ~17px his lunge
        // carries him during the active window.
        let far = 100.0;

        // With the spartan's own reach, this distance is out of range.
        let mut app = test_app();
        let x = x_of::<Player>(&mut app) + far;
        spawn_dummy(&mut app, x);
        tap_attack(&mut app);
        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 2 {
            assert!(
                hurt_of::<Dummy>(&mut app).is_none(),
                "{far}px should be beyond a spartan's spear"
            );
            app.update();
        }

        // Hand the same fighter a longer weapon and it lands, with no change to
        // any system.
        let mut app = test_app();
        let x = x_of::<Player>(&mut app) + far;
        spawn_dummy(&mut app, x);
        let player = app
            .world_mut()
            .query_filtered::<Entity, With<Player>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(player).insert(Reach {
            size: Vec2::new(80.0, 14.0),
            forward: 60.0,
            vertical: -2.0,
        });

        tap_attack(&mut app);
        let mut struck = false;
        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 2 {
            if hurt_of::<Dummy>(&mut app).is_some() {
                struck = true;
                break;
            }
            app.update();
        }
        assert!(struck, "a longer reach should have covered {far}px");
    }

    #[test]
    fn a_thrust_does_not_wound_its_owner() {
        let mut app = test_app();
        tap_attack(&mut app);

        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 2 {
            assert!(
                hurt_of::<Player>(&mut app).is_none(),
                "the spartan speared himself"
            );
            app.update();
        }
    }

    /// The enemy closes and attacks on his own, so the player takes a hit.
    #[test]
    fn the_enemy_can_hurt_the_player() {
        let mut app = duel_app();
        let mut struck = false;
        for _ in 0..300 {
            app.update();
            if hurt_of::<Player>(&mut app).is_some() {
                struck = true;
                break;
            }
        }
        assert!(struck, "the enemy never landed a hit on the player");
    }

    /// Guarding into the enemy's thrust should spark, not wound.
    #[test]
    fn guarding_turns_the_hit_aside() {
        let mut app = duel_app();
        set_key(&mut app, KeyCode::KeyK, true);

        let mut outcome = None;
        for _ in 0..300 {
            app.update();
            if let Some(blocked) = hurt_of::<Player>(&mut app) {
                outcome = Some(blocked);
                break;
            }
        }
        assert_eq!(
            outcome,
            Some(true),
            "a hit taken behind a raised shield should register as blocked"
        );
    }
}
