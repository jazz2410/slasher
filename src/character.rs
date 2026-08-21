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
use crate::combat::{
    spawn_health_bar, AttackDamage, AttackHitbox, CombatSet, Health, Hurt, Hurtbox,
};
use crate::level::Level;
use crate::run::run_scoped;
use crate::world::GROUND_Y;

/// These belong to the level rather than to any fighter.
const GRAVITY: f32 = -1200.0;
/// Cutting upward velocity when the jump key is released gives short hops.
const JUMP_CUT_MULTIPLIER: f32 = 0.45;

// ---------------------------------------------------------------------------
// Fighter art, as data. Copy a block to describe a different fighter.
// ---------------------------------------------------------------------------

/// The player's atlas is built from the four action rows in
/// `spartan_sprites.png`. The combat sheet contains walk, thrust, and special;
/// death stays separate so changing art cannot disturb a corpse mid-animation.
const PLAYER_SHEET: &str = "sprites/player_combat.png";
const PLAYER_FRAME: UVec2 = UVec2::new(128, 96);
const PLAYER_COLUMNS: u32 = 6;
const PLAYER_ROWS: u32 = 5;
const PLAYER_DEATH_SHEET: &str = "sprites/player_dies_game.png";
const PLAYER_DEATH_FRAME: UVec2 = UVec2::new(128, 96);
const PLAYER_DEATH_COLUMNS: u32 = 6;
const PLAYER_DEATH_ROWS: u32 = 1;
const PLAYER_ACTION_FRAMES: f32 = 6.0;

/// The standard enemy's new sheet has six walking frames followed by six sword
/// attack frames. Death remains a separate six-frame row.
const ENEMY_STANDARD_SHEET: &str = "sprites/enemy_standard_combat.png";
const ENEMY_STANDARD_FRAME: UVec2 = UVec2::new(128, 96);
const ENEMY_STANDARD_COLUMNS: u32 = 6;
const ENEMY_STANDARD_ROWS: u32 = 2;
const ENEMY_STANDARD_DEATH_SHEET: &str = "sprites/enemy_standard_dies_game.png";
const ENEMY_STANDARD_DEATH_FRAME: UVec2 = UVec2::new(128, 96);
const ENEMY_STANDARD_DEATH_COLUMNS: u32 = 6;
const ENEMY_STANDARD_DEATH_ROWS: u32 = 1;
const ENEMY_STANDARD_ACTION_FRAMES: f32 = 6.0;

/// The archer uses the same five-column convention, but his standing frames
/// need two extra pixels of headroom for the raised bow.
const ARCHER_SHEET: &str = "sprites/archer_combat.png";
const ARCHER_FRAME: UVec2 = UVec2::new(108, 66);
const ARCHER_COLUMNS: u32 = 5;
const ARCHER_ROWS: u32 = 10;
const ARCHER_DEATH_SHEET: &str = "sprites/archer_dies_game.png";
const ARCHER_DEATH_FRAME: UVec2 = UVec2::new(108, 96);
const ARCHER_DEATH_COLUMNS: u32 = 5;
const ARCHER_DEATH_ROWS: u32 = 5;
const ARCHER_ANIMATION_FRAMES: f32 = 25.0;

pub const PLAYER_MAX_HEALTH: f32 = 100.0;
pub const PLAYER_ATTACK_DAMAGE: f32 = 25.0;
pub const ENEMY_STANDARD_MAX_HEALTH: f32 = 100.0;
pub const ENEMY_STANDARD_ATTACK_DAMAGE: f32 = 25.0;
pub const ARCHER_MAX_HEALTH: f32 = 100.0;
pub const ARCHER_ATTACK_DAMAGE: f32 = 25.0;

pub const PLAYER_STATS: Stats = Stats {
    run_speed: 160.0,
    jump_speed: 420.0,
    // Keep the player planted during the supplied thrust animation. The spear
    // reaches through its hitbox; moving the body here makes the feet slide.
    lunge_speed: 0.0,
    attack: AttackTiming {
        startup: 0.21,
        active: 0.14,
        recovery: 0.35,
    },
};

/// Kept separate even while the numbers match, so tuning this enemy never
/// changes the player by accident.
pub const ENEMY_STANDARD_STATS: Stats = Stats {
    run_speed: 160.0,
    jump_speed: 420.0,
    lunge_speed: 90.0,
    attack: AttackTiming {
        startup: 0.09,
        active: 0.10,
        recovery: 0.13,
    },
};

/// Kept separate from the melee enemy so the archer can be tuned without
/// changing any other fighter. His attack duration drives the bow animation.
pub const ARCHER_STATS: Stats = Stats {
    run_speed: 105.0,
    jump_speed: 420.0,
    lunge_speed: 0.0,
    attack: AttackTiming {
        startup: 0.38,
        active: 0.10,
        recovery: 0.32,
    },
};

/// Centre-to-feet: every frame is baselined 2px above the cell bottom. The
/// collider is narrower than the art so he fits through a one-tile gap.
pub const SPARTAN_BODY: Body = Body {
    half_height: 30.0,
    half_width: 9.0,
};

/// The archer art is intentionally about 10% smaller than the Spartans.
pub const ARCHER_BODY: Body = Body {
    half_height: 27.0,
    half_width: 8.0,
};

/// Measured off the thrust frames: at full extension the spear tip reaches 43px
/// forward of the sprite's centre and sits roughly level with it.
pub const SPARTAN_REACH: Reach = Reach {
    size: Vec2::new(34.0, 14.0),
    forward: 27.0,
    vertical: -2.0,
};

/// The body itself — narrower than a cell, since the cell is mostly reach.
pub const SPARTAN_HURTBOX: Vec2 = Vec2::new(26.0, 54.0);
pub const ARCHER_HURTBOX: Vec2 = Vec2::new(23.0, 49.0);

const fn enemy_standard_clips(stats: Stats) -> Clips {
    Clips {
        idle: AnimationClip::still(0),
        walk: AnimationClip::new(0, 5, 0.065),
        airborne: AnimationClip::still(0),
        attack: AnimationClip::once(
            6,
            11,
            stats.attack.total() / ENEMY_STANDARD_ACTION_FRAMES,
        ),
        block: AnimationClip::still(6),
        block_impact: AnimationClip::still(6),
        cast: AnimationClip::still(0),
        death: AnimationClip::once(
            0,
            5,
            DEATH_DURATION / ENEMY_STANDARD_ACTION_FRAMES,
        ),
    }
}

pub const PLAYER_CLIPS: Clips = Clips {
    // A restrained breathing cycle: six frames over 1.5 seconds.
    idle: AnimationClip::new(18, 23, 0.25),
    // Six frames at this cadence cover roughly one 62px stride at 160px/s.
    // A slower cycle makes the planted feet skate across the ground.
    walk: AnimationClip::new(0, 5, 0.065),
    airborne: AnimationClip::once(24, 29, 0.10),
    attack: AnimationClip::once(
        6,
        11,
        PLAYER_STATS.attack.total() / PLAYER_ACTION_FRAMES,
    ),
    block: AnimationClip::still(6),
    block_impact: AnimationClip::still(6),
    // The third source row is the shrine-granted special sequence.
    cast: AnimationClip::once(12, 17, CAST_DURATION / PLAYER_ACTION_FRAMES),
    death: AnimationClip::once(0, 5, DEATH_DURATION / PLAYER_ACTION_FRAMES),
};
pub const ENEMY_STANDARD_CLIPS: Clips = enemy_standard_clips(ENEMY_STANDARD_STATS);

pub const ARCHER_CLIPS: Clips = Clips {
    idle: AnimationClip::still(0),
    walk: AnimationClip::new(0, 24, 0.07),
    airborne: AnimationClip::still(2),
    attack: AnimationClip::once(
        25,
        49,
        ARCHER_STATS.attack.total() / ARCHER_ANIMATION_FRAMES,
    ),
    // The archer cannot guard or cast; these are safe fallback poses required
    // by the shared animation contract.
    block: AnimationClip::still(0),
    block_impact: AnimationClip::still(0),
    cast: AnimationClip::still(0),
    death: AnimationClip::once(0, 24, DEATH_DURATION / ARCHER_ANIMATION_FRAMES),
};

/// How long the player is committed to a shrine-granted cast.
///
/// Paced to the 25-frame throw animation at ~32ms a frame: long enough to show
/// the wind-up and release rather than reading as a second sword swing.
pub const CAST_DURATION: f32 = 0.8;
/// Time given to the complete fall-and-smoke animation before an enemy is
/// removed or the player's run ends.
pub const DEATH_DURATION: f32 = 1.0;

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
                update_animation
                    .after(CharacterSet::Physics)
                    .after(CombatSet::Death),
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

/// The collision box, as half-extents from the sprite's centre.
#[derive(Component, Clone, Copy, Debug)]
pub struct Body {
    /// Distance from the sprite's centre down to the feet.
    pub half_height: f32,
    pub half_width: f32,
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
    /// The shrine-granted attack.
    pub cast: AnimationClip,
    pub death: AnimationClip,
}

/// The normal and death atlases carried by every fighter. Death is separate
/// because its prone spear frames are wider than every standing action.
#[derive(Component, Clone)]
pub struct CharacterArt {
    combat_image: Handle<Image>,
    combat_layout: Handle<TextureAtlasLayout>,
    death_image: Handle<Image>,
    death_layout: Handle<TextureAtlasLayout>,
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
    pub max_health: f32,
    pub attack_damage: f32,
    pub attack_style: AttackStyle,
    pub death_image: Handle<Image>,
    pub death_layout: Handle<TextureAtlasLayout>,
}

/// Every blueprint the game knows about. Add a field per new fighter.
#[derive(Resource)]
pub struct Kinds {
    pub player: CharacterKind,
    pub enemy_standard: CharacterKind,
    pub archer: CharacterKind,
}

fn load_kinds(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let player_image = asset_server.load(PLAYER_SHEET);
    let player_layout = layouts.add(TextureAtlasLayout::from_grid(
        PLAYER_FRAME,
        PLAYER_COLUMNS,
        PLAYER_ROWS,
        None,
        None,
    ));
    let player_death_image = asset_server.load(PLAYER_DEATH_SHEET);
    let player_death_layout = layouts.add(TextureAtlasLayout::from_grid(
        PLAYER_DEATH_FRAME,
        PLAYER_DEATH_COLUMNS,
        PLAYER_DEATH_ROWS,
        None,
        None,
    ));
    let standard_image = asset_server.load(ENEMY_STANDARD_SHEET);
    let standard_layout = layouts.add(TextureAtlasLayout::from_grid(
        ENEMY_STANDARD_FRAME,
        ENEMY_STANDARD_COLUMNS,
        ENEMY_STANDARD_ROWS,
        None,
        None,
    ));
    let standard_death_image = asset_server.load(ENEMY_STANDARD_DEATH_SHEET);
    let standard_death_layout = layouts.add(TextureAtlasLayout::from_grid(
        ENEMY_STANDARD_DEATH_FRAME,
        ENEMY_STANDARD_DEATH_COLUMNS,
        ENEMY_STANDARD_DEATH_ROWS,
        None,
        None,
    ));
    let archer_image = asset_server.load(ARCHER_SHEET);
    let archer_layout = layouts.add(TextureAtlasLayout::from_grid(
        ARCHER_FRAME,
        ARCHER_COLUMNS,
        ARCHER_ROWS,
        None,
        None,
    ));
    let archer_death_image = asset_server.load(ARCHER_DEATH_SHEET);
    let archer_death_layout = layouts.add(TextureAtlasLayout::from_grid(
        ARCHER_DEATH_FRAME,
        ARCHER_DEATH_COLUMNS,
        ARCHER_DEATH_ROWS,
        None,
        None,
    ));

    let enemy_standard = CharacterKind {
        image: standard_image,
        layout: standard_layout,
        clips: ENEMY_STANDARD_CLIPS,
        reach: SPARTAN_REACH,
        hurtbox: SPARTAN_HURTBOX,
        body: SPARTAN_BODY,
        stats: ENEMY_STANDARD_STATS,
        max_health: ENEMY_STANDARD_MAX_HEALTH,
        attack_damage: ENEMY_STANDARD_ATTACK_DAMAGE,
        attack_style: AttackStyle::Melee,
        death_image: standard_death_image,
        death_layout: standard_death_layout,
    };

    commands.insert_resource(Kinds {
        player: CharacterKind {
            image: player_image,
            layout: player_layout,
            clips: PLAYER_CLIPS,
            reach: SPARTAN_REACH,
            hurtbox: SPARTAN_HURTBOX,
            body: SPARTAN_BODY,
            stats: PLAYER_STATS,
            max_health: PLAYER_MAX_HEALTH,
            attack_damage: PLAYER_ATTACK_DAMAGE,
            attack_style: AttackStyle::Melee,
            death_image: player_death_image,
            death_layout: player_death_layout,
        },
        enemy_standard,
        archer: CharacterKind {
            image: archer_image,
            layout: archer_layout,
            clips: ARCHER_CLIPS,
            reach: Reach {
                size: Vec2::ZERO,
                forward: 0.0,
                vertical: 0.0,
            },
            hurtbox: ARCHER_HURTBOX,
            body: ARCHER_BODY,
            stats: ARCHER_STATS,
            max_health: ARCHER_MAX_HEALTH,
            attack_damage: ARCHER_ATTACK_DAMAGE,
            attack_style: AttackStyle::Ranged,
            death_image: archer_death_image,
            death_layout: archer_death_layout,
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

/// Whether the shared attack state creates a local hitbox or lets an enemy
/// controller loose a projectile during its active window.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttackStyle {
    Melee,
    Ranged,
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
    pub(crate) fn is_active(&self) -> bool {
        self.elapsed >= self.timing.startup
            && self.elapsed < self.timing.startup + self.timing.active
    }

    fn is_driving(&self) -> bool {
        self.elapsed < self.timing.startup + self.timing.active
    }

    pub(crate) fn hitbox(&self) -> Option<Entity> {
        self.hitbox
    }
}

/// Present while loosing a shrine-granted arrow. Like [`Attacking`] it carries
/// its own clock, so the commitment is the same length whatever the art does.
#[derive(Component)]
pub struct Casting {
    pub elapsed: f32,
    /// The arrow leaves the hand partway through, not on the first frame.
    pub loosed: bool,
}

/// Present from the lethal blow until the final death frame has been shown.
#[derive(Component)]
pub struct Dying {
    pub elapsed: f32,
}

impl Dying {
    pub fn new() -> Self {
        Self { elapsed: 0.0 }
    }

    pub fn is_done(&self) -> bool {
        self.elapsed >= DEATH_DURATION
    }
}

impl Casting {
    pub fn new() -> Self {
        Self {
            elapsed: 0.0,
            loosed: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.elapsed >= CAST_DURATION
    }
}

/// Present while the shield is up. Held rather than timed, unlike [`Attacking`].
///
/// It records *how long* it has been up, because a guard is only a parry while
/// it is fresh. Without that, holding the shield down would be a free answer to
/// everything and there would be no fight to have.
#[derive(Component, Default)]
pub struct Blocking {
    pub elapsed: f32,
}

/// Spawn a fighter of `kind` at `x`, facing `facing`, tinted `tint`.
pub fn spawn_character(
    commands: &mut Commands,
    kind: &CharacterKind,
    name: &'static str,
    feet: Vec2,
    facing: f32,
    tint: Color,
) -> Entity {
    let mut sprite = Sprite::from_atlas_image(
        kind.image.clone(),
        TextureAtlas {
            layout: kind.layout.clone(),
            index: kind.clips.idle.first,
        },
    );
    sprite.color = tint;

    let entity = commands
        .spawn((
            Name::new(name),
            Character,
            sprite,
            Transform::from_xyz(feet.x, feet.y + kind.body.half_height, 10.0),
            Velocity::default(),
            Grounded(true),
            Facing(facing),
            BaseTint(tint),
            CharacterArt {
                combat_image: kind.image.clone(),
                combat_layout: kind.layout.clone(),
                death_image: kind.death_image.clone(),
                death_layout: kind.death_layout.clone(),
            },
            Intent::default(),
            // The blueprint's data, copied onto the entity so each fighter
            // carries its own numbers.
            (
                Hurtbox(kind.hurtbox),
                kind.clips,
                kind.reach,
                kind.body,
                kind.stats,
                kind.attack_style,
            ),
            // Current animation state, seeded from the blueprint's clips.
            (kind.clips.idle, AnimationTimer::from_clip(&kind.clips.idle)),
            Health::full(kind.max_health),
            AttackDamage(kind.attack_damage),
            run_scoped(),
        ))
        .id();

    spawn_health_bar(commands, entity);
    entity
}

/// The shield tracks the key directly: up while held, down the moment it is
/// released. Only from a grounded, unhurt, non-attacking stance.
fn update_block(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<
        (
            Entity,
            &Intent,
            &Grounded,
            Option<&Attacking>,
            Option<&Hurt>,
            Option<&mut Blocking>,
        ),
        (With<Character>, Without<Dying>),
    >,
) {
    for (entity, intent, grounded, attacking, hurt, blocking) in &mut query {
        let wants_guard =
            intent.block_held && grounded.0 && attacking.is_none() && hurt.is_none();
        match (wants_guard, blocking) {
            (true, None) => {
                commands.entity(entity).insert(Blocking::default());
            }
            // Age the guard. Raising it again after a release starts a fresh
            // window, which is what makes timing a block worth doing.
            (true, Some(mut guard)) => guard.elapsed += time.delta_secs(),
            (false, Some(_)) => {
                commands.entity(entity).remove::<Blocking>();
            }
            (false, None) => {}
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
            Without<Dying>,
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
    mut query: Query<
        (Entity, &mut Attacking, &Facing, &Reach, &AttackStyle),
        With<Character>,
    >,
) {
    for (entity, mut attack, facing, reach, style) in &mut query {
        attack.elapsed += time.delta_secs();

        match (style, attack.is_active(), attack.hitbox) {
            (AttackStyle::Melee, true, None) => {
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
            (_, false, Some(hitbox)) => {
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
    level: Option<Res<Level>>,
    mut query: Query<
        (
            &Intent,
            &Facing,
            &Stats,
            &Body,
            Option<&Attacking>,
            Option<&Blocking>,
            Option<&Hurt>,
            Option<&Casting>,
            Option<&Dying>,
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
        casting,
        dying,
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
                _ if dying.is_some() => 0.0,
                _ if casting.is_some() => 0.0,
                Some(attack) if attack.is_driving() => facing.0 * stats.lunge_speed,
                Some(_) => 0.0,
                // Planting the shield roots you. Turning is still allowed, so
                // you can face an attacker without giving up the guard.
                None if blocking.is_some() => 0.0,
                None => intent.direction * stats.run_speed,
            };

            if intent.jump_pressed
                && grounded.0
                && attacking.is_none()
                && blocking.is_none()
                && casting.is_none()
                && dying.is_none()
            {
                velocity.0.y = stats.jump_speed;
                grounded.0 = false;
            }
        }

        // Variable jump height: releasing early clips the rest of the ascent.
        if !intent.jump_held && velocity.0.y > 0.0 {
            velocity.0.y *= JUMP_CUT_MULTIPLIER;
        }

        velocity.0.y += GRAVITY * dt;

        let half = Vec2::new(body.half_width, body.half_height);
        let mut centre = transform.translation.truncate();
        // Where the feet were before moving — a one-way platform only catches a
        // body that was already above it.
        let previous_bottom = centre.y - half.y;

        // Axis-separated: move and resolve horizontally, then vertically.
        // Resolving both at once cannot tell a wall from a floor.
        centre.x += velocity.0.x * dt;
        if let Some(level) = level.as_deref() {
            level.resolve_horizontal(&mut centre, half, &mut velocity.0.x);
        }

        centre.y += velocity.0.y * dt;
        grounded.0 = match level.as_deref() {
            Some(level) => {
                level.resolve_vertical(&mut centre, half, &mut velocity.0.y, previous_bottom)
            }
            // No level loaded — flat ground at GROUND_Y, which is what the
            // tests run against.
            None => {
                let floor = GROUND_Y + half.y;
                if centre.y <= floor {
                    centre.y = floor;
                    velocity.0.y = 0.0;
                    true
                } else {
                    false
                }
            }
        };

        transform.translation.x = centre.x;
        transform.translation.y = centre.y;
    }
}

fn update_animation(
    mut query: Query<
        (
            &Velocity,
            &Grounded,
            &Facing,
            &Clips,
            &CharacterArt,
            Option<&Attacking>,
            Option<&Blocking>,
            Option<&Hurt>,
            Option<&Casting>,
            Option<&Dying>,
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
        art,
        attacking,
        blocking,
        hurt,
        casting,
        dying,
        mut clip,
        mut timer,
        mut sprite,
    ) in &mut query
    {
        sprite.flip_x = facing.0 < 0.0;

        let death_art = dying.is_some();
        sprite.image = if death_art {
            art.death_image.clone()
        } else {
            art.combat_image.clone()
        };
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.layout = if death_art {
                art.death_layout.clone()
            } else {
                art.combat_layout.clone()
            };
        }

        let next = match hurt {
            _ if dying.is_some() => clips.death,
            // A turned-aside hit gets the impact pose; a clean hit has no
            // dedicated art, so the red flash carries it over the idle pose.
            Some(h) if h.blocked => clips.block_impact,
            Some(_) => clips.idle,
            None if casting.is_some() => clips.cast,
            None if attacking.is_some() => clips.attack,
            None if blocking.is_some() => clips.block,
            None if !grounded.0 => clips.airborne,
            None if velocity.0.x != 0.0 => clips.walk,
            None => clips.idle,
        };

        play(&mut clip, &mut timer, &mut sprite, next);
    }
}
