//! Side-scrolling camera: follows the player horizontally, holds a fixed
//! vertical framing so the level height stays readable.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::player::Player;

/// World units visible top-to-bottom. The sprite is 64 units tall, so this
/// frames roughly five-and-a-half spartans of headroom.
const VIEWPORT_HEIGHT: f32 = 360.0;
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

fn follow_player(
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };

    // Exponential smoothing — the decay form keeps it identical at any framerate.
    let t = 1.0 - (-FOLLOW_SHARPNESS * time.delta_secs()).exp();
    camera.translation.x = camera.translation.x.lerp(player.translation.x, t);
    camera.translation.y = camera
        .translation
        .y
        .lerp(crate::world::GROUND_Y + VERTICAL_OFFSET, t);
}
