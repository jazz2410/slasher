//! A shrine, the blessing it grants, and the arrow that blessing looses.
//!
//! The shape of it: a shrine stands in the level, the player interacts with it
//! once, and from then on a second attack is available on its own button. That
//! attack looses a fire arrow which flies forward and kills the first thing it
//! touches.
//!
//! A shrine's gift is a single charge: loose the arrow and the fire is gone,
//! and you cannot cast again until another shrine answers. Some levels instead
//! leave the fire burning permanently — an `EternalFlame` marker anywhere in
//! the level grants the same state with no charge to spend.
//!
//! The blessing is scoped to the run either way, so dying costs it. That is
//! deliberate: it makes reaching the shrine part of the level rather than a
//! permanent upgrade, which is what suits levels you retry.

use bevy::prelude::*;

use crate::character::{
    Attacking, BaseTint, Blocking, Casting, Character, Dying, Facing, CAST_DURATION,
};
use crate::combat::{Health, Hurt, Hurtbox};
use crate::level::Level;
use crate::player::Player;
use crate::run::{run_scoped, Run};

/// LDtk entity names accepted for a shrine. The old `FIreShrine` typo remains
/// accepted so older levels do not silently lose their shrine.
const SHRINE_MARKERS: [&str; 3] = ["FireShrine", "FIreShrine", "Shrine"];
/// LDtk entity name that, placed anywhere in a level, leaves the player
/// permanently able to cast. Position is irrelevant — its presence is the
/// whole statement.
const ETERNAL_MARKER: &str = "EternalFlame";

/// Development switch for testing the cast without a shrine. Now that the
/// level contains a real FireShrine it stays off during normal play.
const START_BLESSED: bool = false;

/// How close the player must stand to hear the god.
const INTERACT_RANGE: f32 = 44.0;
/// Fraction of the cast the arrow leaves on — not the first frame, so the
/// animation reads as a draw and a release.
const LOOSE_AT: f32 = 0.5;
/// Breathing room between arrows where the fire never runs out. A single
/// charge has nothing to wait for — there is only ever one.
const ARROW_COOLDOWN: f32 = 2.5;

const ARROW_SPEED: f32 = 430.0;
/// The spear art, at the size `tools/import_weapon.py` produced it for. Drawing
/// it larger than this would only soften it.
const ARROW_SPRITE_PATH: &str = "weapons/fire_spear.png";
const ARROW_SPRITE: Vec2 = Vec2::new(64.0, 20.0);
/// What actually connects — deliberately smaller than the picture, so a hit
/// lands when the burning head reaches you rather than when the haft does.
const ARROW_SIZE: Vec2 = Vec2::new(46.0, 10.0);
/// Launch offset from the caster's centre, far enough forward that the whole
/// spear clears him.
const ARROW_OFFSET: Vec2 = Vec2::new(34.0, 6.0);

const SHRINE_SPRITE_PATH: &str = "levels/FireShrine.png";
/// Small enough to sit cleanly on Level_1's 56px-wide platform.
const SHRINE_SIZE: Vec2 = Vec2::new(48.0, 48.0);
#[cfg(test)]
const SHRINE_DORMANT: Color = Color::srgb(0.353, 0.310, 0.271);
const SHRINE_BLESSED: Color = Color::srgb(0.847, 0.698, 0.404);
const SHRINE_SPENT: Color = Color::srgb(0.62, 0.56, 0.52);
const HINT_Y: f32 = SHRINE_SIZE.y / 2.0 + 9.0;
/// Only the tests draw an arrow as a flat colour now; the game uses the art.
#[cfg(test)]
const ARROW_COLOUR: Color = Color::srgb(0.910, 0.518, 0.173);
/// While charged, the spartan carries the god's fire on him. Tinting the base
/// colour rather than the sprite means a damage flash still restores *to* this,
/// so the state survives being hit.
const BLESSED_TINT: Color = Color::srgb(1.25, 0.95, 0.70);

pub struct ShrinePlugin;

impl Plugin for ShrinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(Run::Playing),
            (spawn_shrines, spawn_blessed_indicator),
        )
        .add_systems(
            Update,
            (
                grant_standing_blessing,
                highlight_shrines,
                interact,
                begin_cast,
                tick_cast,
                update_blessed_indicator,
                fly_arrows,
            )
                .chain()
                .run_if(in_state(Run::Playing)),
        );
    }
}

#[derive(Component)]
pub struct Shrine {
    pub spent: bool,
}

/// Small floating diamond that marks an unspent shrine as interactable.
#[derive(Component)]
struct ShrineHint {
    active: bool,
}

/// Temporary top-centre UI badge. Its simple nested squares can be replaced by
/// an image later without changing any blessing logic.
#[derive(Component)]
struct BlessedIndicator;

/// How much fire the blessing holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Charges {
    /// What a shrine gives: one arrow, then the state is gone.
    Single,
    /// What an `EternalFlame` level gives: it never runs out.
    Endless,
}

/// The god's favour. Being *in this state* is what allows the cast at all;
/// without it the button does nothing.
#[derive(Component)]
pub struct Blessed {
    pub charges: Charges,
    cooldown: Timer,
    /// The colour to hand back when the fire leaves him.
    restore_tint: Color,
}

impl Blessed {
    fn new(charges: Charges, restore_tint: Color) -> Self {
        // Starts ready: the first arrow is available the moment you are blessed.
        let mut cooldown = Timer::from_seconds(ARROW_COOLDOWN, TimerMode::Once);
        cooldown.set_elapsed(cooldown.duration());
        Self {
            charges,
            cooldown,
            restore_tint,
        }
    }
}

/// Put the fire on him, or take it off.
fn set_alight(sprite: &mut Sprite, base: &mut BaseTint, colour: Color) {
    base.0 = colour;
    sprite.color = colour;
}

/// Levels carrying an `EternalFlame` marker keep the player permanently able to
/// cast. Runs every frame rather than on spawn so it cannot lose a race with
/// the player being created; `Without<Blessed>` makes it idempotent.
fn grant_standing_blessing(
    mut commands: Commands,
    level: Option<Res<Level>>,
    mut player: Query<(Entity, &mut Sprite, &mut BaseTint), (With<Player>, Without<Blessed>)>,
) {
    let holy_ground = level
        .as_deref()
        .is_some_and(|level| level.all_spawns(ETERNAL_MARKER).next().is_some());
    if !holy_ground && !START_BLESSED {
        return;
    }
    for (entity, mut sprite, mut base) in &mut player {
        let restore = base.0;
        set_alight(&mut sprite, &mut base, BLESSED_TINT);
        commands
            .entity(entity)
            .insert(Blessed::new(Charges::Endless, restore));
        info!("this ground is holy: the fire does not go out");
    }
}

#[derive(Component)]
pub struct FireArrow {
    /// Never kills the one who loosed it.
    owner: Entity,
    velocity: Vec2,
}

fn spawn_shrines(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    level: Option<Res<Level>>,
) {
    let Some(level) = level else { return };
    let image = asset_server.load(SHRINE_SPRITE_PATH);
    let placements: Vec<Vec2> = SHRINE_MARKERS
        .iter()
        .flat_map(|marker| level.all_spawns(marker))
        .collect();

    for at in placements {
        let shrine = commands
            .spawn((
                Name::new("Fire Shrine"),
                Shrine { spent: false },
                run_scoped(),
                Sprite {
                    image: image.clone(),
                    custom_size: Some(SHRINE_SIZE),
                    ..default()
                },
                // `at` is the marker's feet, so lift it by half its height.
                Transform::from_xyz(at.x, at.y + SHRINE_SIZE.y / 2.0, 5.0),
            ))
            .id();

        commands.spawn((
            Name::new("Fire Shrine Interaction Hint"),
            ShrineHint { active: true },
            Sprite::from_color(Color::srgb(1.35, 0.82, 0.20), Vec2::splat(6.0)),
            Transform::from_xyz(0.0, HINT_Y, 1.0)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
            ChildOf(shrine),
        ));
    }
}

fn spawn_blessed_indicator(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Fire Blessing Indicator"),
            BlessedIndicator,
            run_scoped(),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(14.0),
                width: Val::Percent(100.0),
                height: Val::Px(38.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: Val::Px(36.0),
                        height: Val::Px(36.0),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(7.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.30, 0.07, 0.025, 0.92)),
                    BorderColor::all(Color::srgb(1.0, 0.48, 0.10)),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Node {
                            width: Val::Px(14.0),
                            height: Val::Px(20.0),
                            border_radius: BorderRadius::all(Val::Percent(45.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(1.0, 0.66, 0.12)),
                    ));
                });
        });
}

fn update_blessed_indicator(
    blessed_player: Query<(), (With<Player>, With<Blessed>)>,
    mut indicators: Query<&mut Visibility, With<BlessedIndicator>>,
) {
    let visibility = if blessed_player.is_empty() {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for mut indicator in &mut indicators {
        *indicator = visibility;
    }
}

/// Warm the shrine gently at a distance and more strongly when the player is
/// close enough to press E. The floating diamond supplies the interaction cue
/// without requiring a font or UI overlay.
fn highlight_shrines(
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<Shrine>)>,
    mut shrines: Query<
        (&Shrine, &Transform, &mut Sprite),
        (Without<Player>, Without<ShrineHint>),
    >,
    mut hints: Query<
        (&ShrineHint, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<Shrine>, Without<Player>),
    >,
) {
    let player_at = player.single().ok().map(|transform| transform.translation.truncate());
    let pulse = (time.elapsed_secs() * 3.5).sin() * 0.5 + 0.5;

    for (shrine, transform, mut sprite) in &mut shrines {
        if shrine.spent {
            sprite.color = SHRINE_SPENT;
            continue;
        }

        let near = player_at.is_some_and(|player| {
            player.distance(transform.translation.truncate()) <= INTERACT_RANGE
        });
        let warmth = 0.06 + pulse * 0.07 + if near { 0.16 } else { 0.0 };
        sprite.color = Color::srgb(1.0 + warmth, 1.0 + warmth * 0.55, 1.0 - warmth * 0.25);
    }

    for (hint, mut transform, mut sprite, mut visibility) in &mut hints {
        *visibility = if hint.active {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        transform.translation.y = HINT_Y + pulse * 2.5;
        let size = 0.9 + pulse * 0.2;
        transform.scale = Vec3::splat(size);
        sprite.color = Color::srgba(1.35, 0.82, 0.20, 0.65 + pulse * 0.35);
    }
}

/// Step up to a shrine and press the interact key.
fn interact(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut player: Query<(Entity, &Transform, &mut Sprite, &mut BaseTint), With<Player>>,
    mut shrines: Query<(Entity, &mut Shrine, &Transform, &mut Sprite), Without<Player>>,
    mut hints: Query<(&ChildOf, &mut ShrineHint)>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let Ok((player, player_transform, mut sprite, mut base)) = player.single_mut() else {
        return;
    };
    let here = player_transform.translation.truncate();

    for (shrine_entity, mut shrine, transform, mut shrine_sprite) in &mut shrines {
        if shrine.spent {
            continue;
        }
        if here.distance(transform.translation.truncate()) > INTERACT_RANGE {
            continue;
        }

        shrine.spent = true;
        shrine_sprite.color = SHRINE_BLESSED;
        for (child_of, mut hint) in &mut hints {
            if child_of.parent() == shrine_entity {
                hint.active = false;
            }
        }
        let restore = base.0;
        set_alight(&mut sprite, &mut base, BLESSED_TINT);
        commands
            .entity(player)
            .insert(Blessed::new(Charges::Single, restore));
        info!("the shrine answers: one arrow is yours");
        return;
    }
}

/// Loose an arrow, if the god has been asked and the last one has cooled.
fn begin_cast(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut player: Query<
        (Entity, &mut Blessed),
        (
            With<Player>,
            Without<Casting>,
            Without<Attacking>,
            Without<Blocking>,
            Without<Hurt>,
            Without<Dying>,
        ),
    >,
) {
    for (entity, mut blessed) in &mut player {
        blessed.cooldown.tick(time.delta());
        if keys.just_pressed(KeyCode::KeyL) && blessed.cooldown.is_finished() {
            blessed.cooldown.reset();
            commands.entity(entity).insert(Casting::new());
        }
    }
}

/// Advance a cast, releasing the arrow partway through.
fn tick_cast(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    mut casters: Query<(
        Entity,
        &mut Casting,
        &Transform,
        &Facing,
        Option<&mut Blessed>,
        Option<&mut Sprite>,
        Option<&mut BaseTint>,
    )>,
) {
    for (entity, mut cast, transform, facing, blessed, sprite, base) in &mut casters {
        cast.elapsed += time.delta_secs();

        if !cast.loosed && cast.elapsed >= CAST_DURATION * LOOSE_AT {
            cast.loosed = true;
            let from = transform.translation.truncate()
                + Vec2::new(facing.0 * ARROW_OFFSET.x, ARROW_OFFSET.y);
            commands.spawn((
                Name::new("Fire Arrow"),
                FireArrow {
                    owner: entity,
                    velocity: Vec2::new(facing.0 * ARROW_SPEED, 0.0),
                },
                run_scoped(),
                Sprite {
                    image: asset_server.load(ARROW_SPRITE_PATH),
                    custom_size: Some(ARROW_SPRITE),
                    // The art points right; mirror it when it flies the other way.
                    flip_x: facing.0 < 0.0,
                    ..default()
                },
                Transform::from_xyz(from.x, from.y, 9.0),
            ));

            // The fire leaves with the arrow. A shrine's gift is spent here;
            // an eternal flame only starts cooling down.
            match blessed {
                Some(blessed) if blessed.charges == Charges::Single => {
                    if let (Some(mut sprite), Some(mut base)) = (sprite, base) {
                        let restore = blessed.restore_tint;
                        set_alight(&mut sprite, &mut base, restore);
                    }
                    commands.entity(entity).remove::<Blessed>();
                }
                Some(mut blessed) => blessed.cooldown.reset(),
                None => {}
            }
        }

        if cast.is_done() {
            commands.entity(entity).remove::<Casting>();
        }
    }
}

/// Fly, and kill the first body in the way.
///
/// Flat flight, no gravity: it reads as a bolt of fire rather than a thrown
/// object, and it means the arrow's path is exactly where you aimed it.
fn fly_arrows(
    mut commands: Commands,
    time: Res<Time>,
    level: Option<Res<Level>>,
    mut arrows: Query<(Entity, &FireArrow, &mut Transform)>,
    mut targets: Query<(Entity, &Transform, &Hurtbox, &mut Health), (With<Character>, Without<FireArrow>)>,
) {
    let dt = time.delta_secs();

    for (entity, arrow, mut transform) in &mut arrows {
        transform.translation += arrow.velocity.extend(0.0) * dt;
        let at = transform.translation.truncate();

        // Into a wall, or off the edge of the world.
        if let Some(level) = level.as_deref() {
            let bounds = level.bounds();
            if level.is_solid_at(at) || !bounds.contains(at) {
                commands.entity(entity).despawn();
                continue;
            }
        }

        let head = Rect::from_center_size(at, ARROW_SIZE);
        for (target, target_transform, hurtbox, mut health) in &mut targets {
            if target == arrow.owner {
                continue;
            }
            let body =
                Rect::from_center_size(target_transform.translation.truncate(), hurtbox.0);
            if head.intersect(body).is_empty() {
                continue;
            }

            // The god's arrow does not wound. It kills.
            health.current = 0.0;
            commands.entity(entity).despawn();
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::CharacterPlugin;
    use crate::combat::{CombatPlugin, MAX_HEALTH};
    use crate::enemy::{EnemyPlugin, EnemyStandard};
    use crate::player::PlayerPlugin;
    use crate::run::RunPlugin;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    const DT: f32 = 1.0 / 60.0;

    /// No `LevelPlugin`: fighters fall back to flat ground and fixed spawns,
    /// which keeps these tests about the shrine rather than about level data.
    fn shrine_app() -> App {
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
                RunPlugin,
                CharacterPlugin,
                CombatPlugin,
                PlayerPlugin,
                EnemyPlugin,
                ShrinePlugin,
            ));
        app.update();
        app.update();
        app
    }

    fn player_at(app: &mut App) -> Vec2 {
        app.world_mut()
            .query_filtered::<&Transform, With<Player>>()
            .single(app.world())
            .unwrap()
            .translation
            .truncate()
    }

    fn place_shrine(app: &mut App, at: Vec2) {
        app.world_mut().spawn((
            Shrine { spent: false },
            Sprite::from_color(SHRINE_DORMANT, SHRINE_SIZE),
            Transform::from_xyz(at.x, at.y, 5.0),
        ));
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.release(key);
        input.clear();
    }

    fn charges(app: &mut App) -> Option<Charges> {
        app.world_mut()
            .query_filtered::<&Blessed, With<Player>>()
            .iter(app.world())
            .next()
            .map(|b| b.charges)
    }

    fn is_blessed(app: &mut App) -> bool {
        app.world_mut()
            .query_filtered::<Entity, (With<Blessed>, With<Player>)>()
            .iter(app.world())
            .next()
            .is_some()
    }

    fn blessing_indicator_visible(app: &mut App) -> bool {
        app.world_mut()
            .query_filtered::<&Visibility, With<BlessedIndicator>>()
            .single(app.world())
            .is_ok_and(|visibility| *visibility == Visibility::Visible)
    }

    fn arrows(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<FireArrow>>()
            .iter(app.world())
            .count()
    }

    fn enemies(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, (With<EnemyStandard>, Without<Dying>)>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn stepping_up_to_a_shrine_grants_the_blessing() {
        let mut app = shrine_app();
        let here = player_at(&mut app);
        place_shrine(&mut app, here);
        assert!(!is_blessed(&mut app));
        assert!(!blessing_indicator_visible(&mut app));

        tap(&mut app, KeyCode::KeyE);
        assert!(is_blessed(&mut app), "asking at the altar should be answered");
        assert!(blessing_indicator_visible(&mut app));
    }

    #[test]
    fn a_shrine_out_of_reach_grants_nothing() {
        let mut app = shrine_app();
        let here = player_at(&mut app);
        place_shrine(&mut app, here + Vec2::new(INTERACT_RANGE * 3.0, 0.0));

        tap(&mut app, KeyCode::KeyE);
        assert!(!is_blessed(&mut app), "a distant god does not hear");
    }

    #[test]
    fn the_arrow_needs_the_blessing() {
        let mut app = shrine_app();
        tap(&mut app, KeyCode::KeyL);
        for _ in 0..60 {
            app.update();
        }
        assert_eq!(arrows(&mut app), 0, "an unblessed spartan looses nothing");
    }

    #[test]
    fn a_blessed_cast_looses_one_arrow() {
        let mut app = shrine_app();
        let here = player_at(&mut app);
        place_shrine(&mut app, here);
        tap(&mut app, KeyCode::KeyE);

        tap(&mut app, KeyCode::KeyL);
        let mut seen = 0;
        for _ in 0..(CAST_DURATION / DT).ceil() as usize + 4 {
            app.update();
            seen = seen.max(arrows(&mut app));
        }
        assert_eq!(seen, 1, "one cast, one arrow");
    }

    #[test]
    fn the_arrow_kills_the_first_enemy_it_touches() {
        let mut app = shrine_app();
        let here = player_at(&mut app);
        place_shrine(&mut app, here);
        tap(&mut app, KeyCode::KeyE);
        assert_eq!(enemies(&mut app), 1);

        tap(&mut app, KeyCode::KeyL);
        let mut killed = false;
        for _ in 0..240 {
            app.update();
            if enemies(&mut app) == 0 {
                killed = true;
                break;
            }
        }
        assert!(killed, "the arrow never reached him");
        assert_eq!(arrows(&mut app), 0, "the arrow should be spent on the kill");
    }

    /// Sat directly on top of its owner, an arrow must still ignore him.
    ///
    /// A cast arrow spawns ahead of the caster and moves before its first
    /// collision check, so in normal play it clears him on frame one and the
    /// owner rule is never reached. Testing this through a real cast therefore
    /// proves nothing about the rule — it only proves the spawn offset. So the
    /// arrow is placed right on him instead, barely moving.
    #[test]
    fn the_arrow_spares_the_one_who_loosed_it() {
        let mut app = shrine_app();
        let player = app
            .world_mut()
            .query_filtered::<Entity, With<Player>>()
            .single(app.world())
            .unwrap();
        let here = player_at(&mut app);

        app.world_mut().spawn((
            FireArrow {
                owner: player,
                velocity: Vec2::new(1.0, 0.0),
            },
            Sprite::from_color(ARROW_COLOUR, ARROW_SIZE),
            Transform::from_xyz(here.x, here.y, 9.0),
        ));

        for _ in 0..20 {
            app.update();
            let health = app
                .world_mut()
                .query_filtered::<&Health, With<Player>>()
                .iter(app.world())
                .next()
                .map(|h| h.current);
            assert_eq!(
                health,
                Some(MAX_HEALTH),
                "the arrow must pass through its own caster"
            );
        }
    }

    /// Arrows despawn once they fly off, so comparing raw counts across a cast
    /// is unreliable. Clear the field first and the count afterwards is exactly
    /// "did this attempt produce an arrow".
    fn clear_arrows(app: &mut App) {
        let existing: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<FireArrow>>()
            .iter(app.world())
            .collect();
        for arrow in existing {
            app.world_mut().despawn(arrow);
        }
    }

    /// Casting is refused while hurt, and by the time a multi-round test gets
    /// going the enemy has closed in. These tests are about the charge, not the
    /// fight, so take him off the board.
    fn clear_enemies(app: &mut App) {
        let existing: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<EnemyStandard>>()
            .iter(app.world())
            .collect();
        for enemy in existing {
            app.world_mut().despawn(enemy);
        }
    }

    fn cast_and_settle(app: &mut App) {
        tap(app, KeyCode::KeyL);
        for _ in 0..(CAST_DURATION / DT).ceil() as usize + 2 {
            app.update();
        }
    }

    /// A shrine's gift is one arrow. Loosing it leaves the state.
    #[test]
    fn a_shrine_grants_exactly_one_arrow() {
        let mut app = shrine_app();
        clear_enemies(&mut app);
        let here = player_at(&mut app);
        place_shrine(&mut app, here);
        tap(&mut app, KeyCode::KeyE);
        assert!(is_blessed(&mut app));

        cast_and_settle(&mut app);
        assert!(
            !is_blessed(&mut app),
            "the fire should leave with the arrow"
        );
        assert!(!blessing_indicator_visible(&mut app));

        // Ask again with nothing left to give.
        clear_arrows(&mut app);
        cast_and_settle(&mut app);
        assert_eq!(arrows(&mut app), 0, "a spent spartan cannot cast again");
    }

    /// ...and a second shrine re-arms him.
    #[test]
    fn another_shrine_restores_the_charge() {
        let mut app = shrine_app();
        clear_enemies(&mut app);
        let here = player_at(&mut app);
        place_shrine(&mut app, here);
        tap(&mut app, KeyCode::KeyE);
        cast_and_settle(&mut app);
        assert!(!is_blessed(&mut app));

        let now = player_at(&mut app);
        place_shrine(&mut app, now);
        tap(&mut app, KeyCode::KeyE);
        assert!(is_blessed(&mut app), "a fresh shrine should answer again");
    }

    /// A level carrying an `EternalFlame` marker never spends the charge.
    #[test]
    fn an_eternal_flame_never_runs_out() {
        let mut app = shrine_app();
        clear_enemies(&mut app);
        app.world_mut()
            .insert_resource(crate::level::Level::with_spawns(vec![
                crate::level::SpawnPoint {
                    identifier: ETERNAL_MARKER.to_string(),
                    at: Vec2::ZERO,
                },
            ]));
        app.update();
        assert!(is_blessed(&mut app), "holy ground should light him up");
        // Assert the *kind*, not just the effect. `grant_standing_blessing`
        // re-grants every frame, so a single charge that is spent and
        // immediately restored would look endless from the outside.
        assert_eq!(
            charges(&mut app),
            Some(Charges::Endless),
            "holy ground should grant an endless charge, not a re-granted single"
        );

        for round in 0..3 {
            clear_arrows(&mut app);
            cast_and_settle(&mut app);
            assert_eq!(
                arrows(&mut app),
                1,
                "round {round}: the fire should not run out"
            );
            assert!(is_blessed(&mut app), "round {round}: still blessed");
            // Wait out the cooldown before asking again.
            for _ in 0..(ARROW_COOLDOWN / DT).ceil() as usize + 2 {
                app.update();
            }
        }
    }

    /// The cooldown only means anything where the fire is endless.
    #[test]
    fn an_eternal_flame_still_has_a_cooldown() {
        let mut app = shrine_app();
        clear_enemies(&mut app);
        app.world_mut()
            .insert_resource(crate::level::Level::with_spawns(vec![
                crate::level::SpawnPoint {
                    identifier: ETERNAL_MARKER.to_string(),
                    at: Vec2::ZERO,
                },
            ]));
        app.update();

        cast_and_settle(&mut app);

        // Straight back on the button, well inside the cooldown.
        clear_arrows(&mut app);
        cast_and_settle(&mut app);
        assert_eq!(
            arrows(&mut app),
            0,
            "a second arrow came before the fire had cooled"
        );
    }
}
