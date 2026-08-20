//! Hits: what they cost, and how a spartan shows he felt one.
//!
//! A clean strike takes health, staggers the victim and flashes him red. A
//! guarded strike costs nothing — it sparks off the shield and shoves him a
//! little. The shield only covers the side he faces, so turning your back on a
//! thrust means eating it in full.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::character::{
    Attacking, BaseTint, Blocking, Casting, Character, CharacterSet, Dying, Facing, Reach,
    Velocity,
};

/// How long a clean hit takes the victim out of the fight.
const HURT_DURATION: f32 = 0.34;
/// A guarded hit costs far less — that is the point of guarding.
const BLOCKED_DURATION: f32 = 0.16;
const KNOCKBACK_SPEED: f32 = 240.0;
const BLOCKED_KNOCKBACK: f32 = 80.0;
/// Blinks per second while hurt. Fast enough to read as damage, slow enough to
/// see at 60fps.
const FLASH_HZ: f32 = 12.0;
const HURT_FLASH: Color = Color::srgb(3.0, 0.6, 0.6);
const BLOCK_FLASH: Color = Color::srgb(2.4, 2.2, 1.2);

/// A spear thrust costs a quarter of a full bar, so a duel runs four hits.
#[cfg(test)]
pub const SPEAR_DAMAGE: f32 = crate::character::PLAYER_ATTACK_DAMAGE;
#[cfg(test)]
pub const MAX_HEALTH: f32 = crate::character::PLAYER_MAX_HEALTH;

/// How long after raising the shield a guard still counts as a parry.
///
/// Long enough to be learnable, short enough that it cannot be held. A guard
/// that never goes stale makes turtling the correct answer to every attack,
/// which is the same as having no fight at all.
pub const PARRY_WINDOW: f32 = 0.18;
/// A stale guard still turns the blade, but the shock gets through.
pub const CHIP_DAMAGE: f32 = 8.0;
/// What a parry costs the attacker: long enough to walk in and punish.
const PARRY_STAGGER: f32 = 0.5;

/// Floating bar above each fighter. Deliberately small — it should read at a
/// glance without competing with the character.
const BAR_SIZE: Vec2 = Vec2::new(38.0, 4.0);
/// Clear of the helmet crest.
const BAR_OFFSET_Y: f32 = 44.0;
const BAR_BACKING: Color = Color::srgba(0.04, 0.03, 0.03, 0.85);
/// Bronze at full, blood at empty — palette colours, so it belongs to the world.
const BAR_FULL: Color = Color::srgb(0.659, 0.498, 0.271);
const BAR_EMPTY: Color = Color::srgb(0.639, 0.110, 0.094);

pub struct CombatPlugin;

/// Ordering seam for starting and advancing death animations.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CombatSet {
    Death,
}

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (resolve_hits, tick_hurt)
                .chain()
                .after(CharacterSet::Control)
                .before(CharacterSet::Physics),
        )
        // After physics, so a bar never lags the body it belongs to.
        .add_systems(
            Update,
            (update_health_bars, begin_deaths, tick_deaths)
                .chain()
                .in_set(CombatSet::Death)
                .after(CharacterSet::Physics),
        );
    }
}

/// The region of a body that can be struck.
#[derive(Component)]
pub struct Hurtbox(pub Vec2);

/// What a fighter has left.
#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn full(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            (self.current / self.max).clamp(0.0, 1.0)
        }
    }

    pub fn is_spent(&self) -> bool {
        self.current <= 0.0
    }
}

/// The draining part of a health bar. Its parent is the fighter it belongs to.
#[derive(Component)]
pub struct HealthBarFill;

/// Damage belongs to the attacking character blueprint, allowing the player
/// and every enemy type to be tuned independently.
#[derive(Component, Clone, Copy, Debug)]
pub struct AttackDamage(pub f32);

/// Attach a bar to a fighter. Two child sprites: a fixed dark backing and a
/// fill that shrinks. Children of the character, so they follow it for free —
/// and `flip_x` is a sprite property, not a transform, so facing left does not
/// mirror the bar.
pub fn spawn_health_bar(commands: &mut Commands, owner: Entity) {
    commands.entity(owner).with_children(|parent| {
        parent.spawn((
            Name::new("Health Bar Backing"),
            Sprite::from_color(BAR_BACKING, BAR_SIZE + Vec2::splat(2.0)),
            Transform::from_xyz(0.0, BAR_OFFSET_Y, 20.0),
        ));
        parent.spawn((
            Name::new("Health Bar Fill"),
            HealthBarFill,
            Sprite::from_color(BAR_FULL, BAR_SIZE),
            Transform::from_xyz(0.0, BAR_OFFSET_Y, 21.0),
        ));
    });
}

/// The damaging region of a thrust, spawned as a child of its owner for the
/// active window only.
#[derive(Component)]
pub struct AttackHitbox {
    pub owner: Entity,
    /// One thrust may strike a given target only once, however many frames the
    /// active window lasts.
    struck: HashSet<Entity>,
}

impl AttackHitbox {
    pub fn new(owner: Entity) -> Self {
        Self {
            owner,
            struck: HashSet::new(),
        }
    }
}

/// Present while reeling from a hit. Blocks input, drives the flash.
#[derive(Component)]
pub struct Hurt {
    timer: Timer,
    pub blocked: bool,
}

impl Hurt {
    /// Exponential decay applied to knockback velocity, in units per second.
    pub const KNOCKBACK_DAMPING: f32 = 5.0;

    fn of(seconds: f32, blocked: bool) -> Self {
        Self {
            timer: Timer::from_seconds(seconds, TimerMode::Once),
            blocked,
        }
    }

    /// Took it on the body.
    pub(crate) fn wounded() -> Self {
        Self::of(HURT_DURATION, false)
    }

    /// Took it on the shield.
    pub(crate) fn guarded() -> Self {
        Self::of(BLOCKED_DURATION, true)
    }

    /// Had his thrust turned aside — the attacker's punishment for a parry.
    fn staggered() -> Self {
        Self::of(PARRY_STAGGER, true)
    }
}

/// What a strike did when it landed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Guard {
    /// Nothing in the way.
    Open,
    /// Shield up, but the guard had gone stale — some damage gets through.
    Stale,
    /// Shield raised inside the window, and facing the blow.
    Parried,
}

/// Decide what a guard was worth against a blow from the front or behind.
///
/// Pulled out of the system so the rule can be tested on its own — it is the
/// heart of the fight, and the thing most likely to be tuned.
pub fn guard_against(guard: Option<&Blocking>, facing_the_blow: bool) -> Guard {
    match guard {
        // The shield covers the side he faces. Turning your back on a thrust
        // means taking it in full, however well timed the button was.
        Some(guard) if facing_the_blow => {
            if guard.elapsed <= PARRY_WINDOW {
                Guard::Parried
            } else {
                Guard::Stale
            }
        }
        _ => Guard::Open,
    }
}

impl Guard {
    pub(crate) fn damage(self, attack_damage: f32) -> f32 {
        match self {
            Guard::Open => attack_damage,
            Guard::Stale => CHIP_DAMAGE,
            Guard::Parried => 0.0,
        }
    }

    pub(crate) fn knockback(self) -> f32 {
        match self {
            Guard::Open => KNOCKBACK_SPEED,
            _ => BLOCKED_KNOCKBACK,
        }
    }
}

fn resolve_hits(
    mut commands: Commands,
    mut hitboxes: Query<&mut AttackHitbox>,
    owners: Query<(&Transform, &Facing, &Reach, &AttackDamage), With<Character>>,
    mut targets: Query<
        (
            Entity,
            &Transform,
            &Hurtbox,
            &Facing,
            Option<&Blocking>,
            &mut Velocity,
        ),
        With<Character>,
    >,
    mut healths: Query<&mut Health>,
) {
    for mut hitbox in &mut hitboxes {
        let Ok((owner_transform, owner_facing, reach, attack_damage)) = owners.get(hitbox.owner)
        else {
            continue;
        };

        // Derived from the owner rather than the hitbox entity's own transform:
        // as a child, its GlobalTransform is only propagated in PostUpdate and
        // would be a frame stale here.
        let spear = reach.rect(owner_transform.translation.truncate(), owner_facing.0);

        for (target, transform, hurtbox, facing, blocking, mut velocity) in &mut targets {
            if target == hitbox.owner || hitbox.struck.contains(&target) {
                continue;
            }

            let body = Rect::from_center_size(transform.translation.truncate(), hurtbox.0);
            if spear.intersect(body).is_empty() {
                continue;
            }

            let from_attacker = transform.translation.x - owner_transform.translation.x;
            let away = if from_attacker == 0.0 {
                owner_facing.0
            } else {
                from_attacker.signum()
            };
            let guard = guard_against(blocking, facing.0 * away < 0.0);

            hitbox.struck.insert(target);
            velocity.0.x = away * guard.knockback();
            commands.entity(target).insert(if guard == Guard::Open {
                Hurt::wounded()
            } else {
                Hurt::guarded()
            });

            let damage = guard.damage(attack_damage.0);
            if damage > 0.0 {
                if let Ok(mut health) = healths.get_mut(target) {
                    health.current = (health.current - damage).max(0.0);
                }
            }

            // A parry turns the fight around: the attacker is staggered long
            // enough to be walked into and punished. That reward is what makes
            // timing the shield worth the risk of holding it too early.
            if guard == Guard::Parried {
                commands.entity(hitbox.owner).insert(Hurt::staggered());
            }
        }
    }
}

fn tick_hurt(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Hurt, &mut Sprite, &crate::character::BaseTint)>,
) {
    for (entity, mut hurt, mut sprite, base) in &mut query {
        hurt.timer.tick(time.delta());

        if hurt.timer.is_finished() {
            sprite.color = base.0;
            commands.entity(entity).remove::<Hurt>();
            continue;
        }

        // Square wave rather than a fade: an on/off blink reads as an impact,
        // where a smooth ramp reads as a status effect.
        let lit = (hurt.timer.elapsed_secs() * FLASH_HZ).fract() < 0.5;
        sprite.color = match (lit, hurt.blocked) {
            (true, true) => BLOCK_FLASH,
            (true, false) => HURT_FLASH,
            (false, _) => base.0,
        };
    }
}

/// Resize each bar to its owner's remaining health.
///
/// The fill drains from the right: its left edge stays put while the sprite
/// narrows, which is why the x offset moves as the width shrinks.
fn update_health_bars(
    fighters: Query<&Health, With<Character>>,
    mut bars: Query<(&ChildOf, &mut Sprite, &mut Transform), With<HealthBarFill>>,
) {
    for (child_of, mut sprite, mut transform) in &mut bars {
        let Ok(health) = fighters.get(child_of.parent()) else {
            continue;
        };
        let fraction = health.fraction();
        let width = BAR_SIZE.x * fraction;

        sprite.custom_size = Some(Vec2::new(width, BAR_SIZE.y));
        transform.translation.x = -(BAR_SIZE.x - width) / 2.0;
        sprite.color = BAR_EMPTY.mix(&BAR_FULL, fraction);
    }
}

/// Turn a lethal hit into a state instead of removing the fighter immediately.
/// Any attack already in flight is cancelled so a corpse cannot keep hitting.
fn begin_deaths(
    mut commands: Commands,
    mut fighters: Query<
        (Entity, &Health, Option<&Attacking>, Option<&mut Sprite>, &BaseTint),
        (With<Character>, Without<Dying>),
    >,
) {
    for (entity, health, attacking, sprite, base) in &mut fighters {
        if !health.is_spent() {
            continue;
        }

        if let Some(mut sprite) = sprite {
            sprite.color = base.0;
        }
        if let Some(hitbox) = attacking.and_then(Attacking::hitbox) {
            commands.entity(hitbox).despawn();
        }
        commands
            .entity(entity)
            .remove::<(Attacking, Blocking, Casting, Hurt, Hurtbox)>()
            .insert(Dying::new());
    }
}

/// Advance the death clock. The non-repeating animation holds its final frame,
/// so defeated enemies remain as harmless corpses until the run is restarted.
fn tick_deaths(time: Res<Time>, mut fighters: Query<&mut Dying>) {
    for mut dying in &mut fighters {
        dying.elapsed += time.delta_secs();
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn guard_aged(seconds: f32) -> Blocking {
        Blocking { elapsed: seconds }
    }

    #[test]
    fn a_fresh_guard_facing_the_blow_parries() {
        assert_eq!(
            guard_against(Some(&guard_aged(0.0)), true),
            Guard::Parried
        );
        assert_eq!(
            guard_against(Some(&guard_aged(PARRY_WINDOW)), true),
            Guard::Parried,
            "the window is inclusive at its edge"
        );
    }

    #[test]
    fn a_guard_held_too_long_goes_stale() {
        assert_eq!(
            guard_against(Some(&guard_aged(PARRY_WINDOW + 0.01)), true),
            Guard::Stale
        );
        assert_eq!(guard_against(Some(&guard_aged(5.0)), true), Guard::Stale);
    }

    #[test]
    fn a_guard_turned_the_wrong_way_is_no_guard() {
        // Perfect timing does not help a man facing away from the spear.
        assert_eq!(guard_against(Some(&guard_aged(0.0)), false), Guard::Open);
        assert_eq!(guard_against(None, true), Guard::Open);
    }

    #[test]
    fn each_outcome_costs_what_it_should() {
        assert_eq!(Guard::Open.damage(SPEAR_DAMAGE), SPEAR_DAMAGE);
        assert_eq!(Guard::Stale.damage(SPEAR_DAMAGE), CHIP_DAMAGE);
        assert_eq!(Guard::Parried.damage(SPEAR_DAMAGE), 0.0);
        assert!(
            Guard::Stale.damage(SPEAR_DAMAGE) < Guard::Open.damage(SPEAR_DAMAGE),
            "a stale guard must still be better than none"
        );
    }
}
