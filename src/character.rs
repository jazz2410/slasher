//! Everything a fighter shares: sprite setup, platformer physics, and the
//! animation state machine.
//!
//! Two axes of reuse live here. Along one, the player and the enemy are the
//! same creature and differ only in who fills [`Intent`] each frame — a
//! keyboard for one, a few AI rules for the other. Along the other, a *kind* of
//! fighter is pure data: [`CharacterKind`] carries its spritesheet, its clips,
//! its reach and its stats, so a new enemy with different art and a different
//! weapon is a new blueprint rather than a new system.

use bevy::prelude::*;

use crate::animation::{play, AnimationClip, AnimationTimer};
use crate::combat::{AttackHitbox, Hurt, Hurtbox};
use crate::world::GROUND_Y;

/// These belong to the level rather than to any fighter.
const GRAVITY: f32 = -1200.0;
/// Cutting upward velocity when the jump key is released gives short hops.
const JUMP_CUT_MULTIPLIER: f32 = 0.45;

// ---------------------------------------------------------------------------
// The spartan, as data. Copy this block to describe a different fighter.
// ---------------------------------------------------------------------------

/// `spartan_combat.png` is 540x256 — a 5x4 grid of 108x64 cells, stacked by
/// `tools/build_sheet.py` from the walk and attack sheets. Row 0 is the walk
/// cycle; rows 1-3 are one continuous 15-frame thrust. The cell is far wider
/// than the character so the spear has room to extend while the body stays
/// centred (which is what lets `flip_x` mirror in place).
const SPARTAN_SHEET: &str = "sprites/spartan_combat.png";
const SPARTAN_FRAME: UVec2 = UVec2::new(108, 64);
const SPARTAN_COLUMNS: u32 = 5;
const SPARTAN_ROWS: u32 = 4;
/// Frames in the thrust, spanning rows 1-3.
const SPARTAN_ATTACK_FRAMES: f32 = 15.0;

pub const SPARTAN_STATS: Stats = Stats {
    run_speed: 160.0,
    jump_speed: 420.0,
    // Modest forward drive so the thrust travels instead of rooting in place.
    lunge_speed: 90.0,
    attack: AttackTiming {
        startup: 0.09,
        active: 0.10,
        recovery: 0.13,
    },
};

/// Centre-to-feet: every frame is baselined 2px above the cell bottom.
pub const SPARTAN_BODY: Body = Body { half_height: 30.0 };

/// Measured off the thrust frames: at full extension the spear tip reaches 43px
/// forward of the sprite's centre and sits roughly level with it.
pub const SPARTAN_REACH: Reach = Reach {
    size: Vec2::new(34.0, 14.0),
    forward: 27.0,
    vertical: -2.0,
};

/// The body itself — narrower than a cell, since the cell is mostly reach.
pub const SPARTAN_HURTBOX: Vec2 = Vec2::new(26.0, 54.0);

pub const SPARTAN_CLIPS: Clips = Clips {
    idle: AnimationClip::still(0),
    walk: AnimationClip::new(0, 4, 0.09),
    airborne: AnimationClip::still(2),
    // Frame time derived from the attack's real duration, so the animation can
    // never drift out of sync with the hitbox timing.
    attack: AnimationClip::new(5, 19, SPARTAN_STATS.attack.total() / SPARTAN_ATTACK_FRAMES),
    // Stand-in. The current art has no dedicated guard, so this borrows the
    // thrust's opening frame — shield forward, spear braced — which at least
    // reads differently from idle. Replace when a block row exists.
    block: AnimationClip::still(5),
    // Likewise: without spark frames, the gold flash in `combat` is what tells
    // you the hit was turned aside.
    block_impact: AnimationClip::still(5),
};

// ---------------------------------------------------------------------------

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, CharacterSet::Control.before(CharacterSet::Physics));
        app.add_systems(PreStartup, load_kinds).add_systems(
            Update,
            (
                (update_block, start_attack, tick_attack)
                    .chain()
                    .in_set(CharacterSet::Control),
                apply_physics.in_set(CharacterSet::Physics),
                // Animation reads the state the simulation just produced.
                update_animation.after(CharacterSet::Physics),
            ),
        );
    }
}

/// Ordering seam for a frame of character simulation.
///
/// Controllers fill [`Intent`] before `Control`; combat resolves hits between
/// the two, so a knockback is integrated by `Physics` on the same frame it
/// lands rather than an unpredictable frame later.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CharacterSet {
    Control,
    Physics,
}

/// How long each phase of a thrust lasts.
#[derive(Clone, Copy, Debug)]
pub struct AttackTiming {
    /// Wind-up before the weapon commits.
    pub startup: f32,
    /// The window where the hitbox exists and can deal damage.
    pub active: f32,
    /// Tail where the fighter is committed and cannot act.
    pub recovery: f32,
}

impl AttackTiming {
    pub const fn total(self) -> f32 {
        self.startup + self.active + self.recovery
    }
}

/// Movement and attack numbers. Give a heavier enemy a slower run and a longer
/// recovery and he plays differently with no new code.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stats {
    pub run_speed: f32,
    pub jump_speed: f32,
    pub lunge_speed: f32,
    pub attack: AttackTiming,
}

/// Physical dimensions the ground and the sprite origin depend on.
#[derive(Component, Clone, Copy, Debug)]
pub struct Body {
    /// Distance from the sprite's centre down to the feet.
    pub half_height: f32,
}

/// Where a fighter's strike lands, relative to his own centre. The counterpart
/// to [`Hurtbox`], which describes what can be struck.
#[derive(Component, Clone, Copy, Debug)]
pub struct Reach {
    pub size: Vec2,
    /// Forward of centre, in the direction he faces.
    pub forward: f32,
    /// Above (positive) or below (negative) centre.
    pub vertical: f32,
}

impl Reach {
    /// The strike's world rectangle for a fighter at `centre` facing `facing`.
    pub fn rect(&self, centre: Vec2, facing: f32) -> Rect {
        Rect::from_center_size(
            centre + Vec2::new(facing * self.forward, self.vertical),
            self.size,
        )
    }
}

/// Which atlas frames stand for which state.
#[derive(Component, Clone, Copy, Debug)]
pub struct Clips {
    pub idle: AnimationClip,
    pub walk: AnimationClip,
    pub airborne: AnimationClip,
    pub attack: AnimationClip,
    pub block: AnimationClip,
    pub block_impact: AnimationClip,
}

/// A blueprint for one sort of fighter: art plus numbers, no behaviour.
#[derive(Clone)]
pub struct CharacterKind {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub clips: Clips,
    pub reach: Reach,
    pub hurtbox: Vec2,
    pub body: Body,
    pub stats: Stats,
}

/// Every blueprint the game knows about. Add a field per new fighter.
#[derive(Resource)]
pub struct Kinds {
    pub spartan: CharacterKind,
}

fn load_kinds(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.insert_resource(Kinds {
        spartan: CharacterKind {
            image: asset_server.load(SPARTAN_SHEET),
            layout: layouts.add(TextureAtlasLayout::from_grid(
                SPARTAN_FRAME,
                SPARTAN_COLUMNS,
                SPARTAN_ROWS,
                None,
                None,
            )),
            clips: SPARTAN_CLIPS,
            reach: SPARTAN_REACH,
            hurtbox: SPARTAN_HURTBOX,
            body: SPARTAN_BODY,
            stats: SPARTAN_STATS,
        },
    });
}

#[derive(Component)]
pub struct Character;

#[derive(Component, Default)]
pub struct Velocity(pub Vec2);

#[derive(Component, Default)]
pub struct Grounded(pub bool);

/// Which way the fighter faces: `1.0` right, `-1.0` left. Stored explicitly
/// rather than read back off `Sprite::flip_x` so an attack can aim correctly
/// even when started from a standstill.
#[derive(Component)]
pub struct Facing(pub f32);

/// The sprite's resting colour. Fighters may share a spritesheet, so a tint
/// tells them apart — which means "stop flashing" has to restore *this*, not
/// plain white.
#[derive(Component)]
pub struct BaseTint(pub Color);

/// What the controller decided this frame. Physics and animation read it.
#[derive(Component, Default)]
pub struct Intent {
    pub direction: f32,
    pub jump_pressed: bool,
    pub jump_held: bool,
    pub attack_pressed: bool,
    pub block_held: bool,
}

/// Present only while a thrust is in progress. It carries its own schedule, so
/// the attack stays authoritative over the animation rather than the reverse —
/// and so fighters with different timings can swing side by side.
#[derive(Component)]
pub struct Attacking {
    elapsed: f32,
    timing: AttackTiming,
    hitbox: Option<Entity>,
}

impl Attacking {
    fn is_active(&self) -> bool {
        self.elapsed >= self.timing.startup
            && self.elapsed < self.timing.startup + self.timing.active
    }

    fn is_driving(&self) -> bool {
        self.elapsed < self.timing.startup + self.timing.active
    }
}

/// Present while the shield is up. Held rather than timed, unlike [`Attacking`].
#[derive(Component)]
pub struct Blocking;

/// Spawn a fighter of `kind` at `x`, facing `facing`, tinted `tint`.
pub fn spawn_character(
    commands: &mut Commands,
    kind: &CharacterKind,
    name: &'static str,
    x: f32,
    facing: f32,
    tint: Color,
) -> Entity {
    let mut sprite = Sprite::from_atlas_image(
        kind.image.clone(),
        TextureAtlas {
            layout: kind.layout.clone(),
            index: 0,
        },
    );
    sprite.color = tint;

    commands
        .spawn((
            Name::new(name),
            Character,
            sprite,
            Transform::from_xyz(x, GROUND_Y + kind.body.half_height, 10.0),
            Velocity::default(),
            Grounded(true),
            Facing(facing),
            BaseTint(tint),
            Intent::default(),
            // The blueprint's data, copied onto the entity so each fighter
            // carries its own numbers.
            (
                Hurtbox(kind.hurtbox),
                kind.clips,
                kind.reach,
                kind.body,
                kind.stats,
            ),
            // Current animation state, seeded from the blueprint's clips.
            (kind.clips.idle, AnimationTimer::from_clip(&kind.clips.walk)),
        ))
        .id()
}

/// The shield tracks the key directly: up while held, down the moment it is
/// released. Only from a grounded, unhurt, non-attacking stance.
fn update_block(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &Intent,
            &Grounded,
            Option<&Attacking>,
            Option<&Hurt>,
            Option<&Blocking>,
        ),
        With<Character>,
    >,
) {
    for (entity, intent, grounded, attacking, hurt, blocking) in &query {
        let wants_guard =
            intent.block_held && grounded.0 && attacking.is_none() && hurt.is_none();
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
    query: Query<
        (Entity, &Intent, &Stats),
        (
            With<Character>,
            Without<Attacking>,
            Without<Blocking>,
            Without<Hurt>,
        ),
    >,
) {
    for (entity, intent, stats) in &query {
        if intent.attack_pressed {
            commands.entity(entity).insert(Attacking {
                elapsed: 0.0,
                timing: stats.attack,
                hitbox: None,
            });
        }
    }
}

fn tick_attack(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Attacking, &Facing, &Reach), With<Character>>,
) {
    for (entity, mut attack, facing, reach) in &mut query {
        attack.elapsed += time.delta_secs();

        match (attack.is_active(), attack.hitbox) {
            (true, None) => {
                let hitbox = commands
                    .spawn((
                        Name::new("Attack Hitbox"),
                        AttackHitbox::new(entity),
                        Sprite::from_color(Color::srgba(1.0, 0.25, 0.25, 0.30), reach.size),
                        Transform::from_xyz(facing.0 * reach.forward, reach.vertical, 1.0),
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

        if attack.elapsed >= attack.timing.total() {
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
            &Stats,
            &Body,
            Option<&Attacking>,
            Option<&Blocking>,
            Option<&Hurt>,
            &mut Velocity,
            &mut Grounded,
            &mut Transform,
        ),
        With<Character>,
    >,
) {
    let dt = time.delta_secs();

    for (
        intent,
        facing,
        stats,
        body,
        attacking,
        blocking,
        hurt,
        mut velocity,
        mut grounded,
        mut transform,
    ) in &mut query
    {
        if hurt.is_some() {
            // Hitstun: the knockback carries you, input does not. Decay is
            // exponential so it behaves the same at any framerate.
            velocity.0.x *= (-Hurt::KNOCKBACK_DAMPING * dt).exp();
        } else {
            velocity.0.x = match attacking {
                Some(attack) if attack.is_driving() => facing.0 * stats.lunge_speed,
                Some(_) => 0.0,
                // Planting the shield roots you. Turning is still allowed, so
                // you can face an attacker without giving up the guard.
                None if blocking.is_some() => 0.0,
                None => intent.direction * stats.run_speed,
            };

            if intent.jump_pressed && grounded.0 && attacking.is_none() && blocking.is_none() {
                velocity.0.y = stats.jump_speed;
                grounded.0 = false;
            }
        }

        // Variable jump height: releasing early clips the rest of the ascent.
        if !intent.jump_held && velocity.0.y > 0.0 {
            velocity.0.y *= JUMP_CUT_MULTIPLIER;
        }

        velocity.0.y += GRAVITY * dt;
        transform.translation += velocity.0.extend(0.0) * dt;

        let floor = GROUND_Y + body.half_height;
        if transform.translation.y <= floor {
            transform.translation.y = floor;
            velocity.0.y = 0.0;
            grounded.0 = true;
        }
    }
}

fn update_animation(
    mut query: Query<
        (
            &Velocity,
            &Grounded,
            &Facing,
            &Clips,
            Option<&Attacking>,
            Option<&Blocking>,
            Option<&Hurt>,
            &mut AnimationClip,
            &mut AnimationTimer,
            &mut Sprite,
        ),
        With<Character>,
    >,
) {
    for (
        velocity,
        grounded,
        facing,
        clips,
        attacking,
        blocking,
        hurt,
        mut clip,
        mut timer,
        mut sprite,
    ) in &mut query
    {
        sprite.flip_x = facing.0 < 0.0;

        let next = match hurt {
            // A turned-aside hit gets the impact pose; a clean hit has no
            // dedicated art, so the red flash carries it over the idle pose.
            Some(h) if h.blocked => clips.block_impact,
            Some(_) => clips.idle,
            None if attacking.is_some() => clips.attack,
            None if blocking.is_some() => clips.block,
            None if !grounded.0 => clips.airborne,
            None if velocity.0.x != 0.0 => clips.walk,
            None => clips.idle,
        };

        play(&mut clip, &mut timer, &mut sprite, next);
    }
}
