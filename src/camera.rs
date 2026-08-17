//! Side-scrolling camera: follows the player horizontally, holds a fixed
//! vertical framing, and never looks past the edge of the level.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::level::Level;
use crate::player::Player;

/// World units visible top-to-bottom. The spartan is 60 units tall, so this
/// frames roughly six of him — architecture towers, which is the intent.
///
/// Matched to a level height of 22 tiles (352px) so a one-screen level fills
/// the view exactly, with no dead band above or below.
const VIEWPORT_HEIGHT: f32 = 352.0;
/// Higher is snappier. Framerate-independent via `exp` below.
const FOLLOW_SHARPNESS: f32 = 8.0;
/// Camera sits above the ground line so the floor isn't dead centre.
const VERTICAL_OFFSET: f32 = 60.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(PostUpdate, follow_player);
    }
}

#[derive(Component)]
struct MainCamera;

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Main Camera"),
        Camera2d,
        MainCamera,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEWPORT_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}

/// Keep the camera far enough inside `min..max` that the view never spills over
/// the edge. A level smaller than the view cannot satisfy that, so it is
/// centred instead — better a symmetric margin than a lopsided one.
fn clamp_view(target: f32, min: f32, max: f32, half_view: f32) -> f32 {
    let (low, high) = (min + half_view, max - half_view);
    if low > high {
        (min + max) / 2.0
    } else {
        target.clamp(low, high)
    }
}

fn follow_player(
    time: Res<Time>,
    level: Option<Res<Level>>,
    player: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    mut camera: Query<(&mut Transform, &Projection), With<MainCamera>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let Ok((mut camera, projection)) = camera.single_mut() else {
        return;
    };

    let mut target = Vec2::new(
        player.translation.x,
        crate::world::GROUND_Y + VERTICAL_OFFSET,
    );

    if let (Some(level), Projection::Orthographic(ortho)) = (level.as_deref(), projection) {
        // `area` is the visible rect around the camera, so its size is the
        // viewport in world units — correct at any window size or aspect.
        let half = ortho.area.size() / 2.0;
        let bounds = level.bounds();
        target.x = clamp_view(target.x, bounds.min.x, bounds.max.x, half.x);
        target.y = clamp_view(target.y, bounds.min.y, bounds.max.y, half.y);
    }

    // Exponential smoothing — the decay form keeps it identical at any framerate.
    let t = 1.0 - (-FOLLOW_SHARPNESS * time.delta_secs()).exp();
    camera.translation.x = camera.translation.x.lerp(target.x, t);
    camera.translation.y = camera.translation.y.lerp(target.y, t);
}

#[cfg(test)]
mod tests {
    use super::clamp_view;

    #[test]
    fn the_view_stays_inside_a_level_larger_than_itself() {
        // Level 0..640, view 200 wide.
        assert_eq!(clamp_view(320.0, 0.0, 640.0, 100.0), 320.0, "middle is free");
        assert_eq!(clamp_view(0.0, 0.0, 640.0, 100.0), 100.0, "held off the left edge");
        assert_eq!(clamp_view(640.0, 0.0, 640.0, 100.0), 540.0, "held off the right edge");
    }

    #[test]
    fn a_level_smaller_than_the_view_is_centred() {
        // Level 0..100, view 400 wide: no position hides the edges, so centre.
        assert_eq!(clamp_view(0.0, 0.0, 100.0, 200.0), 50.0);
        assert_eq!(clamp_view(999.0, 0.0, 100.0, 200.0), 50.0);
    }

    #[test]
    fn a_level_exactly_the_view_size_is_pinned() {
        assert_eq!(clamp_view(0.0, 0.0, 400.0, 200.0), 200.0);
        assert_eq!(clamp_view(400.0, 0.0, 400.0, 200.0), 200.0);
    }
}
