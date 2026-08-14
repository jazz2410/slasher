//! Hits, and how a spartan shows he felt one.
//!
//! There is no health and no bar — a strike reads purely through the reaction:
//! the victim flinches, gets knocked back, and flashes. A guarded hit instead
//! throws sparks off the shield and barely moves him.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::character::{Blocking, Character, CharacterSet, Facing, Reach, Velocity};

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

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (resolve_hits, tick_hurt)
                .chain()
                .after(CharacterSet::Control)
                .before(CharacterSet::Physics),
        );
    }
}

/// The region of a body that can be struck.
#[derive(Component)]
pub struct Hurtbox(pub Vec2);

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

    fn new(blocked: bool) -> Self {
        let seconds = if blocked { BLOCKED_DURATION } else { HURT_DURATION };
        Self {
            timer: Timer::from_seconds(seconds, TimerMode::Once),
            blocked,
        }
    }
}

fn resolve_hits(
    mut commands: Commands,
    mut hitboxes: Query<&mut AttackHitbox>,
    owners: Query<(&Transform, &Facing, &Reach), With<Character>>,
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
) {
    for mut hitbox in &mut hitboxes {
        let Ok((owner_transform, owner_facing, reach)) = owners.get(hitbox.owner) else {
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
            // The shield only covers the side he is facing: turning your back
            // on a thrust means eating it.
            let guarded = blocking.is_some() && facing.0 * away < 0.0;

            hitbox.struck.insert(target);
            velocity.0.x = away * if guarded { BLOCKED_KNOCKBACK } else { KNOCKBACK_SPEED };
            commands.entity(target).insert(Hurt::new(guarded));
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
