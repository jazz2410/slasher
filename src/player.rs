//! The player spartan — keyboard and mouse translated into [`Intent`].

use bevy::prelude::*;

use crate::character::{spawn_character, Attacking, CharacterSet, Dying, Facing, Intent, Kinds};
use crate::level::Level;
use crate::run::{run_scoped, Run};
use crate::world::GROUND_Y;

const START_X: f32 = -140.0;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Run::Playing), spawn_player)
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

    let player = spawn_character(&mut commands, &kinds.player, "Player", feet, 1.0, Color::WHITE);
    commands.entity(player).insert((Player, run_scoped()));
}

fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut query: Query<
        (&mut Intent, &mut Facing, Option<&Attacking>),
        (With<Player>, Without<Dying>),
    >,
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
        Blocking, CharacterPlugin, Dying, Reach, DEATH_DURATION, ENEMY_STANDARD_ATTACK_DAMAGE,
        ENEMY_STANDARD_CLIPS, ENEMY_STANDARD_MAX_HEALTH, PLAYER_ATTACK_DAMAGE,
        PLAYER_CLIPS, PLAYER_MAX_HEALTH, PLAYER_STATS, SPARTAN_BODY, SPARTAN_HURTBOX,
    };

    /// The spartan's own numbers, so the tests move with the blueprint.
    const ATTACK_DURATION: f32 = PLAYER_STATS.attack.total();
    use crate::combat::{
        AttackDamage, CombatPlugin, Health, Hurt, CHIP_DAMAGE, MAX_HEALTH, SPEAR_DAMAGE,
    };
    use crate::enemy::EnemyPlugin;
    use crate::run::{Run, RunPlugin};
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
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins((CharacterPlugin, CombatPlugin, PlayerPlugin, RunPlugin));
        // Two ticks: the first enters Run::Playing, the second lets the spawns
        // it queued actually exist.
        app.update();
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
            .add_plugins(bevy::state::app::StatesPlugin)
            .add_plugins((CharacterPlugin, CombatPlugin, PlayerPlugin, EnemyPlugin, RunPlugin));
        app.update();
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
                    elapsed >= PLAYER_STATS.attack.startup,
                    "hitbox live at {elapsed}s, before startup ends"
                );
                assert!(
                    elapsed
                        < PLAYER_STATS.attack.startup + PLAYER_STATS.attack.active + DT,
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
                PLAYER_STATS,
                SPARTAN_BODY,
                Health::full(MAX_HEALTH),
            ))
            .id()
    }

    fn health_of(app: &mut App, entity: Entity) -> Option<f32> {
        app.world().get::<Health>(entity).map(|h| h.current)
    }

    #[test]
    fn player_and_standard_enemy_receive_their_own_combat_values() {
        let mut app = duel_app();
        let (player_health, player_damage) = app
            .world_mut()
            .query_filtered::<(&Health, &AttackDamage), With<Player>>()
            .single(app.world())
            .unwrap();
        assert_eq!(player_health.max, PLAYER_MAX_HEALTH);
        assert_eq!(player_damage.0, PLAYER_ATTACK_DAMAGE);

        let (enemy_health, enemy_damage) = app
            .world_mut()
            .query_filtered::<
                (&Health, &AttackDamage),
                With<crate::enemy::EnemyStandard>,
            >()
            .single(app.world())
            .unwrap();
        assert_eq!(enemy_health.max, ENEMY_STANDARD_MAX_HEALTH);
        assert_eq!(enemy_damage.0, ENEMY_STANDARD_ATTACK_DAMAGE);
    }

    #[test]
    fn player_uses_the_new_art_and_throw_clip_without_changing_the_enemy() {
        let mut app = duel_app();
        let player_image = app
            .world_mut()
            .query_filtered::<&Sprite, With<Player>>()
            .single(app.world())
            .unwrap()
            .image
            .clone();
        let enemy_image = app
            .world_mut()
            .query_filtered::<&Sprite, With<crate::enemy::EnemyStandard>>()
            .single(app.world())
            .unwrap()
            .image
            .clone();

        assert_ne!(player_image, enemy_image);
        let (initial_clip, initial_timer, initial_sprite) = app
            .world_mut()
            .query_filtered::<
                (
                    &crate::animation::AnimationClip,
                    &crate::animation::AnimationTimer,
                    &Sprite,
                ),
                With<Player>,
            >()
            .single(app.world())
            .unwrap();
        assert_eq!(*initial_clip, PLAYER_CLIPS.idle);
        assert_eq!(
            initial_timer.0.duration().as_secs_f32(),
            PLAYER_CLIPS.idle.frame_duration,
            "a fresh player must not run idle on the walk timer"
        );
        assert_eq!(
            initial_sprite.texture_atlas.as_ref().unwrap().index,
            PLAYER_CLIPS.idle.first,
            "a fresh player must begin on the first idle frame"
        );
        assert_eq!((PLAYER_CLIPS.walk.first, PLAYER_CLIPS.walk.last), (0, 5));
        assert_eq!((PLAYER_CLIPS.attack.first, PLAYER_CLIPS.attack.last), (6, 11));
        assert_eq!((PLAYER_CLIPS.cast.first, PLAYER_CLIPS.cast.last), (12, 17));
        assert_eq!((PLAYER_CLIPS.idle.first, PLAYER_CLIPS.idle.last), (18, 23));
        assert_eq!((PLAYER_CLIPS.airborne.first, PLAYER_CLIPS.airborne.last), (24, 29));
        assert!(PLAYER_CLIPS.idle.repeat, "the idle cycle should loop");
        assert!(!PLAYER_CLIPS.airborne.repeat, "the jump cycle should play once");
        assert_eq!((PLAYER_CLIPS.death.first, PLAYER_CLIPS.death.last), (0, 5));
        let thrust_frames = (PLAYER_CLIPS.attack.last - PLAYER_CLIPS.attack.first + 1) as f32;
        assert!(
            PLAYER_STATS.attack.total() >= thrust_frames / 60.0,
            "the attack state must last long enough to display every thrust frame at 60 FPS"
        );
        assert_eq!(
            PLAYER_STATS.lunge_speed, 0.0,
            "the thrust art must stay planted instead of sliding over the ground"
        );
        assert_eq!(
            (ENEMY_STANDARD_CLIPS.attack.first, ENEMY_STANDARD_CLIPS.attack.last),
            (6, 11),
            "EnemyStandard should use its own sword-attack row"
        );
    }

    fn player_health(app: &mut App) -> f32 {
        app.world_mut()
            .query_filtered::<&Health, With<Player>>()
            .single(app.world())
            .unwrap()
            .current
    }

    #[test]
    fn a_clean_hit_costs_health() {
        let mut app = test_app();
        let ahead = x_of::<Player>(&mut app) + 35.0;
        let dummy = spawn_dummy(&mut app, ahead);
        assert_eq!(health_of(&mut app, dummy), Some(MAX_HEALTH));

        tap_attack(&mut app);
        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 2 {
            app.update();
        }

        let left = health_of(&mut app, dummy).expect("dummy should still be alive");
        // Both assertions matter: the first pins the amount, the second catches
        // a zeroed SPEAR_DAMAGE, which would satisfy the first vacuously.
        assert_eq!(
            left,
            MAX_HEALTH - SPEAR_DAMAGE,
            "one thrust should cost exactly one thrust's worth"
        );
        assert!(left < MAX_HEALTH, "a thrust must actually cost something");
    }

    /// Holding the shield down is not an answer: the guard goes stale and the
    /// shock gets through. This is what stops turtling being optimal.
    #[test]
    fn a_stale_guard_still_takes_chip_damage() {
        let mut app = duel_app();
        set_key(&mut app, KeyCode::KeyK, true);

        let mut guarded = false;
        for _ in 0..400 {
            app.update();
            if hurt_of::<Player>(&mut app) == Some(true) {
                guarded = true;
                break;
            }
        }
        assert!(guarded, "the enemy never landed a hit on the raised shield");
        let left = player_health(&mut app);
        assert_eq!(left, MAX_HEALTH - CHIP_DAMAGE, "a stale guard should chip");
        assert!(
            left > MAX_HEALTH - SPEAR_DAMAGE,
            "a stale guard should still beat taking it clean"
        );
    }

    /// A guard raised as the thrust comes in takes nothing and staggers the
    /// attacker — the reward that makes timing worth the risk.
    #[test]
    fn a_fresh_guard_parries_and_staggers_the_attacker() {
        let mut app = duel_app();

        // Wait for the enemy to commit, then raise the shield into it.
        let mut committed = false;
        for _ in 0..400 {
            app.update();
            if app
                .world_mut()
                .query_filtered::<Entity, (With<Attacking>, With<crate::enemy::EnemyStandard>)>()
                .iter(app.world())
                .next()
                .is_some()
            {
                committed = true;
                break;
            }
        }
        assert!(committed, "the enemy never attacked");

        set_key(&mut app, KeyCode::KeyK, true);
        let mut parried = false;
        for _ in 0..40 {
            app.update();
            if hurt_of::<Player>(&mut app).is_some() {
                parried = true;
                break;
            }
        }
        assert!(parried, "the thrust never landed on the fresh guard");
        assert_eq!(
            player_health(&mut app),
            MAX_HEALTH,
            "a parry must cost nothing at all"
        );

        let staggered = app
            .world_mut()
            .query_filtered::<Entity, (With<Hurt>, With<crate::enemy::EnemyStandard>)>()
            .iter(app.world())
            .next()
            .is_some();
        assert!(staggered, "a parry should stagger the attacker");
    }

    /// ...and the same hit unguarded does take health, so the test above is not
    /// passing merely because nothing ever connects.
    #[test]
    fn an_unguarded_hit_from_the_enemy_costs_health() {
        let mut app = duel_app();

        let mut struck = false;
        for _ in 0..400 {
            app.update();
            if hurt_of::<Player>(&mut app) == Some(false) {
                struck = true;
                break;
            }
        }
        assert!(struck, "the enemy never landed a clean hit");
        assert!(
            player_health(&mut app) < MAX_HEALTH,
            "an unguarded thrust should have cost health"
        );
    }

    fn count<T: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<T>>()
            .iter(app.world())
            .count()
    }

    fn run_state(app: &App) -> Run {
        app.world().resource::<State<Run>>().get().clone()
    }

    /// Play it for real: walk into the enemy, let him kill you, and expect the
    /// level back.
    ///
    /// The forced-health tests above all passed while this was broken. Setting
    /// health to zero by hand happens *between* frames, so the death was always
    /// observed; in a real fight the blow lands mid-frame, and the ordering bug
    /// only showed up there. A test has to lose the fight the way a player does.
    #[test]
    fn losing_a_real_fight_restarts_the_level() {
        let mut app = whole_game_app();
        set_key(&mut app, KeyCode::KeyD, true); // walk into him and take it

        let mut died_at = None;
        for frame in 0..600 {
            app.update();
            if died_at.is_none() && count::<Player>(&mut app) == 0 {
                died_at = Some(frame);
            }
        }

        let died_at = died_at.expect("the enemy never managed to kill a passive player");
        assert_eq!(
            run_state(&app),
            Run::Playing,
            "died on frame {died_at} but the run never came back"
        );
        assert_eq!(count::<Player>(&mut app), 1, "the player should be back");
        assert_eq!(
            count::<crate::enemy::EnemyStandard>(&mut app),
            1,
            "so should the enemy"
        );
    }

    /// The full plugin set, as `main.rs` assembles it minus rendering. The
    /// slimmer apps above can miss anything caused by plugin interaction.
    fn whole_game_app() -> App {
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
            .add_plugins((
                crate::world::WorldPlugin,
                crate::run::RunPlugin,
                crate::level::LevelPlugin,
                CharacterPlugin,
                CombatPlugin,
                PlayerPlugin,
                EnemyPlugin,
                crate::animation::AnimationPlugin,
            ));
        app.update();
        app.update();
        app
    }

    #[test]
    fn dying_restarts_the_level_with_the_whole_game_running() {
        let mut app = whole_game_app();
        assert_eq!(count::<Player>(&mut app), 1, "the player should have spawned");
        let original = app
            .world_mut()
            .query_filtered::<Entity, With<Player>>()
            .single(app.world())
            .unwrap();

        {
            let mut query = app.world_mut().query_filtered::<&mut Health, With<Player>>();
            query.single_mut(app.world_mut()).unwrap().current = 0.0;
        }

        let mut saw_animation = false;
        let mut restarted = false;
        for _ in 0..300 {
            app.update();
            saw_animation |= app.world().get::<Dying>(original).is_some();
            if app.world().get_entity(original).is_err()
                && run_state(&app) == Run::Playing
                && count::<Player>(&mut app) == 1
            {
                restarted = true;
                break;
            }
        }
        assert!(saw_animation, "the player skipped the death animation");
        assert!(restarted, "the level never came back (state {:?})", run_state(&app));
        assert_eq!(player_health(&mut app), MAX_HEALTH);
    }

    /// Dying rebuilds the level from data that was never mutated, so a retry is
    /// a despawn and a respawn rather than a reload.
    #[test]
    fn dying_restarts_the_level() {
        let mut app = duel_app();
        let enemies_at_start = count::<crate::enemy::EnemyStandard>(&mut app);
        assert!(enemies_at_start > 0, "the duel should start with an enemy");
        assert_eq!(run_state(&app), Run::Playing);

        // Strike him down.
        {
            let mut query = app.world_mut().query_filtered::<&mut Health, With<Player>>();
            query.single_mut(app.world_mut()).unwrap().current = 0.0;
        }
        // The player remains in the level while all death frames play.
        app.update();
        assert_eq!(run_state(&app), Run::Playing);
        assert_eq!(count::<Player>(&mut app), 1);
        assert_eq!(count::<Dying>(&mut app), 1);

        for _ in 0..(DEATH_DURATION / DT).ceil() as usize + 3 {
            app.update();
            if run_state(&app) == Run::Ended {
                break;
            }
        }
        assert_eq!(run_state(&app), Run::Ended, "death should end the run");
        assert_eq!(count::<Player>(&mut app), 0, "the run should be torn down");
        assert_eq!(count::<crate::enemy::EnemyStandard>(&mut app), 0);

        // ...and comes back.
        for _ in 0..120 {
            app.update();
            if run_state(&app) == Run::Playing && count::<Player>(&mut app) == 1 {
                break;
            }
        }

        assert_eq!(run_state(&app), Run::Playing, "the level should restart");
        assert_eq!(count::<Player>(&mut app), 1, "exactly one player, not two");
        assert_eq!(
            count::<crate::enemy::EnemyStandard>(&mut app),
            enemies_at_start,
            "the enemies should be back, and only once each"
        );
        assert_eq!(
            player_health(&mut app),
            MAX_HEALTH,
            "a restarted level starts at full health"
        );
    }

    /// Restarting repeatedly must not leave anything behind.
    #[test]
    fn repeated_deaths_do_not_accumulate() {
        let mut app = duel_app();
        let enemies = count::<crate::enemy::EnemyStandard>(&mut app);

        for round in 0..3 {
            {
                let mut query = app.world_mut().query_filtered::<&mut Health, With<Player>>();
                query.single_mut(app.world_mut()).unwrap().current = 0.0;
            }
            for _ in 0..120 {
                app.update();
                if run_state(&app) == Run::Playing && count::<Player>(&mut app) == 1 {
                    break;
                }
            }
            assert_eq!(count::<Player>(&mut app), 1, "round {round}: one player");
            assert_eq!(
                count::<crate::enemy::EnemyStandard>(&mut app),
                enemies,
                "round {round}: enemies should not stack up"
            );
        }
    }

    #[test]
    fn a_spent_fighter_remains_as_a_harmless_corpse() {
        let mut app = test_app();
        let ahead = x_of::<Player>(&mut app) + 35.0;
        let dummy = spawn_dummy(&mut app, ahead);

        // One thrust from dead.
        app.world_mut().get_mut::<Health>(dummy).unwrap().current = SPEAR_DAMAGE;

        tap_attack(&mut app);
        for _ in 0..(ATTACK_DURATION / DT).ceil() as usize + 4 {
            app.update();
        }

        assert!(
            app.world().get::<Dying>(dummy).is_some(),
            "a lethal hit should begin the death state"
        );
        for _ in 0..(DEATH_DURATION / DT).ceil() as usize + 3 {
            app.update();
        }

        assert!(
            app.world().get_entity(dummy).is_ok(),
            "a defeated fighter should hold the final death frame"
        );
        assert!(
            app.world().get::<Transform>(dummy).unwrap().translation.z < 10.0,
            "a corpse should render behind living fighters"
        );
        assert!(
            app.world().get::<crate::combat::Hurtbox>(dummy).is_none(),
            "a corpse must no longer be a combat target"
        );
    }

    #[test]
    fn the_enemy_uses_the_death_animation_before_leaving() {
        let mut app = duel_app();
        let enemy = app
            .world_mut()
            .query_filtered::<Entity, With<crate::enemy::EnemyStandard>>()
            .single(app.world())
            .unwrap();
        app.world_mut().get_mut::<Health>(enemy).unwrap().current = 0.0;

        app.update();

        assert!(app.world().get::<Dying>(enemy).is_some());
        assert_eq!(
            *app.world().get::<crate::animation::AnimationClip>(enemy).unwrap(),
            ENEMY_STANDARD_CLIPS.death
        );
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
