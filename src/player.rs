//! The spartan: input, simple platformer physics, and animation state.

use bevy::prelude::*;

use crate::animation::{play, AnimationClip, AnimationTimer};
use crate::world::GROUND_Y;

/// `spartan_combat.png` is 648x192 — a 6x3 grid of 108x64 cells. Row 0 walks,
/// row 1 thrusts, row 2 blocks. The cell is far wider than the character because
/// the torso anchor sits at its horizontal centre (so `flip_x` mirrors in place)
/// while the spear reaches ~52px forward. Regenerate with
/// `tools/process_sprite.py`.
const FRAME_SIZE: UVec2 = UVec2::new(108, 64);
const FRAME_COLUMNS: u32 = 6;
const FRAME_ROWS: u32 = 3;

/// Distance from the sprite's centre down to the character's feet. Every frame
/// is baselined 2px above the cell bottom.
const HALF_HEIGHT: f32 = 30.0;

const RUN_SPEED: f32 = 160.0;
const JUMP_SPEED: f32 = 420.0;
const GRAVITY: f32 = -1200.0;
/// Cutting upward velocity when the jump key is released gives short hops.
const JUMP_CUT_MULTIPLIER: f32 = 0.45;

/// Wind-up before the spear commits — long enough to read, short enough to
/// still feel responsive.
const ATTACK_STARTUP: f32 = 0.09;
/// The window where the hitbox exists and can deal damage.
const ATTACK_ACTIVE: f32 = 0.10;
/// Tail where you are committed and cannot act. This is what makes a whiffed
/// thrust cost something.
const ATTACK_RECOVERY: f32 = 0.13;
const ATTACK_DURATION: f32 = ATTACK_STARTUP + ATTACK_ACTIVE + ATTACK_RECOVERY;
/// Modest forward drive so the thrust travels instead of rooting in place.
const ATTACK_LUNGE_SPEED: f32 = 90.0;

/// Measured off the thrust frames: at full extension the spear tip reaches 50px
/// forward of the sprite's centre and sits ~10px below it.
const HITBOX_SIZE: Vec2 = Vec2::new(40.0, 16.0);
const HITBOX_FORWARD_OFFSET: f32 = 30.0;
const HITBOX_VERTICAL_OFFSET: f32 = -10.0;

const WALK_CLIP: AnimationClip = AnimationClip::new(0, 5, 0.09);
const IDLE_CLIP: AnimationClip = AnimationClip::still(0);
const AIRBORNE_CLIP: AnimationClip = AnimationClip::still(2);
/// Row 1 of the sheet. Frame time is derived from the attack's real duration so
/// the animation can never drift out of sync with the hitbox timing.
const ATTACK_CLIP: AnimationClip = AnimationClip::new(6, 11, ATTACK_DURATION / 6.0);
/// Row 2. Raise the shield and hold it there. Frames 14-15 are the spark-lit
/// impact poses — those belong to taking a hit, so they wait for enemies.
const BLOCK_CLIP: AnimationClip = AnimationClip::once(12, 13, 0.07);

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player).add_systems(
            Update,
            (
                read_input,
                update_block,
                start_attack,
                tick_attack,
                apply_physics,
                update_animation,
            )
                .chain(),
        );
    }
}

#[derive(Component)]
pub struct Player;

#[derive(Component, Default)]
pub struct Velocity(pub Vec2);

#[derive(Component, Default)]
pub struct Grounded(pub bool);

/// Which way the spartan faces: `1.0` right, `-1.0` left. Stored explicitly
/// rather than read back off `Sprite::flip_x` so an attack can aim correctly
/// even when started from a standstill.
#[derive(Component)]
pub struct Facing(pub f32);

/// What the input system decided this frame; physics and animation both read it.
#[derive(Component, Default)]
pub struct Intent {
    direction: f32,
    jump_pressed: bool,
    jump_held: bool,
    attack_pressed: bool,
    block_held: bool,
}

/// Present only while a thrust is in progress. Its own clock drives the phases,
/// so the attack behaves identically with or without finished art.
#[derive(Component)]
pub struct Attacking {
    elapsed: f32,
    hitbox: Option<Entity>,
}

impl Attacking {
    /// The damaging window.
    fn is_active(&self) -> bool {
        self.elapsed >= ATTACK_STARTUP && self.elapsed < ATTACK_STARTUP + ATTACK_ACTIVE
    }

    /// Startup and active combined — the spear is driving forward.
    fn is_driving(&self) -> bool {
        self.elapsed < ATTACK_STARTUP + ATTACK_ACTIVE
    }
}

/// Present while the shield is up. Held rather than timed, unlike `Attacking`.
#[derive(Component)]
pub struct Blocking;

/// Marks the damaging region of a thrust. Enemies will query for this once they
/// exist; for now it is drawn translucent so the reach is visible.
#[derive(Component)]
pub struct AttackHitbox;

fn spawn_player(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        FRAME_SIZE,
        FRAME_COLUMNS,
        FRAME_ROWS,
        None,
        None,
    ));

    commands.spawn((
        Name::new("Player"),
        Player,
        Sprite::from_atlas_image(
            asset_server.load("sprites/spartan_combat.png"),
            TextureAtlas { layout, index: 0 },
        ),
        Transform::from_xyz(0.0, GROUND_Y + HALF_HEIGHT, 10.0),
        Velocity::default(),
        Grounded(true),
        Facing(1.0),
        Intent::default(),
        IDLE_CLIP,
        AnimationTimer::from_clip(&WALK_CLIP),
    ));
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

/// The shield tracks the key directly: raise it when held, drop it the moment
/// it is released. Only from a grounded, non-attacking stance — you cannot
/// abandon a thrust halfway through by guarding.
fn update_block(
    mut commands: Commands,
    query: Query<
        (Entity, &Intent, &Grounded, Option<&Attacking>, Option<&Blocking>),
        With<Player>,
    >,
) {
    for (entity, intent, grounded, attacking, blocking) in &query {
        let wants_guard = intent.block_held && grounded.0 && attacking.is_none();
        match (wants_guard, blocking.is_some()) {
            (true, false) => {
                commands.entity(entity).insert(Blocking);
            }
            (false, true) => {
                commands.entity(entity).remove::<Blocking>();
            }
            _ => {}
        }
    }
}

fn start_attack(
    mut commands: Commands,
    query: Query<(Entity, &Intent), (With<Player>, Without<Attacking>, Without<Blocking>)>,
) {
    for (entity, intent) in &query {
        if intent.attack_pressed {
            commands.entity(entity).insert(Attacking {
                elapsed: 0.0,
                hitbox: None,
            });
        }
    }
}

fn tick_attack(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Attacking, &Facing), With<Player>>,
) {
    for (entity, mut attack, facing) in &mut query {
        attack.elapsed += time.delta_secs();

        match (attack.is_active(), attack.hitbox) {
            (true, None) => {
                let hitbox = commands
                    .spawn((
                        Name::new("Attack Hitbox"),
                        AttackHitbox,
                        Sprite::from_color(Color::srgba(1.0, 0.25, 0.25, 0.35), HITBOX_SIZE),
                        Transform::from_xyz(
                            facing.0 * HITBOX_FORWARD_OFFSET,
                            HITBOX_VERTICAL_OFFSET,
                            1.0,
                        ),
                        ChildOf(entity),
                    ))
                    .id();
                attack.hitbox = Some(hitbox);
            }
            (false, Some(hitbox)) => {
                commands.entity(hitbox).despawn();
                attack.hitbox = None;
            }
            _ => {}
        }

        if attack.elapsed >= ATTACK_DURATION {
            commands.entity(entity).remove::<Attacking>();
        }
    }
}

fn apply_physics(
    time: Res<Time>,
    mut query: Query<
        (
            &Intent,
            &Facing,
            Option<&Attacking>,
            Option<&Blocking>,
            &mut Velocity,
            &mut Grounded,
            &mut Transform,
        ),
        With<Player>,
    >,
) {
    let dt = time.delta_secs();

    for (intent, facing, attacking, blocking, mut velocity, mut grounded, mut transform) in
        &mut query
    {
        // Committing to a thrust takes over horizontal control: drive forward
        // through the strike, then plant for the recovery.
        velocity.0.x = match attacking {
            Some(attack) if attack.is_driving() => facing.0 * ATTACK_LUNGE_SPEED,
            Some(_) => 0.0,
            // Planting the shield roots you. Turning is still allowed, so you
            // can face an attacker without giving up the guard.
            None if blocking.is_some() => 0.0,
            None => intent.direction * RUN_SPEED,
        };

        if intent.jump_pressed && grounded.0 && attacking.is_none() && blocking.is_none() {
            velocity.0.y = JUMP_SPEED;
            grounded.0 = false;
        }

        // Variable jump height: releasing early clips the rest of the ascent.
        if !intent.jump_held && velocity.0.y > 0.0 {
            velocity.0.y *= JUMP_CUT_MULTIPLIER;
        }

        velocity.0.y += GRAVITY * dt;
        transform.translation += velocity.0.extend(0.0) * dt;

        let floor = GROUND_Y + HALF_HEIGHT;
        if transform.translation.y <= floor {
            transform.translation.y = floor;
            velocity.0.y = 0.0;
            grounded.0 = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            .add_plugins(PlayerPlugin);
        app.update(); // runs Startup, spawning the player
        app
    }

    fn hitbox_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<AttackHitbox>>()
            .iter(app.world())
            .count()
    }

    fn is_attacking(app: &mut App) -> bool {
        app.world_mut()
            .query_filtered::<Entity, With<Attacking>>()
            .iter(app.world())
            .next()
            .is_some()
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
        assert!(!is_attacking(&mut app), "should not start out attacking");

        tap_attack(&mut app);
        assert!(is_attacking(&mut app), "tapping J starts a thrust");

        let mut elapsed = DT;
        let mut hitbox_seen = false;

        // Step well past the full duration and check the window is respected.
        while elapsed < ATTACK_DURATION * 2.0 {
            let count = hitbox_count(&mut app);
            assert!(count <= 1, "at most one hitbox at a time, saw {count}");

            if count == 1 {
                hitbox_seen = true;
                assert!(
                    elapsed >= ATTACK_STARTUP,
                    "hitbox live at {elapsed}s, before startup ends at {ATTACK_STARTUP}s"
                );
                assert!(
                    elapsed < ATTACK_STARTUP + ATTACK_ACTIVE + DT,
                    "hitbox still live at {elapsed}s, past the active window"
                );
            }

            app.update();
            elapsed += DT;
        }

        assert!(hitbox_seen, "the active window never produced a hitbox");
        assert!(!is_attacking(&mut app), "attack should have ended by now");
        assert_eq!(hitbox_count(&mut app), 0, "hitbox outlived the attack");
    }

    #[test]
    fn attack_cannot_be_retriggered_mid_swing() {
        let mut app = test_app();

        // One tap to start, then mash during startup. Each `tap_attack` is also
        // one frame, so track the frame count to measure against the original
        // attack clock rather than the last button press.
        tap_attack(&mut app);
        for _ in 0..4 {
            tap_attack(&mut app);
        }
        let mut frames = 5;
        assert!(is_attacking(&mut app), "still mid-thrust after mashing");

        // Run to just past when a single, un-restarted thrust must have ended.
        // If mashing reset the clock, the attack outlives this deadline.
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

    fn set_key(app: &mut App, key: KeyCode, down: bool) {
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        if down {
            input.press(key);
        } else {
            input.release(key);
        }
    }

    fn is_blocking(app: &mut App) -> bool {
        app.world_mut()
            .query_filtered::<Entity, With<Blocking>>()
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

    #[test]
    fn shield_tracks_the_key() {
        let mut app = test_app();
        assert!(!is_blocking(&mut app));

        set_key(&mut app, KeyCode::KeyK, true);
        app.update();
        assert!(is_blocking(&mut app), "holding K raises the shield");

        // Still up while held.
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

        // Dropping the shield frees the attack again.
        set_key(&mut app, KeyCode::KeyK, false);
        app.update();
        tap_attack(&mut app);
        assert!(is_attacking(&mut app), "attack works once the guard is down");
    }

    #[test]
    fn blocking_roots_movement() {
        let mut app = test_app();

        // Confirm the movement key does move him first, so the assertion below
        // is about the guard rather than about input not working.
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

        // Face left, then attack and try to turn around mid-thrust.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyA);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyA);

        let facing_before = app
            .world_mut()
            .query_filtered::<&Facing, With<Player>>()
            .single(app.world())
            .unwrap()
            .0;
        assert_eq!(facing_before, -1.0, "holding A should face left");

        tap_attack(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);
        app.update();

        let facing_during = app
            .world_mut()
            .query_filtered::<&Facing, With<Player>>()
            .single(app.world())
            .unwrap()
            .0;
        assert_eq!(facing_during, -1.0, "facing must not flip mid-thrust");
    }
}

fn update_animation(
    mut query: Query<
        (
            &Velocity,
            &Grounded,
            &Facing,
            Option<&Attacking>,
            Option<&Blocking>,
            &mut AnimationClip,
            &mut AnimationTimer,
            &mut Sprite,
        ),
        With<Player>,
    >,
) {
    for (velocity, grounded, facing, attacking, blocking, mut clip, mut timer, mut sprite) in
        &mut query
    {
        sprite.flip_x = facing.0 < 0.0;

        let next = if attacking.is_some() {
            ATTACK_CLIP
        } else if blocking.is_some() {
            BLOCK_CLIP
        } else if !grounded.0 {
            AIRBORNE_CLIP
        } else if velocity.0.x != 0.0 {
            WALK_CLIP
        } else {
            IDLE_CLIP
        };

        play(&mut clip, &mut timer, &mut sprite, next);
    }
}
