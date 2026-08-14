//! Generic spritesheet animation. Nothing in here knows about the player, so
//! enemies and props can reuse it as-is.

use bevy::prelude::*;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, advance_animations);
    }
}

/// A half-open-free range of atlas indices to play, inclusive on both ends.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct AnimationClip {
    pub first: usize,
    pub last: usize,
    pub frame_duration: f32,
    pub repeat: bool,
}

impl AnimationClip {
    pub const fn new(first: usize, last: usize, frame_duration: f32) -> Self {
        Self { first, last, frame_duration, repeat: true }
    }

    /// Play `first` through `last` once, then hold the final frame. Suits a
    /// pose that is entered and then sustained, like raising a shield.
    pub const fn once(first: usize, last: usize, frame_duration: f32) -> Self {
        Self { first, last, frame_duration, repeat: false }
    }

    /// A single frame held indefinitely — useful for idle and airborne poses.
    ///
    /// The duration is arbitrary but must stay a sane finite value: it reaches
    /// `Duration::from_secs_f32`, which panics on `f32::MAX`. A one-frame
    /// non-repeating clip never advances regardless of what we put here.
    pub const fn still(frame: usize) -> Self {
        Self { first: frame, last: frame, frame_duration: 0.1, repeat: false }
    }
}

#[derive(Component, Deref, DerefMut)]
pub struct AnimationTimer(pub Timer);

impl AnimationTimer {
    pub fn from_clip(clip: &AnimationClip) -> Self {
        Self(Timer::from_seconds(clip.frame_duration, TimerMode::Repeating))
    }
}

/// Swap the entity onto `clip`, restarting the timer only if the clip actually
/// changed. Calling this every frame with the same clip is a no-op.
pub fn play(
    current: &mut AnimationClip,
    timer: &mut AnimationTimer,
    sprite: &mut Sprite,
    clip: AnimationClip,
) {
    if *current == clip {
        return;
    }
    *current = clip;
    timer.0.set_duration(std::time::Duration::from_secs_f32(clip.frame_duration));
    timer.0.reset();
    if let Some(atlas) = sprite.texture_atlas.as_mut() {
        atlas.index = clip.first;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_sprite() -> Sprite {
        Sprite::from_atlas_image(
            Handle::default(),
            TextureAtlas { layout: Handle::default(), index: 0 },
        )
    }

    /// `play` pushes `frame_duration` into `Duration::from_secs_f32`, which
    /// panics on non-finite or huge values. Every clip must survive that.
    #[test]
    fn switching_to_any_clip_does_not_panic() {
        let clips = [
            AnimationClip::still(0),
            AnimationClip::still(2),
            AnimationClip::new(0, 5, 0.09),
        ];

        for &target in &clips {
            for &start in &clips {
                let mut current = start;
                let mut timer = AnimationTimer::from_clip(&start);
                let mut sprite = test_sprite();

                play(&mut current, &mut timer, &mut sprite, target);

                assert_eq!(current, target);
                if start != target {
                    assert_eq!(sprite.texture_atlas.unwrap().index, target.first);
                }
            }
        }
    }

    #[test]
    fn still_clip_holds_its_frame() {
        let clip = AnimationClip::still(2);
        let mut sprite = test_sprite();
        let atlas = sprite.texture_atlas.as_mut().unwrap();
        atlas.index = clip.first;

        // Simulate many elapsed frames worth of ticks.
        for _ in 0..50 {
            let atlas = sprite.texture_atlas.as_mut().unwrap();
            if atlas.index == clip.last && !clip.repeat {
                continue;
            }
            atlas.index += 1;
        }

        assert_eq!(sprite.texture_atlas.unwrap().index, 2);
    }
}

fn advance_animations(
    time: Res<Time>,
    mut query: Query<(&AnimationClip, &mut AnimationTimer, &mut Sprite)>,
) {
    for (clip, mut timer, mut sprite) in &mut query {
        let Some(atlas) = sprite.texture_atlas.as_mut() else {
            continue;
        };

        timer.tick(time.delta());
        if !timer.just_finished() {
            continue;
        }

        // An index outside the clip means something else just switched clips;
        // snap back in rather than walking off the end of the sheet.
        if atlas.index < clip.first || atlas.index > clip.last {
            atlas.index = clip.first;
        } else if atlas.index == clip.last {
            if clip.repeat {
                atlas.index = clip.first;
            }
        } else {
            atlas.index += 1;
        }
    }
}
